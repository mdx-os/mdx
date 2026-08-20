use std::collections::{HashMap, HashSet};

use mdx_core::{MdxKernel, Receipt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AllowedLedgerReference {
    constraint: &'static str,
    table: &'static str,
    primary_key: &'static str,
}

const ALLOWED_LEDGER_REFERENCES: [AllowedLedgerReference; 2] = [
    AllowedLedgerReference {
        constraint: "observatory_role_view_snapshots_source_receipt_id_fkey",
        table: "observatory_role_view_snapshots",
        primary_key: "observatory_role_view_snapshot_id",
    },
    AllowedLedgerReference {
        constraint: "model_route_decisions_source_receipt_id_fkey",
        table: "model_route_decisions",
        primary_key: "decision_id",
    },
];

#[derive(Debug)]
struct LedgerRepairPlan {
    canonical_prefix: Vec<Receipt>,
    missing_canonical: Vec<Receipt>,
    branch_hashes: Vec<String>,
    branch_receipt_ids: Vec<String>,
    declared_head: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LedgerReferenceTarget {
    ReceiptId,
    Hash,
}

trait RepairTransactionFinish {
    fn commit_repair(self) -> Result<(), String>;
    fn rollback_repair(self) -> Result<(), String>;
}

impl RepairTransactionFinish for postgres::Transaction<'_> {
    fn commit_repair(self) -> Result<(), String> {
        self.commit()
            .map_err(|error| format!("postgres commit ledger repair: {error}"))
    }

    fn rollback_repair(self) -> Result<(), String> {
        self.rollback()
            .map_err(|error| format!("postgres rollback ledger repair: {error}"))
    }
}

fn finish_repair_transaction(
    tx: impl RepairTransactionFinish,
    repair_result: Result<String, String>,
) -> Result<String, String> {
    match repair_result {
        Ok(report) => {
            tx.commit_repair()?;
            Ok(report)
        }
        Err(error) => {
            tx.rollback_repair().map_err(|rollback_error| {
                format!("ledger repair failed: {error}; {rollback_error}")
            })?;
            Err(format!("ledger repair rolled back: {error}"))
        }
    }
}

fn require_operator_gates(
    enabled: Option<&str>,
    maintenance_ack: Option<&str>,
) -> Result<(), String> {
    if enabled != Some("1") {
        return Err("MDX_LEDGER_REPAIR_ENABLED=1 is required".to_string());
    }
    if maintenance_ack != Some("1") {
        return Err(
            "MDX_LEDGER_REPAIR_MAINTENANCE_ACK=1 is required; hold governed writes and restart the kernel after repair"
                .to_string(),
        );
    }
    Ok(())
}

pub(crate) fn run() -> Result<String, String> {
    let enabled = std::env::var("MDX_LEDGER_REPAIR_ENABLED").ok();
    let maintenance_ack = std::env::var("MDX_LEDGER_REPAIR_MAINTENANCE_ACK").ok();
    require_operator_gates(enabled.as_deref(), maintenance_ack.as_deref())?;
    let approver_actor_id = std::env::var("MDX_LEDGER_REPAIR_APPROVER_ACTOR_ID")
        .map_err(|_| "MDX_LEDGER_REPAIR_APPROVER_ACTOR_ID is required".to_string())?;
    let approval_reference = std::env::var("MDX_LEDGER_REPAIR_APPROVAL_REFERENCE")
        .map_err(|_| "MDX_LEDGER_REPAIR_APPROVAL_REFERENCE is required".to_string())?;
    let database_url =
        std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL is required".to_string())?;

    let mut client = crate::postgres_exec::connect(&database_url)?;
    let mut tx = client
        .transaction()
        .map_err(|error| format!("postgres begin ledger repair: {error}"))?;
    tx.batch_execute(
        "SET LOCAL lock_timeout = '15s'; \
         LOCK TABLE ledger_entries IN ACCESS EXCLUSIVE MODE; \
         LOCK TABLE observatory_role_view_snapshots IN ACCESS EXCLUSIVE MODE; \
         LOCK TABLE model_route_decisions IN ACCESS EXCLUSIVE MODE;",
    )
    .map_err(|error| format!("postgres lock ledger repair boundary: {error}"))?;

    let heads = crate::postgres_exec::query_declared_global_heads(&mut tx)?;
    let declared_head = match heads.as_slice() {
        [head] => head.clone(),
        [] => return Err("ledger repair requires one declared global chain head".to_string()),
        _ => {
            return Err("ledger repair refuses multiple declared global chain heads".to_string());
        }
    };
    let durable = crate::postgres_exec::query_all_ledger_entries_with_client(&mut tx)?;
    let snapshot = verified_snapshot_receipts()?;
    let plan = build_plan(&snapshot, &durable, &declared_head)?;
    if plan.missing_canonical.is_empty() {
        tx.rollback()
            .map_err(|error| format!("postgres rollback no-op ledger repair: {error}"))?;
        return Ok(
            "ledger_repair: OK no_repair_required=true archived_branch_receipts=0 restored_canonical_receipts=0 archived_reference_rows=0"
                .to_string(),
        );
    }

    let archived_reference_rows =
        inspect_references(&mut tx, &plan.branch_receipt_ids, &plan.branch_hashes)?;
    if std::env::var("MDX_LEDGER_REPAIR_DRY_RUN").ok().as_deref() == Some("1") {
        tx.rollback()
            .map_err(|error| format!("postgres rollback ledger repair dry run: {error}"))?;
        return Ok(format!(
            "ledger_repair: OK dry_run=true declared_head_unchanged=true branch_receipts={} canonical_receipts={} dependent_rows={}",
            plan.branch_hashes.len(),
            plan.missing_canonical.len(),
            archived_reference_rows
        ));
    }
    let repair_result = (|| -> Result<String, String> {
        let repair_id = new_repair_id()?;
        let snapshot_head = snapshot
            .last()
            .map(|receipt| receipt.hash.as_str())
            .ok_or_else(|| "verified snapshot is empty".to_string())?;
        tx.execute(
            "INSERT INTO ledger_repair_runs (repair_id, approver_actor_id, approval_reference, \
             declared_head_hash, snapshot_head_hash, archived_branch_receipts, \
             restored_canonical_receipts, archived_reference_rows, status) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'completed')",
            &[
                &repair_id,
                &approver_actor_id,
                &approval_reference,
                &plan.declared_head,
                &snapshot_head,
                &(plan.branch_hashes.len() as i32),
                &(plan.missing_canonical.len() as i32),
                &(archived_reference_rows as i32),
            ],
        )
        .map_err(|error| format!("postgres record ledger repair run: {error}"))?;

        let archived_references = archive_known_references(
            &mut tx,
            &repair_id,
            &approver_actor_id,
            &approval_reference,
            &plan.branch_hashes,
        )?;
        if archived_references != archived_reference_rows {
            return Err(format!(
                "ledger repair expected to archive {archived_reference_rows} dependent rows but archived {archived_references}"
            ));
        }
        archive_branch_rows(
            &mut tx,
            &repair_id,
            &approver_actor_id,
            &approval_reference,
            &plan.branch_hashes,
        )?;
        let deleted = tx
            .execute(
                "DELETE FROM ledger_entries WHERE hash = ANY($1)",
                &[&plan.branch_hashes],
            )
            .map_err(|error| format!("postgres remove archived ledger branches: {error}"))?;
        if deleted as usize != plan.branch_hashes.len() {
            return Err(format!(
                "ledger repair expected to remove {} archived branch rows but removed {deleted}",
                plan.branch_hashes.len()
            ));
        }
        for receipt in &plan.missing_canonical {
            restore_verified_receipt(&mut tx, receipt)?;
        }

        let repaired = crate::postgres_exec::query_all_ledger_entries_with_client(&mut tx)?;
        let (canonical, _) =
            crate::postgres_exec::canonical_ledger_entries(repaired, &plan.declared_head)?;
        if canonical.len() != plan.canonical_prefix.len()
            || canonical
                .iter()
                .zip(&plan.canonical_prefix)
                .any(|(left, right)| left.hash != right.hash)
        {
            return Err(
                "ledger repair postcondition failed: declared chain does not match verified snapshot prefix"
                    .to_string(),
            );
        }
        let heads_after = crate::postgres_exec::query_declared_global_heads(&mut tx)?;
        if heads_after.as_slice() != [plan.declared_head.as_str()] {
            return Err("ledger repair changed the declared chain head".to_string());
        }
        Ok(format!(
            "ledger_repair: OK declared_head_unchanged=true archived_branch_receipts={} restored_canonical_receipts={} archived_reference_rows={}",
            plan.branch_hashes.len(),
            plan.missing_canonical.len(),
            archived_reference_rows
        ))
    })();
    finish_repair_transaction(tx, repair_result)
}

fn new_repair_id() -> Result<String, String> {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "system clock is before the Unix epoch".to_string())?;
    Ok(format!("ledger_repair_{}", elapsed.as_nanos()))
}

fn verified_snapshot_receipts() -> Result<Vec<Receipt>, String> {
    let mut kernel = MdxKernel::boot_local();
    let restored = crate::kernel_snapshot::restore_into(&mut kernel)?;
    if restored.is_none() {
        return Err(format!(
            "ledger repair requires verified snapshot {}",
            crate::kernel_snapshot::SNAPSHOT_PATH
        ));
    }
    Ok(kernel.ledger().entries().to_vec())
}

fn build_plan(
    snapshot: &[Receipt],
    durable: &[Receipt],
    declared_head: &str,
) -> Result<LedgerRepairPlan, String> {
    let head_index = snapshot
        .iter()
        .position(|receipt| receipt.hash == declared_head)
        .ok_or_else(|| {
            "ledger repair refuses a declared head absent from the verified snapshot".to_string()
        })?;
    let canonical_prefix = snapshot[..=head_index].to_vec();
    let durable_by_hash = durable
        .iter()
        .map(|receipt| (receipt.hash.as_str(), receipt))
        .collect::<HashMap<_, _>>();
    let durable_by_id = durable
        .iter()
        .map(|receipt| (receipt.receipt_id.as_str(), receipt))
        .collect::<HashMap<_, _>>();
    if durable_by_hash.len() != durable.len() || durable_by_id.len() != durable.len() {
        return Err("ledger repair refuses duplicate durable hashes or receipt ids".to_string());
    }
    let missing_canonical = canonical_prefix
        .iter()
        .filter(|receipt| !durable_by_hash.contains_key(receipt.hash.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if missing_canonical.is_empty() {
        return Ok(LedgerRepairPlan {
            canonical_prefix,
            missing_canonical,
            branch_hashes: Vec::new(),
            branch_receipt_ids: Vec::new(),
            declared_head: declared_head.to_string(),
        });
    }

    let canonical_hashes = canonical_prefix
        .iter()
        .map(|receipt| receipt.hash.as_str())
        .collect::<HashSet<_>>();
    let mut branch = HashSet::<String>::new();
    for canonical in &missing_canonical {
        let conflict = durable_by_id
            .get(canonical.receipt_id.as_str())
            .ok_or_else(|| {
                "ledger repair refuses a missing canonical receipt without an id collision"
                    .to_string()
            })?;
        if conflict.hash == canonical.hash {
            return Err("ledger repair planner observed an inconsistent hash index".to_string());
        }
        branch.insert(conflict.hash.clone());
    }
    loop {
        let before = branch.len();
        for receipt in durable {
            if branch.contains(&receipt.hash)
                && let Some(previous) = &receipt.previous_hash
                && durable_by_hash.contains_key(previous.as_str())
                && !canonical_hashes.contains(previous.as_str())
            {
                branch.insert(previous.clone());
            }
            if !canonical_hashes.contains(receipt.hash.as_str())
                && receipt
                    .previous_hash
                    .as_ref()
                    .is_some_and(|previous| branch.contains(previous))
            {
                branch.insert(receipt.hash.clone());
            }
        }
        if branch.len() == before {
            break;
        }
    }
    for hash in &branch {
        let receipt = durable_by_hash
            .get(hash.as_str())
            .ok_or_else(|| "ledger repair branch index is inconsistent".to_string())?;
        if let Some(previous) = &receipt.previous_hash
            && !canonical_hashes.contains(previous.as_str())
            && !branch.contains(previous)
        {
            return Err(
                "ledger repair refuses a colliding branch with an unavailable ancestor".to_string(),
            );
        }
    }
    let mut branch_hashes = branch.into_iter().collect::<Vec<_>>();
    branch_hashes.sort();
    let branch_receipt_ids = durable
        .iter()
        .filter(|receipt| branch_hashes.binary_search(&receipt.hash).is_ok())
        .map(|receipt| receipt.receipt_id.clone())
        .collect::<Vec<_>>();

    let branch_set = branch_hashes.iter().cloned().collect::<HashSet<_>>();
    let mut simulated = durable
        .iter()
        .filter(|receipt| !branch_set.contains(&receipt.hash))
        .cloned()
        .collect::<Vec<_>>();
    simulated.extend(missing_canonical.iter().cloned());
    let (ordered, _) = crate::postgres_exec::canonical_ledger_entries(simulated, declared_head)?;
    if ordered.len() != canonical_prefix.len()
        || ordered
            .iter()
            .zip(&canonical_prefix)
            .any(|(left, right)| left.hash != right.hash)
    {
        return Err(
            "ledger repair simulation does not recover the verified snapshot prefix".to_string(),
        );
    }

    Ok(LedgerRepairPlan {
        canonical_prefix,
        missing_canonical,
        branch_hashes,
        branch_receipt_ids,
        declared_head: declared_head.to_string(),
    })
}

fn inspect_references(
    tx: &mut postgres::Transaction<'_>,
    branch_receipt_ids: &[String],
    branch_hashes: &[String],
) -> Result<usize, String> {
    let constraints = tx
        .query(
            "SELECT ns.nspname, cl.relname, co.conname, \
                    array_agg(source_at.attname::text ORDER BY source_key.ord), \
                    array_agg(target_at.attname::text ORDER BY target_key.ord) \
             FROM pg_constraint co \
             JOIN pg_class cl ON cl.oid = co.conrelid \
             JOIN pg_namespace ns ON ns.oid = cl.relnamespace \
             JOIN unnest(co.conkey) WITH ORDINALITY source_key(attnum, ord) ON true \
             JOIN unnest(co.confkey) WITH ORDINALITY target_key(attnum, ord) \
               ON target_key.ord = source_key.ord \
             JOIN pg_attribute source_at \
               ON source_at.attrelid = co.conrelid AND source_at.attnum = source_key.attnum \
             JOIN pg_attribute target_at \
               ON target_at.attrelid = co.confrelid AND target_at.attnum = target_key.attnum \
             WHERE co.contype = 'f' AND co.confrelid = 'ledger_entries'::regclass \
             GROUP BY ns.nspname, cl.relname, co.conname",
            &[],
        )
        .map_err(|error| format!("postgres inspect ledger references: {error}"))?;
    let mut allowed_rows = 0usize;
    for row in constraints {
        let schema: String = row.get(0);
        let table: String = row.get(1);
        let constraint: String = row.get(2);
        let source_columns: Vec<String> = row.get(3);
        let target_columns: Vec<String> = row.get(4);
        if source_columns.len() != 1 {
            return Err(format!(
                "ledger repair refuses composite ledger reference {constraint}"
            ));
        }
        let column = &source_columns[0];
        let reference_target = ledger_reference_target(&constraint, &target_columns)?;
        let lookup_values = match reference_target {
            LedgerReferenceTarget::ReceiptId => branch_receipt_ids,
            LedgerReferenceTarget::Hash => branch_hashes,
        };
        let sql = format!(
            "SELECT COUNT(*)::bigint FROM {}.{} WHERE {} = ANY($1)",
            quote_identifier(&schema),
            quote_identifier(&table),
            quote_identifier(column)
        );
        let hits = tx
            .query_one(&sql, &[&lookup_values])
            .map_err(|error| format!("postgres count {constraint}: {error}"))?
            .get::<_, i64>(0)
            .max(0) as usize;
        if hits == 0 {
            continue;
        }
        let Some(reference) =
            allowed_ledger_reference(&schema, &table, column, &constraint, reference_target)
        else {
            return Err(format!(
                "ledger repair refuses {hits} dependent rows through unsupported constraint {constraint}"
            ));
        };
        if reference.table == "model_route_decisions" {
            inspect_model_route_decision_dependents(tx, branch_receipt_ids)?;
        }
        allowed_rows += hits;
    }
    Ok(allowed_rows)
}

fn allowed_ledger_reference(
    schema: &str,
    table: &str,
    column: &str,
    constraint: &str,
    reference_target: LedgerReferenceTarget,
) -> Option<AllowedLedgerReference> {
    if schema != "public"
        || column != "source_receipt_id"
        || reference_target != LedgerReferenceTarget::ReceiptId
    {
        return None;
    }
    ALLOWED_LEDGER_REFERENCES
        .iter()
        .copied()
        .find(|reference| reference.constraint == constraint && reference.table == table)
}

fn inspect_model_route_decision_dependents(
    tx: &mut postgres::Transaction<'_>,
    branch_receipt_ids: &[String],
) -> Result<(), String> {
    for (constraint, sql) in [
        (
            "model_outcomes_decision_id_fkey",
            "SELECT COUNT(*)::bigint FROM model_outcomes child \
             JOIN model_route_decisions decision ON decision.decision_id = child.decision_id \
             WHERE decision.source_receipt_id = ANY($1)",
        ),
        (
            "model_adaptive_comparisons_baseline_decision_id_fkey",
            "SELECT COUNT(*)::bigint FROM model_adaptive_comparisons child \
             JOIN model_route_decisions decision \
               ON decision.decision_id = child.baseline_decision_id \
             WHERE decision.source_receipt_id = ANY($1)",
        ),
    ] {
        let hits = tx
            .query_one(sql, &[&branch_receipt_ids])
            .map_err(|error| format!("postgres count {constraint}: {error}"))?
            .get::<_, i64>(0)
            .max(0) as usize;
        if hits != 0 {
            return Err(format!(
                "ledger repair refuses {hits} nested dependent rows through unsupported constraint {constraint}"
            ));
        }
    }
    Ok(())
}

fn ledger_reference_target(
    constraint: &str,
    target_columns: &[String],
) -> Result<LedgerReferenceTarget, String> {
    match target_columns {
        [column] if column == "receipt_id" => Ok(LedgerReferenceTarget::ReceiptId),
        [column] if column == "hash" => Ok(LedgerReferenceTarget::Hash),
        [_] => Err(format!(
            "ledger repair refuses {constraint} referencing an unsupported ledger column"
        )),
        _ => Err(format!(
            "ledger repair refuses composite ledger reference {constraint}"
        )),
    }
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn archive_known_references(
    tx: &mut postgres::Transaction<'_>,
    repair_id: &str,
    approver_actor_id: &str,
    approval_reference: &str,
    branch_hashes: &[String],
) -> Result<usize, String> {
    let mut total = 0usize;
    for reference in ALLOWED_LEDGER_REFERENCES {
        total += archive_known_reference(
            tx,
            repair_id,
            approver_actor_id,
            approval_reference,
            branch_hashes,
            reference,
        )?;
    }
    Ok(total)
}

fn archive_known_reference(
    tx: &mut postgres::Transaction<'_>,
    repair_id: &str,
    approver_actor_id: &str,
    approval_reference: &str,
    branch_hashes: &[String],
    reference: AllowedLedgerReference,
) -> Result<usize, String> {
    let table = quote_identifier(reference.table);
    let primary_key = quote_identifier(reference.primary_key);
    let archive_sql = format!(
        "INSERT INTO ledger_branch_reference_archives \
         (repair_id, tenant_id, source_table, source_primary_key, source_receipt_id, \
          source_receipt_hash, row_data, approver_actor_id, approval_reference) \
         SELECT $1, source.tenant_id, $4, source.{primary_key}::text, \
                source.source_receipt_id, entry.hash, to_jsonb(source), $2, $3 \
         FROM public.{table} source \
         JOIN ledger_entries entry ON entry.receipt_id = source.source_receipt_id \
         WHERE entry.hash = ANY($5)"
    );
    let archived = tx
        .execute(
            &archive_sql,
            &[
                &repair_id,
                &approver_actor_id,
                &approval_reference,
                &reference.table,
                &branch_hashes,
            ],
        )
        .map_err(|error| format!("postgres archive branch references: {error}"))?;
    let delete_sql = format!(
        "DELETE FROM public.{table} source USING ledger_entries entry \
         WHERE entry.receipt_id = source.source_receipt_id AND entry.hash = ANY($1)"
    );
    let deleted = tx
        .execute(&delete_sql, &[&branch_hashes])
        .map_err(|error| format!("postgres remove archived branch references: {error}"))?;
    if archived != deleted {
        return Err(format!(
            "ledger repair archived {archived} dependent rows but removed {deleted}"
        ));
    }
    Ok(archived as usize)
}

fn archive_branch_rows(
    tx: &mut postgres::Transaction<'_>,
    repair_id: &str,
    approver_actor_id: &str,
    approval_reference: &str,
    branch_hashes: &[String],
) -> Result<(), String> {
    let archived = tx
        .execute(
            "INSERT INTO ledger_branch_entry_archives \
             (hash, repair_id, receipt_id, tenant_id, trace_id, actor_id, loop_id, workflow_id, \
              kind, policy_decision_id, payload, previous_hash, receipt_timestamp, hash_version, \
              original_created_at, approver_actor_id, approval_reference) \
             SELECT hash, $1, receipt_id, tenant_id, trace_id, actor_id, loop_id, workflow_id, \
                    kind, policy_decision_id, payload, previous_hash, receipt_timestamp, hash_version, \
                    created_at, $2, $3 FROM ledger_entries WHERE hash = ANY($4)",
            &[
                &repair_id,
                &approver_actor_id,
                &approval_reference,
                &branch_hashes,
            ],
        )
        .map_err(|error| format!("postgres archive ledger branches: {error}"))?;
    if archived as usize != branch_hashes.len() {
        return Err(format!(
            "ledger repair expected to archive {} branch rows but archived {archived}",
            branch_hashes.len()
        ));
    }
    Ok(())
}

fn restore_verified_receipt(
    tx: &mut postgres::Transaction<'_>,
    receipt: &Receipt,
) -> Result<(), String> {
    tx.batch_execute(&mdx_core::render_postgres_receipt_insert_sql(receipt))
        .map_err(|error| format!("postgres restore verified canonical receipt: {error}"))?;
    let restored = tx
        .query_one(
            "SELECT COUNT(*)::bigint FROM ledger_entries WHERE receipt_id = $1 AND hash = $2",
            &[&receipt.receipt_id, &receipt.hash],
        )
        .map_err(|error| format!("postgres verify restored canonical receipt: {error}"))?
        .get::<_, i64>(0);
    if restored != 1 {
        return Err(format!(
            "ledger repair failed to restore canonical receipt {} exactly",
            receipt.receipt_id
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    struct FinishProbe {
        committed: Rc<Cell<bool>>,
        rolled_back: Rc<Cell<bool>>,
    }

    impl RepairTransactionFinish for FinishProbe {
        fn commit_repair(self) -> Result<(), String> {
            self.committed.set(true);
            Ok(())
        }

        fn rollback_repair(self) -> Result<(), String> {
            self.rolled_back.set(true);
            Ok(())
        }
    }

    fn canonical_receipts() -> Vec<Receipt> {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .run_evals_runner_agent()
            .expect("seed canonical receipts");
        kernel.ledger().entries().to_vec()
    }

    #[test]
    fn repair_plan_archives_collisions_and_recovers_the_declared_chain() {
        let canonical = canonical_receipts();
        assert!(canonical.len() > 6);
        let head = canonical.last().expect("head").hash.clone();
        let mut durable = canonical.clone();
        for index in [2usize, 4usize] {
            durable[index].hash.push_str("_historical_branch");
            durable[index]
                .payload
                .insert("branch".to_string(), "historical".to_string());
        }
        let plan = build_plan(&canonical, &durable, &head).expect("repair plan");
        assert_eq!(plan.missing_canonical.len(), 2);
        assert_eq!(plan.branch_hashes.len(), 2);
        assert_eq!(plan.canonical_prefix, canonical);
    }

    #[test]
    fn repair_plan_archives_every_descendant_of_a_colliding_receipt() {
        let canonical = canonical_receipts();
        let head = canonical.last().expect("head").hash.clone();
        let mut durable = canonical.clone();
        let mut conflict = canonical[2].clone();
        conflict.hash.push_str("_historical_branch");
        conflict
            .payload
            .insert("branch".to_string(), "historical".to_string());
        durable[2] = conflict.clone();

        let mut descendant = canonical[3].clone();
        descendant.receipt_id.push_str("_branch_descendant");
        descendant.hash.push_str("_branch_descendant");
        descendant.previous_hash = Some(conflict.hash.clone());
        durable.push(descendant.clone());

        let plan = build_plan(&canonical, &durable, &head).expect("repair plan");
        assert_eq!(plan.missing_canonical.len(), 1);
        assert_eq!(plan.branch_hashes.len(), 2);
        assert!(plan.branch_hashes.contains(&conflict.hash));
        assert!(plan.branch_hashes.contains(&descendant.hash));
    }

    #[test]
    fn repair_plan_archives_ancestors_but_preserves_unrelated_orphans() {
        let canonical = canonical_receipts();
        let head = canonical.last().expect("head").hash.clone();
        let mut durable = canonical.clone();
        let mut ancestor = canonical[2].clone();
        ancestor.receipt_id.push_str("_branch_ancestor");
        ancestor.hash.push_str("_branch_ancestor");
        durable.push(ancestor.clone());

        let mut conflict = canonical[3].clone();
        conflict.hash.push_str("_historical_branch");
        conflict.previous_hash = Some(ancestor.hash.clone());
        durable[3] = conflict.clone();

        let mut orphan = canonical[4].clone();
        orphan.receipt_id.push_str("_orphan");
        orphan.hash.push_str("_orphan");
        orphan.previous_hash = Some("absent_parent".to_string());
        durable.push(orphan.clone());

        let plan = build_plan(&canonical, &durable, &head).expect("repair plan");
        assert_eq!(plan.missing_canonical.len(), 1);
        assert_eq!(plan.branch_hashes.len(), 2);
        assert!(plan.branch_hashes.contains(&ancestor.hash));
        assert!(plan.branch_hashes.contains(&conflict.hash));
        assert!(!plan.branch_hashes.contains(&orphan.hash));
    }

    #[test]
    fn repair_plan_preserves_a_noncolliding_continuation_after_the_declared_head() {
        let canonical = canonical_receipts();
        let head = canonical.last().expect("head").hash.clone();
        let mut durable = canonical.clone();
        let mut conflict = canonical[2].clone();
        conflict.hash.push_str("_historical_branch");
        durable[2] = conflict.clone();

        let mut continuation = canonical.last().expect("head").clone();
        continuation.receipt_id.push_str("_continuation");
        continuation.hash.push_str("_continuation");
        continuation.previous_hash = Some(head.clone());
        durable.push(continuation.clone());

        let plan = build_plan(&canonical, &durable, &head).expect("repair plan");
        assert_eq!(plan.branch_hashes, vec![conflict.hash]);
        assert!(!plan.branch_hashes.contains(&continuation.hash));
    }

    #[test]
    fn repair_plan_refuses_a_colliding_branch_with_an_unavailable_ancestor() {
        let canonical = canonical_receipts();
        let head = canonical.last().expect("head").hash.clone();
        let mut durable = canonical.clone();
        let mut conflict = canonical[2].clone();
        conflict.hash.push_str("_historical_branch");
        conflict.previous_hash = Some("missing_branch_ancestor".to_string());
        durable[2] = conflict;

        let error = build_plan(&canonical, &durable, &head)
            .expect_err("partial colliding branches must refuse");
        assert!(error.contains("unavailable ancestor"));
    }

    #[test]
    fn ledger_reference_targets_distinguish_ids_hashes_and_composites() {
        assert_eq!(
            ledger_reference_target("by_id", &["receipt_id".to_string()]),
            Ok(LedgerReferenceTarget::ReceiptId)
        );
        assert_eq!(
            ledger_reference_target("by_hash", &["hash".to_string()]),
            Ok(LedgerReferenceTarget::Hash)
        );
        let error =
            ledger_reference_target("composite", &["receipt_id".to_string(), "hash".to_string()])
                .expect_err("composite references must refuse");
        assert!(error.contains("composite ledger reference"));
    }

    #[test]
    fn repair_allows_only_the_two_archived_reference_shapes() {
        assert_eq!(
            allowed_ledger_reference(
                "public",
                "observatory_role_view_snapshots",
                "source_receipt_id",
                "observatory_role_view_snapshots_source_receipt_id_fkey",
                LedgerReferenceTarget::ReceiptId,
            ),
            Some(ALLOWED_LEDGER_REFERENCES[0])
        );
        assert_eq!(
            allowed_ledger_reference(
                "public",
                "model_route_decisions",
                "source_receipt_id",
                "model_route_decisions_source_receipt_id_fkey",
                LedgerReferenceTarget::ReceiptId,
            ),
            Some(ALLOWED_LEDGER_REFERENCES[1])
        );
        assert_eq!(
            allowed_ledger_reference(
                "public",
                "model_route_decisions",
                "source_receipt_id",
                "unexpected_constraint",
                LedgerReferenceTarget::ReceiptId,
            ),
            None
        );
        assert_eq!(
            allowed_ledger_reference(
                "private",
                "model_route_decisions",
                "source_receipt_id",
                "model_route_decisions_source_receipt_id_fkey",
                LedgerReferenceTarget::ReceiptId,
            ),
            None
        );
    }

    #[test]
    fn repair_plan_refuses_missing_canonical_rows_without_id_collisions() {
        let canonical = canonical_receipts();
        let head = canonical.last().expect("head").hash.clone();
        let mut durable = canonical.clone();
        durable.remove(2);
        let error =
            build_plan(&canonical, &durable, &head).expect_err("unexplained gaps must refuse");
        assert!(error.contains("without an id collision"));
    }

    #[test]
    fn operator_command_requires_enablement_and_a_maintenance_window() {
        let error = require_operator_gates(None, None).expect_err("repair must be held");
        assert!(error.contains("MDX_LEDGER_REPAIR_ENABLED=1"));
        let error =
            require_operator_gates(Some("1"), None).expect_err("repair must require held writes");
        assert!(error.contains("MDX_LEDGER_REPAIR_MAINTENANCE_ACK=1"));
        assert_eq!(require_operator_gates(Some("1"), Some("1")), Ok(()));
    }

    #[test]
    fn post_mutation_failure_takes_the_explicit_rollback_path() {
        let committed = Rc::new(Cell::new(false));
        let rolled_back = Rc::new(Cell::new(false));
        let probe = FinishProbe {
            committed: Rc::clone(&committed),
            rolled_back: Rc::clone(&rolled_back),
        };

        let error = finish_repair_transaction(probe, Err("forced postcondition".to_string()))
            .expect_err("postcondition failures must return an error");

        assert!(!committed.get());
        assert!(rolled_back.get());
        assert!(error.contains("ledger repair rolled back"));
    }
}
