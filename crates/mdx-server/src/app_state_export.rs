use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use mdx_core::{
    AuthTenantPolicyPreflight, DeploymentMode, ForgeOutcomeSignal, MarketplaceAct, MdxKernel,
    MemoryRecord, MessagePresenceRequest, MessageRealtimeCutoverPreflight, MessageThreadMessage,
    PagesPublication, PagesSearchPreflight, PostgresAppStateWriteReport, PostgresAppStateWriter,
    PostgresMigrationEvidence, Receipt, TwinSessionDraftRequest, WorkerRuntimeRequest,
    render_postgres_app_state_export_sql, validate_migration_contracts,
};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const DEFAULT_EXPORT_INTERVAL_MS: u64 = 250;

#[derive(Default)]
struct ExportSchedule {
    requested: u64,
    exported: u64,
    running: bool,
}

impl ExportSchedule {
    fn request(&mut self) -> bool {
        self.requested = self.requested.saturating_add(1);
        if self.running {
            false
        } else {
            self.running = true;
            true
        }
    }

    fn next_target(&mut self) -> Option<u64> {
        if self.exported < self.requested {
            Some(self.requested)
        } else {
            self.running = false;
            None
        }
    }

    fn complete(&mut self, target: u64) {
        self.exported = self.exported.max(target);
    }

    fn stop_after_failure(&mut self) {
        self.running = false;
    }
}

static EXPORT_SCHEDULE: OnceLock<Mutex<ExportSchedule>> = OnceLock::new();

fn export_schedule() -> &'static Mutex<ExportSchedule> {
    EXPORT_SCHEDULE.get_or_init(|| Mutex::new(ExportSchedule::default()))
}

fn lock_export_schedule() -> std::sync::MutexGuard<'static, ExportSchedule> {
    export_schedule()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn export_interval() -> Duration {
    let milliseconds = std::env::var("MDX_POSTGRES_EXPORT_INTERVAL_MS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_EXPORT_INTERVAL_MS);
    Duration::from_millis(milliseconds)
}

pub(crate) fn render_local_app_state_export_sql() -> Result<String, String> {
    let kernel = local_app_state_export_kernel()?;
    Ok(render_postgres_app_state_export_sql(
        kernel.ledger().entries(),
        kernel.memory_records(),
    ))
}

pub(crate) fn write_local_app_state_postgres() -> Result<String, String> {
    let kernel = local_app_state_export_kernel()?;
    let report = write_kernel_app_state_postgres(&kernel)?;
    Ok(format!(
        "postgres_app_state_writer: OK adapter={} app_state_tables={} ledger_receipts={} memory_records={}",
        PostgresAppStateWriter::adapter_name(),
        report.app_state_tables,
        report.ledger_receipts,
        report.memory_records
    ))
}

pub(crate) fn write_auth_user_admission_postgres(receipts: &[Receipt]) -> Result<(), String> {
    let writer = observed_app_state_writer()?;
    write_app_state_with_writer(&writer, receipts, &[]).map(|_| ())
}

pub(crate) fn route_app_state_writer_enabled() -> bool {
    std::env::var("MDX_POSTGRES_APP_STATE_WRITER")
        .ok()
        .as_deref()
        == Some("1")
}

pub(crate) fn write_kernel_app_state_postgres(
    kernel: &MdxKernel,
) -> Result<PostgresAppStateWriteReport, String> {
    let writer = observed_app_state_writer()?;
    write_kernel_app_state_with_writer(&writer, kernel)
}

fn write_kernel_app_state_with_writer(
    writer: &PostgresAppStateWriter,
    kernel: &MdxKernel,
) -> Result<PostgresAppStateWriteReport, String> {
    write_app_state_with_writer(writer, kernel.ledger().entries(), kernel.memory_records())
}

fn write_shared_kernel_app_state_postgres(
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<PostgresAppStateWriteReport, String> {
    let writer = observed_app_state_writer()?;
    let (persisted_count, persisted_head) =
        crate::postgres_exec::query_ledger_cursor(writer.database_url())?;
    let (receipts, memory) = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        let entries = kernel.ledger().entries();
        let start = verified_delta_start(entries, persisted_count, persisted_head.as_deref())?;
        (entries[start..].to_vec(), kernel.memory_records().to_vec())
    };
    write_owned_app_state_with_writer(&writer, receipts, memory)
}

fn verified_delta_start(
    receipts: &[Receipt],
    persisted_count: usize,
    persisted_head: Option<&str>,
) -> Result<usize, String> {
    if persisted_count > receipts.len() {
        return Err(format!(
            "postgres ledger has {persisted_count} receipts but the kernel has only {}",
            receipts.len()
        ));
    }
    if persisted_count == 0 {
        if persisted_head.is_some() {
            return Err("postgres ledger reports a head for an empty chain".to_string());
        }
        return Ok(0);
    }
    let expected = receipts[persisted_count - 1].hash.as_str();
    if persisted_head != Some(expected) {
        return Err("postgres ledger head does not match the kernel prefix".to_string());
    }
    Ok(persisted_count)
}

fn write_app_state_with_writer(
    writer: &PostgresAppStateWriter,
    receipts: &[Receipt],
    memory: &[MemoryRecord],
) -> Result<PostgresAppStateWriteReport, String> {
    write_owned_app_state_with_writer(writer, receipts.to_vec(), memory.to_vec())
}

#[derive(Default)]
struct TenantAppState {
    receipts: Vec<Receipt>,
    memory: Vec<MemoryRecord>,
}

fn group_app_state_by_tenant(
    receipts: Vec<Receipt>,
    memory: Vec<MemoryRecord>,
) -> BTreeMap<String, TenantAppState> {
    let mut tenants = BTreeMap::<String, TenantAppState>::new();
    for receipt in receipts {
        tenants
            .entry(receipt.tenant_id.as_str().to_string())
            .or_default()
            .receipts
            .push(receipt);
    }
    for record in memory {
        tenants
            .entry(record.tenant_id.as_str().to_string())
            .or_default()
            .memory
            .push(record);
    }
    tenants
}

fn write_owned_app_state_with_writer(
    writer: &PostgresAppStateWriter,
    receipts: Vec<Receipt>,
    memory: Vec<MemoryRecord>,
) -> Result<PostgresAppStateWriteReport, String> {
    // The serving path executes through the native driver: one persistent
    // connection and one transaction for the full global receipt chain. The
    // tenant GUC changes inside that transaction before each tenant slice.
    // Receipts and memory are cloned exactly once under the kernel read lock,
    // then moved into tenant groups. Render and execute one tenant at a time so
    // the export worker never retains a second receipt copy or every tenant's
    // SQL simultaneously. The database transaction still spans the full
    // global chain, and governed writes never wait on the kernel lock.
    let receipt_count = receipts.len();
    let memory_count = memory.len();
    let tenant_batches = group_app_state_by_tenant(receipts, memory);
    let tenant_ids = tenant_batches.keys().cloned().collect::<Vec<_>>();
    crate::postgres_exec::execute_generated_tenant_batches(
        writer.database_url(),
        &tenant_ids,
        |tenant_id| {
            let batch = &tenant_batches[tenant_id];
            render_postgres_app_state_export_sql(&batch.receipts, &batch.memory)
        },
    )?;
    println!(
        "app-state exported via native driver: {} receipts, {} memory records, {} tenant transactions",
        receipt_count,
        memory_count,
        tenant_ids.len()
    );
    Ok(PostgresAppStateWriteReport {
        app_state_tables: 0,
        ledger_receipts: receipt_count,
        memory_records: memory_count,
    })
}

pub(crate) fn persist_after_governed_post(
    method: &str,
    path: &str,
    response_status: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<(), String> {
    if !method.eq_ignore_ascii_case("POST")
        || response_status != "200 OK"
        || !crate::request_security::is_governed_write_route(path)
        || !route_app_state_writer_enabled()
    {
        return Ok(());
    }
    write_shared_kernel_app_state_postgres(kernel)?;
    Ok(())
}

pub(crate) fn spawn_after_response(
    method: &str,
    path: &str,
    response_status: &str,
    kernel: Arc<RwLock<MdxKernel>>,
    mode: DeploymentMode,
) {
    if !crate::kernel_snapshot::enabled(mode)
        || !method.eq_ignore_ascii_case("POST")
        || response_status != "200 OK"
        || !crate::request_security::is_governed_write_route(path)
        || !route_app_state_writer_enabled()
    {
        return;
    }
    let should_spawn = lock_export_schedule().request();
    if !should_spawn {
        return;
    }
    let spawn = std::thread::Builder::new()
        .name("mdx-app-state-export".to_string())
        .spawn(move || {
            let interval = export_interval();
            loop {
                if !interval.is_zero() {
                    std::thread::sleep(interval);
                }
                let Some(target) = lock_export_schedule().next_target() else {
                    return;
                };
                match write_shared_kernel_app_state_postgres(&kernel) {
                    Ok(_) => lock_export_schedule().complete(target),
                    Err(error) => {
                        eprintln!("mdx-server app-state export degraded: {error}");
                        record_export_degraded(&error);
                        lock_export_schedule().stop_after_failure();
                        return;
                    }
                }
            }
        });
    if let Err(error) = spawn {
        lock_export_schedule().stop_after_failure();
        let error = format!("spawn app-state export worker: {error}");
        eprintln!("mdx-server app-state export degraded: {error}");
        record_export_degraded(&error);
    }
}

/// Record a degraded export so an operator can see that the Postgres shadow
/// fell behind the durable ledger. Best effort by design: the write that
/// triggered this is already durable in the snapshot.
pub(crate) fn record_export_degraded(error: &str) {
    let recorded_at_epoch_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let body = serde_json::json!({
        "name": "mdx-app-state-export-degraded",
        "recorded_at_epoch_seconds": recorded_at_epoch_seconds,
        "error": error,
        "detail": "a governed write landed durably in the kernel snapshot but the Postgres app-state export failed; the export will catch up on the next successful write",
    })
    .to_string();
    let _ = std::fs::create_dir_all(".mdx-local");
    let _ = std::fs::write(".mdx-local/app-state-export-degraded.json", body);
}

fn local_app_state_export_kernel() -> Result<MdxKernel, String> {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent()?;
    kernel.run_product_shaping_agent()?;
    kernel.run_aegis_scanner_agent()?;
    kernel.run_charter_attestation_agent()?;
    kernel.run_talent_autonomy_agent()?;
    let spawn_receipt_id = first_receipt_id(&kernel, "worker.spawn_requested")?;
    let credential_check_receipt_id = first_receipt_id(&kernel, "worker.credential.checked")?;
    kernel.run_local_worker_runtime(WorkerRuntimeRequest {
        spawn_receipt_id,
        credential_check_receipt_id,
        output_artifacts: vec!["local://worker/export-proof-output".to_string()],
        verification_evidence: vec!["make local-full-check".to_string()],
        summary: "Export proof local worker handoff".to_string(),
        next_owner: "human:local_operator".to_string(),
    })?;
    let source_receipt_id = kernel
        .ledger()
        .entries()
        .first()
        .map(|receipt| receipt.receipt_id.clone())
        .ok_or_else(|| "local app-state export seed receipt missing".to_string())?;
    kernel
        .save_twin_session_draft_local(TwinSessionDraftRequest {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            session_id: "local_twin_session_export",
            companion_id: "advisor",
            companion_stance: "advisor",
            persona_profile_id: "local_persona_v1",
            prompt_shape: "operator_question",
            evidence_receipt_id: &source_receipt_id,
            draft_text: "Export-proof Twin draft",
            live_answer: None,
            recall_packet: None,
        })
        .map_err(|error| error.message())?;
    let run_event_receipt_id = kernel
        .record_forge_run_event(mdx_core::ForgeRunEvent {
            tenant_id: "local_tenant",
            actor_id: "agent:forge",
            run_id: "local_forge_run_export",
            event: "run_started",
            work_item_id: "w_export",
            detail: "accepted",
            turn: 0,
            input_tokens: 0,
            output_tokens: 0,
        })
        .map_err(|error| error.message())?
        .receipt_id;
    kernel
        .record_forge_outcome_signal(ForgeOutcomeSignal {
            tenant_id: "local_tenant",
            actor_id: "agent:forge",
            run_id: "local_forge_run_export",
            source_receipt_id: &run_event_receipt_id,
            source_receipt_kind: "forge.run.event",
            disposition: "completed",
            summary: "Export-proof Forge outcome signal.",
            capability_ids: "rust_backend_skill",
            model_or_worker: "local_forge_worker",
            lesson_candidate: "Similar governed builds should cite prior outcome signals before planning.",
            lesson_source: "",
            message_channel_id: "forge",
        })
        .map_err(|error| error.message())?;
    kernel
        .save_marketplace_act(MarketplaceAct {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            actor_role: "operator",
            act: "install",
            source_route: "/marketplace/installs.json",
            capability_id: "rust_backend_skill",
            scope: "repo",
            decision: "install",
            read_only: true,
            note: "Export-proof Marketplace install.",
            ..MarketplaceAct::default()
        })
        .map_err(|error| error.message())?;
    kernel
        .save_marketplace_act(MarketplaceAct {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            actor_role: "operator",
            act: "approval",
            source_route: "/marketplace/approvals.json",
            capability_id: "rust_backend_skill",
            scope: "repo",
            decision: "approve",
            read_only: true,
            note: "Export-proof Marketplace approval.",
            ..MarketplaceAct::default()
        })
        .map_err(|error| error.message())?;
    kernel
        .save_message_thread_message_local(MessageThreadMessage {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            thread_id: "local_thread_export",
            channel_id: "local_ops",
            message_id: "local_message_export",
            content_type: "text",
            body: "Export-proof governed message",
            content_payload: "Export-proof governed message",
            source_receipt_id: &source_receipt_id,
            outbox_topic: "message.thread.local_ops",
        })
        .map_err(|error| error.message())?;
    kernel
        .save_message_presence_request_local(MessagePresenceRequest {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            presence_request_id: "local_presence_export",
            fanout_request_receipt_id: "",
            thread_id: "local_thread_export",
            channel_id: "local_ops",
            presence_scope: "local projection only",
        })
        .map_err(|error| error.message())?;
    kernel
        .save_message_realtime_cutover_preflight_local(MessageRealtimeCutoverPreflight {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            preflight_id: "local_message_realtime_export",
            presence_request_receipt_id: "",
            thread_id: "local_thread_export",
            channel_id: "local_ops",
            requested_realtime_scope: "tenant channel replay",
        })
        .map_err(|error| error.message())?;
    kernel
        .save_pages_publication_local(PagesPublication {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            document_id: "local_page_export",
            title: "Export Proof Page",
            body_ref: "world_model://pages/local_page_export/body/v1",
            source_receipt_id: &source_receipt_id,
            revision_id: "local_page_revision_export",
            page_type: "knowledge",
        })
        .map_err(|error| error.message())?;
    kernel
        .save_pages_search_preflight_local(PagesSearchPreflight {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            preflight_id: "local_pages_search_export",
            source_approval_request_receipt_id: "",
            document_id: "local_page_export",
            revision_id: "local_page_revision_export",
            citation_handle: "citation_local_pages_export",
            attachment_policy_id: "local_attachment_policy",
            requested_index_scope: "local projection only",
        })
        .map_err(|error| error.message())?;
    kernel
        .save_auth_tenant_policy_preflight_local(AuthTenantPolicyPreflight {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            preflight_id: "local_auth_tenant_policy_export",
            source_auth_session_evidence_id: "auth.session.local.stub",
            tenant_membership_scope: "tenant_org_members",
            role_mapping_scope: "mdx_roles",
            invite_state_scope: "invites",
            approved_model_scope: "tenant_approved_models",
        })
        .map_err(|error| error.message())?;
    Ok(kernel)
}

fn first_receipt_id(kernel: &MdxKernel, kind: &str) -> Result<String, String> {
    kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| receipt.kind == kind)
        .map(|receipt| receipt.receipt_id.clone())
        .ok_or_else(|| format!("local app-state export missing receipt kind {kind}"))
}

fn observed_app_state_writer() -> Result<PostgresAppStateWriter, String> {
    if std::env::var("MDX_POSTGRES_MIGRATIONS_OBSERVED")
        .ok()
        .as_deref()
        != Some("1")
    {
        return Err("MDX_POSTGRES_MIGRATIONS_OBSERVED=1 is required".to_string());
    }
    let report = validate_migration_contracts()?;
    PostgresAppStateWriter::connect_after_observed_migrations(
        std::env::var("DATABASE_URL").ok().as_deref(),
        PostgresMigrationEvidence {
            migration_count: report.migration_count,
            tenant_owned_tables: report.tenant_owned_tables,
            rls_enabled_tables: report.rls_enabled_tables,
            observed_by: "mdx-server write-postgres-app-state".to_string(),
        },
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod export_schedule_tests {
    use super::{
        ExportSchedule, group_app_state_by_tenant, local_app_state_export_kernel,
        verified_delta_start,
    };

    #[test]
    fn durable_cursor_selects_only_the_unexported_receipt_suffix() {
        let kernel = local_app_state_export_kernel().expect("local export kernel");
        let receipts = kernel.ledger().entries();
        assert!(receipts.len() > 1);
        let persisted = receipts.len() - 1;
        let head = receipts[persisted - 1].hash.as_str();
        assert_eq!(
            verified_delta_start(receipts, persisted, Some(head)).expect("verified prefix"),
            persisted
        );
        assert!(verified_delta_start(receipts, persisted, Some("wrong-head")).is_err());
        assert!(verified_delta_start(receipts, receipts.len() + 1, Some(head)).is_err());
    }

    #[test]
    fn app_state_moves_each_record_into_exactly_one_tenant_batch() {
        let kernel = local_app_state_export_kernel().expect("local export kernel");
        let receipts = kernel.ledger().entries().to_vec();
        let memory = kernel.memory_records().to_vec();
        let receipt_count = receipts.len();
        let memory_count = memory.len();
        let grouped = group_app_state_by_tenant(receipts, memory);

        assert_eq!(
            grouped
                .values()
                .map(|batch| batch.receipts.len())
                .sum::<usize>(),
            receipt_count
        );
        assert_eq!(
            grouped
                .values()
                .map(|batch| batch.memory.len())
                .sum::<usize>(),
            memory_count
        );
        for (tenant_id, batch) in grouped {
            assert!(
                batch
                    .receipts
                    .iter()
                    .all(|receipt| receipt.tenant_id.as_str() == tenant_id)
            );
            assert!(
                batch
                    .memory
                    .iter()
                    .all(|record| record.tenant_id.as_str() == tenant_id)
            );
        }
    }

    #[test]
    fn burst_requests_start_one_worker_and_export_latest_target() {
        let mut schedule = ExportSchedule::default();
        assert!(schedule.request());
        assert!(!schedule.request());
        assert!(!schedule.request());
        assert_eq!(schedule.next_target(), Some(3));
        schedule.complete(3);
        assert_eq!(schedule.next_target(), None);
        assert!(!schedule.running);
    }

    #[test]
    fn request_during_export_requires_one_follow_up_export() {
        let mut schedule = ExportSchedule::default();
        assert!(schedule.request());
        let first = schedule.next_target().unwrap();
        assert!(!schedule.request());
        schedule.complete(first);
        assert_eq!(schedule.next_target(), Some(2));
        schedule.complete(2);
        assert_eq!(schedule.next_target(), None);
    }

    #[test]
    fn failed_worker_leaves_work_pending_for_the_next_request() {
        let mut schedule = ExportSchedule::default();
        assert!(schedule.request());
        assert_eq!(schedule.next_target(), Some(1));
        schedule.stop_after_failure();
        assert!(schedule.request());
        assert_eq!(schedule.next_target(), Some(2));
    }
}
