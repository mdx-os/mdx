//! Native Postgres transport for the serving path.
//!
//! Live Postgres writes used to spawn a `psql` subprocess per export: a
//! process fork on every governed write, no prepared connection, and a hard
//! runtime dependency on the psql binary being installed next to the server.
//! This module holds one persistent connection behind a mutex and executes
//! the same rendered SQL through the `postgres` crate.
//!
//! The tenant GUC is set per transaction with a bound parameter
//! (`set_config`), one transaction per tenant, so row-level security
//! policies see the right tenant for every row the moment write-side
//! enforcement lands. A failed connection is dropped and rebuilt once per
//! call; a second failure surfaces to the caller.
//!
//! TLS rides rustls: system roots by default, MDX_DATABASE_TLS_ROOT_CERT
//! (a PEM path) for self-signed substrates and the proof harness. The URL's
//! sslmode decides negotiation, and production refuses a DATABASE_URL that
//! does not demand TLS (sslmode=require or stronger) - encrypt-or-refuse,
//! never silently downgrade.

use rustls_pki_types::{CertificateDer, pem::PemObject};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

fn error_with_sources(prefix: &str, error: &(dyn std::error::Error + 'static)) -> String {
    let mut message = format!("{prefix}: {error}");
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(&format!("; caused by: {cause}"));
        source = cause.source();
    }
    message
}

static CLIENT: OnceLock<Mutex<Option<postgres::Client>>> = OnceLock::new();

/// Production must demand TLS in the URL itself, so a copy-pasted plaintext
/// URL cannot silently downgrade the persistence plane.
fn require_tls_in_production(database_url: &str) -> Result<(), String> {
    let production = mdx_core::DeploymentMode::from_env()
        .map_err(|error| error.message())?
        .is_production();
    enforce_tls_posture(database_url, production)
}

/// The pure posture rule, on the driver's own parse of the URL rather than a
/// substring scan: in production the parsed sslmode must be `require` (the
/// strongest mode this driver models; rustls verifies the chain and hostname
/// whenever TLS negotiates, so require carries verify-full semantics here).
fn enforce_tls_posture(database_url: &str, is_production: bool) -> Result<(), String> {
    let config: postgres::Config = database_url
        .parse()
        .map_err(|error| format!("DATABASE_URL did not parse: {error}"))?;
    if is_production && !matches!(config.get_ssl_mode(), postgres::config::SslMode::Require) {
        return Err(
            "production DATABASE_URL must set sslmode=require; a plaintext database \
             connection is refused, never downgraded to"
                .to_string(),
        );
    }
    Ok(())
}

/// The rustls connector: the override root when configured, system roots
/// otherwise. rustls always verifies the chain and hostname when TLS
/// negotiates - there is no insecure verification mode here on purpose.
fn tls_connector() -> Result<tokio_postgres_rustls::MakeRustlsConnect, String> {
    // One process-wide crypto provider; a second install attempt is fine.
    static PROVIDER: OnceLock<()> = OnceLock::new();
    PROVIDER.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
    let mut roots = rustls::RootCertStore::empty();
    if let Ok(path) = std::env::var("MDX_DATABASE_TLS_ROOT_CERT") {
        let pem = std::fs::read(&path)
            .map_err(|error| format!("read MDX_DATABASE_TLS_ROOT_CERT {path}: {error}"))?;
        for cert in CertificateDer::pem_slice_iter(&pem) {
            let cert = cert.map_err(|error| format!("parse {path}: {error}"))?;
            roots
                .add(cert)
                .map_err(|error| format!("add root from {path}: {error}"))?;
        }
        if roots.is_empty() {
            return Err(format!("{path} held no certificates"));
        }
    } else {
        for cert in rustls_native_certs::load_native_certs().certs {
            // An unloadable system cert is skipped, matching every other
            // TLS client on the host; an empty store still fails closed at
            // negotiation time because nothing can verify.
            let _ = roots.add(cert);
        }
    }
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(tokio_postgres_rustls::MakeRustlsConnect::new(config))
}

fn connect(database_url: &str) -> Result<postgres::Client, String> {
    require_tls_in_production(database_url)?;
    postgres::Client::connect(database_url, tls_connector()?).map_err(|error| {
        // Surface the full cause chain: "TLS handshake failed" alone has
        // sent more than one operator down the wrong road.
        let mut message = format!("postgres connect: {error}");
        let mut source = std::error::Error::source(&error);
        while let Some(cause) = source {
            message.push_str(&format!("; caused by: {cause}"));
            source = cause.source();
        }
        message
    })
}

fn client_slot() -> &'static Mutex<Option<postgres::Client>> {
    CLIENT.get_or_init(|| Mutex::new(None))
}

/// Run every tenant slice of one kernel snapshot in a single transaction.
/// The receipt hash chain is global, so committing one tenant before another
/// can leave an orphaned suffix when a later tenant's projection fails.
/// Execute one transaction while rendering only the tenant slice currently
/// being sent to Postgres. Large kernels can produce sizeable app-state SQL;
/// keeping every tenant's rendered SQL alive at once needlessly multiplies
/// the export worker's peak memory. The renderer is called again on the one
/// reconnect retry so transaction atomicity and retry behavior stay intact.
pub(crate) fn execute_generated_tenant_batches<F>(
    database_url: &str,
    tenant_ids: &[String],
    mut render_sql: F,
) -> Result<(), String>
where
    F: FnMut(&str) -> String,
{
    let mut slot = client_slot()
        .lock()
        .map_err(|_| "postgres client lock poisoned".to_string())?;
    // First attempt over the cached connection, then once more over a fresh
    // one: a server restart on the database side must not wedge exports.
    for attempt in 0..2 {
        if slot.is_none() {
            *slot = Some(connect(database_url)?);
        }
        let Some(client) = slot.as_mut() else {
            return Err("postgres client cache failed to initialize".to_string());
        };
        match run_generated_tenant_batches(client, tenant_ids, &mut render_sql) {
            Ok(()) => return Ok(()),
            Err(error) => {
                *slot = None;
                if attempt == 1 {
                    return Err(error);
                }
            }
        }
    }
    unreachable!("loop returns on success or second failure")
}

fn run_generated_tenant_batches<F>(
    client: &mut postgres::Client,
    tenant_ids: &[String],
    render_sql: &mut F,
) -> Result<(), String>
where
    F: FnMut(&str) -> String,
{
    let mut tx = client
        .transaction()
        .map_err(|error| format!("postgres begin: {error}"))?;
    for tenant_id in tenant_ids {
        tx.execute("SELECT set_config('mdx.tenant_id', $1, true)", &[tenant_id])
            .map_err(|error| format!("postgres set tenant {tenant_id}: {error}"))?;
        let sql = render_sql(tenant_id);
        tx.batch_execute(&sql).map_err(|error| {
            error_with_sources(&format!("postgres execute tenant {tenant_id}"), &error)
        })?;
    }
    tx.commit()
        .map_err(|error| format!("postgres commit: {error}"))?;
    Ok(())
}

/// Return the durable global receipt count and its one unreferenced chain
/// head. Ambiguous heads refuse incremental export instead of guessing across
/// a fork.
pub(crate) fn query_ledger_cursor(database_url: &str) -> Result<(usize, Option<String>), String> {
    let mut client = connect(database_url)?;
    let count = client
        .query_one("SELECT COUNT(*)::bigint FROM ledger_entries", &[])
        .map_err(|error| error_with_sources("postgres count ledger_entries", &error))?
        .get::<_, i64>(0)
        .max(0) as usize;
    let heads = client
        .query(
            "SELECT hash FROM ledger_entries WHERE hash NOT IN (\
             SELECT previous_hash FROM ledger_entries WHERE previous_hash IS NOT NULL\
             ) LIMIT 2",
            &[],
        )
        .map_err(|error| error_with_sources("postgres query ledger head", &error))?;
    match (count, heads.as_slice()) {
        (0, []) => Ok((0, None)),
        (0, _) => Err("ledger_entries has a head but no rows".to_string()),
        (_, [head]) => Ok((count, Some(head.get(0)))),
        (_, []) => Err("ledger_entries has rows but no global chain head".to_string()),
        (_, _) => Err("ledger_entries has multiple global chain heads".to_string()),
    }
}

/// Read the full receipt ledger back from the durable store, re-linked into
/// chain order through previous_hash. Used at boot when the persistence
/// plane is configured but no local snapshot exists (new host, lost disk):
/// the database is the source of truth, not just an export target.
pub(crate) fn query_ledger_entries(database_url: &str) -> Result<Vec<mdx_core::Receipt>, String> {
    let mut client = connect(database_url)?;
    let rows = client
        .query(
            "SELECT receipt_id, tenant_id, trace_id, actor_id, loop_id, workflow_id, kind, \
             policy_decision_id, payload::text, previous_hash, receipt_timestamp, \
             hash_version, hash FROM ledger_entries",
            &[],
        )
        .map_err(|error| format!("postgres query ledger_entries: {error}"))?;
    let mut by_previous: std::collections::HashMap<Option<String>, mdx_core::Receipt> =
        std::collections::HashMap::new();
    for row in &rows {
        let payload_text: String = row.get(8);
        let payload_value: serde_json::Value = serde_json::from_str(&payload_text)
            .map_err(|error| format!("ledger_entries payload parse: {error}"))?;
        let payload = payload_value
            .as_object()
            .ok_or_else(|| "ledger_entries payload is not an object".to_string())?
            .iter()
            .map(|(key, value)| {
                value
                    .as_str()
                    .map(|text| (key.clone(), text.to_string()))
                    .ok_or_else(|| {
                        format!("ledger_entries payload value for {key} is not a string")
                    })
            })
            .collect::<Result<std::collections::BTreeMap<String, String>, String>>()?;
        let receipt = mdx_core::Receipt {
            receipt_id: row.get(0),
            tenant_id: mdx_core::TenantId::new(row.get::<_, String>(1)),
            trace_id: mdx_core::TraceId::new(row.get::<_, String>(2)),
            actor_id: mdx_core::ActorId::new(row.get::<_, String>(3)),
            loop_id: mdx_core::LoopId::new(row.get::<_, String>(4)),
            workflow_id: mdx_core::WorkflowId::new(row.get::<_, String>(5)),
            kind: row.get(6),
            policy_decision_id: row.get(7),
            payload,
            previous_hash: row.get(9),
            receipt_timestamp: row.get::<_, Option<String>>(10).unwrap_or_default(),
            hash_version: row
                .get::<_, Option<i32>>(11)
                .map(|value| value.max(1) as u64)
                .unwrap_or(mdx_core::RECEIPT_HASH_VERSION_TIMELESS),
            hash: row.get(12),
        };
        if by_previous
            .insert(receipt.previous_hash.clone(), receipt)
            .is_some()
        {
            return Err(
                "ledger_entries holds two receipts with the same previous_hash; the chain is ambiguous and the restore refuses to guess".to_string(),
            );
        }
    }
    // Walk the chain from the genesis receipt. A row count that does not
    // match the walk means orphaned rows; the restore refuses rather than
    // silently dropping evidence.
    let mut ordered = Vec::with_capacity(rows.len());
    let mut cursor: Option<String> = None;
    while let Some(receipt) = by_previous.remove(&cursor) {
        cursor = Some(receipt.hash.clone());
        ordered.push(receipt);
    }
    if !by_previous.is_empty() {
        return Err(format!(
            "ledger_entries holds {} receipts that do not link into the chain; the restore refuses to guess",
            by_previous.len()
        ));
    }
    Ok(ordered)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DurableMemoryRow {
    memory_id: String,
    tenant_id: String,
    source_receipt_id: String,
    atom_origin: String,
    valid_from_receipt_timestamp: String,
    memory_scope: String,
    memory_tier: String,
    decay_policy: String,
    importance_score: i32,
    consolidation_decision: String,
    content: String,
    valid_until_receipt_timestamp: String,
    invalidated_by_receipt_id: String,
    consolidation_state: String,
    embedding: String,
}

/// Restore the durable Memory rows that accompany a Postgres ledger restore.
/// Render disks are replaceable, so the database must be sufficient to bring
/// both receipts and their materialized Memory records back after a restart.
pub(crate) fn query_memory_records(
    database_url: &str,
    receipts: &[mdx_core::Receipt],
) -> Result<Vec<mdx_core::MemoryRecord>, String> {
    let mut client = connect(database_url)?;
    let rows = client
        .query(
            "SELECT memory_id, tenant_id, source_receipt_id, atom_origin, \
             valid_from_receipt_timestamp, memory_scope, memory_tier, decay_policy, \
             importance_score, consolidation_decision, content, \
             COALESCE(valid_until_receipt_timestamp, ''), \
             COALESCE(invalidated_by_receipt_id, ''), consolidation_state, \
             COALESCE(embedding::text, '') FROM memory_records ORDER BY memory_id",
            &[],
        )
        .map_err(|error| error_with_sources("postgres query memory_records", &error))?;
    let rows = rows
        .into_iter()
        .map(|row| DurableMemoryRow {
            memory_id: row.get(0),
            tenant_id: row.get(1),
            source_receipt_id: row.get(2),
            atom_origin: row.get(3),
            valid_from_receipt_timestamp: row.get(4),
            memory_scope: row.get(5),
            memory_tier: row.get(6),
            decay_policy: row.get(7),
            importance_score: row.get(8),
            consolidation_decision: row.get(9),
            content: row.get(10),
            valid_until_receipt_timestamp: row.get(11),
            invalidated_by_receipt_id: row.get(12),
            consolidation_state: row.get(13),
            embedding: row.get(14),
        })
        .collect();
    memory_records_from_rows(rows, receipts)
}

fn memory_records_from_rows(
    rows: Vec<DurableMemoryRow>,
    receipts: &[mdx_core::Receipt],
) -> Result<Vec<mdx_core::MemoryRecord>, String> {
    let sources: HashMap<&str, &mdx_core::Receipt> = receipts
        .iter()
        .map(|receipt| (receipt.receipt_id.as_str(), receipt))
        .collect();
    let mut gates = HashMap::<&str, &mdx_core::Receipt>::new();
    for receipt in receipts {
        if matches!(
            receipt.kind.as_str(),
            "memory.consolidation_decided"
                | "memory.consolidation.decided"
                | "twin.session.draft.memory_consolidated"
                | "memory.surface_event.consolidated"
        ) && let Some(source_receipt_id) = receipt.payload.get("source_receipt_id")
        {
            gates.insert(source_receipt_id, receipt);
        }
    }
    rows.into_iter()
        .map(|row| {
            let source = sources.get(row.source_receipt_id.as_str()).ok_or_else(|| {
                format!(
                    "memory_records source receipt {} is absent from the restored ledger",
                    row.source_receipt_id
                )
            })?;
            let gate = gates.get(row.source_receipt_id.as_str()).ok_or_else(|| {
                format!(
                    "memory_records consolidation gate for {} is absent from the restored ledger",
                    row.source_receipt_id
                )
            })?;
            let decision = match row.consolidation_decision.as_str() {
                "RETAIN" => mdx_core::ConsolidationDecision::Retain,
                "SKIP" => mdx_core::ConsolidationDecision::Skip,
                "ADD" => mdx_core::ConsolidationDecision::Add,
                "UPDATE" => mdx_core::ConsolidationDecision::Update,
                "SUPERSEDE" => mdx_core::ConsolidationDecision::Supersede,
                "NOOP" => mdx_core::ConsolidationDecision::Noop,
                other => {
                    return Err(format!(
                        "memory_records {} has unknown consolidation decision {other}",
                        row.memory_id
                    ));
                }
            };
            let importance_score = u8::try_from(row.importance_score).map_err(|_| {
                format!(
                    "memory_records {} importance score is outside u8",
                    row.memory_id
                )
            })?;
            Ok(mdx_core::MemoryRecord {
                episode_id: format!("episode_{}", row.memory_id),
                memory_id: row.memory_id,
                tenant_id: mdx_core::TenantId::new(row.tenant_id),
                source_receipt_id: row.source_receipt_id,
                atom_origin: leak_memory_value(row.atom_origin),
                valid_from_receipt_timestamp: row.valid_from_receipt_timestamp,
                consolidation_decision: decision,
                provenance: mdx_core::MemoryProvenance {
                    driver_id: mdx_core::LOCAL_MEMORY_DRIVER.driver_id,
                    provider: mdx_core::LOCAL_MEMORY_DRIVER.provider,
                    durable_driver: mdx_core::LOCAL_MEMORY_DRIVER.durable_driver,
                    durable_table: mdx_core::LOCAL_MEMORY_DRIVER.durable_table,
                    consolidation_gate: "local_consolidation_gate_v1",
                    gate_receipt_id: gate.receipt_id.clone(),
                    source_receipt_kind: source.kind.clone(),
                    temporal_status: "event_completed",
                },
                memory_scope: leak_memory_value(row.memory_scope),
                memory_tier: leak_memory_value(row.memory_tier),
                decay_policy: leak_memory_value(row.decay_policy),
                importance_score,
                content: row.content,
                valid_until_receipt_timestamp: row.valid_until_receipt_timestamp,
                invalidated_by_receipt_id: row.invalidated_by_receipt_id,
                consolidation_state: leak_memory_value(row.consolidation_state),
                embedding: row.embedding,
            })
        })
        .collect()
}

fn leak_memory_value(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

/// The highest numeric id suffix across restored receipts and policy ids,
/// so the restored id counter can never reissue an existing id.
pub(crate) fn max_id_suffix(receipts: &[mdx_core::Receipt]) -> u64 {
    receipts
        .iter()
        .flat_map(|receipt| {
            [
                Some(receipt.receipt_id.as_str()),
                receipt.policy_decision_id.as_deref(),
            ]
        })
        .flatten()
        .filter_map(|id| id.rsplit('_').next())
        .filter_map(|tail| tail.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
}

pub(crate) fn restore_kernel(
    database_url: &str,
    kernel: &mut mdx_core::MdxKernel,
) -> Result<Option<(usize, usize)>, String> {
    let entries = query_ledger_entries(database_url)
        .map_err(|error| format!("postgres ledger restore refused: {error}"))?;
    if entries.is_empty() {
        return Ok(None);
    }
    let memory_records = query_memory_records(database_url, &entries)
        .map_err(|error| format!("postgres memory restore refused: {error}"))?;
    let counter = max_id_suffix(&entries);
    let ledger_count = kernel
        .restore_ledger_entries(entries)
        .map_err(|error| format!("postgres ledger restore refused: {error}"))?;
    kernel.restore_ids_counter(counter);
    let memory_count = kernel
        .restore_memory_brain_snapshot(mdx_core::MemoryBrainSnapshot {
            records: memory_records,
            ..mdx_core::MemoryBrainSnapshot::default()
        })
        .map_err(|error| format!("postgres memory restore refused: {error}"))?;
    Ok(Some((ledger_count, memory_count)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durable_memory_rows_rejoin_their_receipt_provenance() {
        let mut kernel = mdx_core::MdxKernel::boot_local();
        kernel
            .run_evals_runner_agent()
            .expect("seed a receipt-backed Memory record");
        let source = kernel.memory_records().first().expect("memory record");
        let rows = vec![DurableMemoryRow {
            memory_id: source.memory_id.clone(),
            tenant_id: source.tenant_id.as_str().to_string(),
            source_receipt_id: source.source_receipt_id.clone(),
            atom_origin: source.atom_origin.to_string(),
            valid_from_receipt_timestamp: source.valid_from_receipt_timestamp.clone(),
            memory_scope: source.memory_scope.to_string(),
            memory_tier: source.memory_tier.to_string(),
            decay_policy: source.decay_policy.to_string(),
            importance_score: i32::from(source.importance_score),
            consolidation_decision: source.consolidation_decision.as_str().to_string(),
            content: source.content.clone(),
            valid_until_receipt_timestamp: source.valid_until_receipt_timestamp.clone(),
            invalidated_by_receipt_id: source.invalidated_by_receipt_id.clone(),
            consolidation_state: source.consolidation_state.to_string(),
            embedding: source.embedding.clone(),
        }];
        let restored = memory_records_from_rows(rows, kernel.ledger().entries())
            .expect("durable memory restore");
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].memory_id, source.memory_id);
        assert_eq!(restored[0].source_receipt_id, source.source_receipt_id);
        assert_eq!(
            restored[0].provenance.gate_receipt_id,
            source.provenance.gate_receipt_id
        );
        assert_eq!(
            restored[0].provenance.source_receipt_kind,
            source.provenance.source_receipt_kind
        );
        assert_eq!(restored[0].content, source.content);
    }

    #[test]
    fn production_posture_admits_only_a_parsed_sslmode_require() {
        let secured = "postgres://mdx:mdx@db.internal:5432/mdx?sslmode=require";
        assert!(enforce_tls_posture(secured, true).is_ok());
        assert!(enforce_tls_posture(secured, false).is_ok());
    }

    #[test]
    fn production_posture_refuses_plaintext_prefer_and_disable() {
        for url in [
            "postgres://mdx:mdx@db.internal:5432/mdx",
            "postgres://mdx:mdx@db.internal:5432/mdx?sslmode=prefer",
            "postgres://mdx:mdx@db.internal:5432/mdx?sslmode=disable",
        ] {
            let refusal = enforce_tls_posture(url, true).expect_err("production must refuse");
            assert!(refusal.contains("sslmode=require"), "{refusal}");
            assert!(
                enforce_tls_posture(url, false).is_ok(),
                "local modes keep plaintext for the deterministic substrates"
            );
        }
    }

    #[test]
    fn production_posture_cannot_be_fooled_by_a_lookalike_substring() {
        // The old substring scan would have admitted this; the parsed
        // posture refuses it because the URL itself does not parse.
        let lookalike = "postgres://mdx:mdx@db.internal:5432/mdx?sslmode=require-not&sslmode";
        assert!(enforce_tls_posture(lookalike, true).is_err());
    }

    #[test]
    fn production_posture_refuses_an_unparseable_url_in_any_mode() {
        assert!(enforce_tls_posture("not a database url", true).is_err());
        assert!(enforce_tls_posture("not a database url", false).is_err());
    }

    #[test]
    fn tls_connector_refuses_an_empty_override_file() {
        let dir = std::env::temp_dir().join("mdx-tls-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("empty.pem");
        std::fs::write(&path, "").expect("write temp pem");
        // SAFETY: test-local env mutation, single-threaded use of this var.
        unsafe { std::env::set_var("MDX_DATABASE_TLS_ROOT_CERT", &path) };
        let outcome = tls_connector();
        unsafe { std::env::remove_var("MDX_DATABASE_TLS_ROOT_CERT") };
        assert!(
            outcome.is_err(),
            "an empty root file must refuse, not trust nothing"
        );
    }
}
