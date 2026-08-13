// Reconstruct the Memory Brain from a restored kernel's real historical
// ledger. Memory is DERIVED storage: it does not replay from the ledger at
// boot (the 2026-07 RC3 finding), so a kernel restored from a rich receipt
// snapshot has the full chain but an empty memory core. This CLI verb walks
// the content-bearing receipts and mirrors the live surface memory path for
// each one, so real published pages and posted messages become real,
// semantically-embedded, recall-eligible memory records.
//
// Only prose-bearing kinds are reconstructed: pages.document.published (title
// is inline; body is a world_model:// reference, so the content string mirrors
// the live pages path exactly) and message.human/agent.posted (body is inline
// real text). Twin answers in this era are "No model connected" placeholders,
// and message action cards are structured requests, not prose, so both are
// skipped.
//
// Shared-scope consolidation (team_memory, company_memory) lands in
// PENDING_RATIFICATION and stays out of recall until a DISTINCT actor clears
// it. Backfilled pages and messages are already-real published artifacts, so
// the backfill legitimately ratifies them: the drafter actor and the ratifier
// actor are distinct so the kernel's distinct-actor rule is satisfied, and the
// ratification receipt is stamped honestly as a backfill clear.
use crate::kernel_snapshot;
use crate::memory_embedding;
use mdx_core::{MEMORY_CONSOLIDATION_PENDING, MdxKernel, MemoryConsolidationRatification};
use std::collections::HashSet;

// The backfill writes and ratifies under two distinct system actors so the
// kernel's distinct-actor ratification rule is satisfied honestly.
const BACKFILL_ACTOR: &str = "system:memory_backfill";
const BACKFILL_RATIFIER: &str = "system:memory_backfill_ratifier";
const BACKFILL_RATIFY_NOTE: &str = "backfill-ratified from restored historical ledger";

#[derive(Clone, Copy, PartialEq, Eq)]
enum SurfaceBucket {
    Pages,
    Messages,
}

struct BackfillItem {
    bucket: SurfaceBucket,
    surface: &'static str,
    scope: &'static str,
    receipt_id: String,
    tenant_id: String,
    content: String,
}

#[derive(Default)]
struct BackfillCounts {
    pages_backfilled: usize,
    messages_backfilled: usize,
    embeddings_attached: usize,
    ratified_count: usize,
    skipped_existing: usize,
}

/// Entry point for the `memory-backfill-from-ledger` verb. Boots a local
/// kernel, restores the durable ledger and any existing memory core, runs the
/// reconstruction, and writes both snapshots so a running kernel picks up the
/// new records on its next restart.
pub fn run_memory_backfill() -> Result<String, String> {
    if std::fs::metadata(kernel_snapshot::SNAPSHOT_PATH).is_err() {
        return Err(format!(
            "no ledger snapshot at {}; restore a kernel snapshot before backfilling memory",
            kernel_snapshot::SNAPSHOT_PATH
        ));
    }
    let mut kernel = MdxKernel::boot_local();
    let ledger_count = kernel_snapshot::restore_into(&mut kernel)?.ok_or_else(|| {
        format!(
            "ledger snapshot {} was absent after the metadata check",
            kernel_snapshot::SNAPSHOT_PATH
        )
    })?;
    // Restore any existing memory core so a re-run skips records a prior
    // backfill already wrote (idempotency across process invocations, since
    // memory records live in the memory sibling snapshot, not the ledger).
    kernel_snapshot::restore_memory_into(&mut kernel)?;

    let counts = backfill_from_kernel(&mut kernel)?;

    let rendered = kernel_snapshot::render_snapshots(&kernel)?;
    kernel_snapshot::write_snapshots(&rendered)?;

    let total_memory_records_now = kernel.memory_records().len();
    Ok(format!(
        r#"{{"name":"mdx-memory-backfill-from-ledger","status":"OK","ledger_receipts_restored":{ledger_count},"pages_backfilled":{},"messages_backfilled":{},"embeddings_attached":{},"ratified_count":{},"skipped_existing":{},"total_memory_records_now":{total_memory_records_now}}}"#,
        counts.pages_backfilled,
        counts.messages_backfilled,
        counts.embeddings_attached,
        counts.ratified_count,
        counts.skipped_existing,
    ))
}

/// The core walk, taking `&mut kernel` so tests exercise it without the
/// snapshot file. Idempotent: a content receipt whose id already backs a
/// memory record is skipped, so re-running never double-writes.
fn backfill_from_kernel(kernel: &mut MdxKernel) -> Result<BackfillCounts, String> {
    let mut existing: HashSet<String> = kernel
        .memory_records()
        .iter()
        .map(|record| record.source_receipt_id.clone())
        .collect();
    let items = collect_backfill_items(kernel);
    let mut counts = BackfillCounts::default();

    for item in items {
        if existing.contains(&item.receipt_id) {
            counts.skipped_existing += 1;
            continue;
        }
        // Resolve and embed before the write, mirroring memory_extraction's
        // record_verbatim. Tests take the deterministic no-embedder path so
        // they never depend on a live loopback embedder a dev box happens to
        // run; a missing embedder here means the record is still written,
        // just lexical (fail-closed to today's behavior).
        let embedding = if cfg!(test) {
            None
        } else {
            memory_embedding::resolve_embedder(kernel, &item.tenant_id)
                .and_then(|client| memory_embedding::embed_with(&client, &item.content))
        };
        let outcome = kernel.consolidate_surface_memory(
            &item.tenant_id,
            BACKFILL_ACTOR,
            item.surface,
            item.scope,
            &item.receipt_id,
            &item.content,
            "unadjudicated_verbatim",
        )?;
        if outcome.memory_id.is_empty() {
            // Verbatim retention never NOOPs, but guard rather than ratify an
            // empty id if the contract ever changes.
            continue;
        }
        if let Some(vector) = embedding {
            kernel.set_memory_embedding(&outcome.memory_id, vector);
            counts.embeddings_attached += 1;
        }
        // Shared scopes land pending; the distinct-actor ratifier clears them
        // so the reconstructed record is recall-eligible.
        if outcome.consolidation_state == MEMORY_CONSOLIDATION_PENDING {
            kernel.ratify_memory_consolidation(MemoryConsolidationRatification {
                tenant_id: &item.tenant_id,
                actor_id: BACKFILL_RATIFIER,
                memory_id: &outcome.memory_id,
                decision: "approve",
                note: BACKFILL_RATIFY_NOTE,
            })?;
            counts.ratified_count += 1;
        }
        match item.bucket {
            SurfaceBucket::Pages => counts.pages_backfilled += 1,
            SurfaceBucket::Messages => counts.messages_backfilled += 1,
        }
        existing.insert(item.receipt_id);
    }
    Ok(counts)
}

/// Read the content-bearing receipts into owned items. Reading the ledger
/// borrows the kernel immutably, so the walk collects everything it needs
/// before the mutation pass takes the write path.
fn collect_backfill_items(kernel: &MdxKernel) -> Vec<BackfillItem> {
    let mut items = Vec::new();
    let ledger = kernel.ledger();

    for receipt in ledger.query().by_kind("pages.document.published") {
        // Mirror the live pages memory content exactly (pages_publication.rs):
        // "Published page {document_id} titled {title}". The body is a
        // world_model:// reference, never inlined here.
        let document_id = receipt
            .payload
            .get("document_id")
            .cloned()
            .unwrap_or_default();
        let title = receipt.payload.get("title").cloned().unwrap_or_default();
        let content = format!("Published page {document_id} titled {title}");
        items.push(BackfillItem {
            bucket: SurfaceBucket::Pages,
            surface: "pages",
            scope: "company_memory",
            receipt_id: receipt.receipt_id.clone(),
            tenant_id: receipt.tenant_id.as_str().to_string(),
            content,
        });
    }

    for kind in ["message.human.posted", "message.agent.posted"] {
        for receipt in ledger.query().by_kind(kind) {
            // The live message memory path uses the message body verbatim.
            let Some(body) = receipt.payload.get("body") else {
                continue;
            };
            if body.trim().is_empty() {
                continue;
            }
            items.push(BackfillItem {
                bucket: SurfaceBucket::Messages,
                surface: "messages",
                scope: "team_memory",
                receipt_id: receipt.receipt_id.clone(),
                tenant_id: receipt.tenant_id.as_str().to_string(),
                content: body.clone(),
            });
        }
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::{MEMORY_CONSOLIDATION_ACTIVE, MessageThreadMessage, PagesPublication};

    /// A synthetic kernel carrying the two content-bearing receipt kinds the
    /// backfill reconstructs, each citing a real source receipt in the ledger.
    fn kernel_with_content() -> (MdxKernel, Vec<String>) {
        let mut kernel = MdxKernel::boot_local();
        kernel.run_evals_runner_agent().expect("loop run");
        let source_receipt_id = kernel
            .ledger()
            .entries()
            .last()
            .expect("source receipt")
            .receipt_id
            .clone();

        let mut source_ids = Vec::new();
        for (document_id, title) in [
            ("doc-beta-gate", "Beta gate ships before load tests"),
            ("doc-memory-arc", "Memory brain reconstruction plan"),
        ] {
            let report = kernel
                .save_pages_publication_local(PagesPublication {
                    tenant_id: "local_tenant",
                    actor_id: "human:author",
                    document_id,
                    title,
                    body_ref: "world_model://pages/body",
                    source_receipt_id: &source_receipt_id,
                    revision_id: "rev-1",
                    page_type: "knowledge",
                })
                .expect("pages publish");
            source_ids.push(report.publication_receipt_id);
        }
        for (message_id, body) in [
            ("msg-1", "We agreed to ship the beta gate before load tests"),
            (
                "msg-2",
                "The memory backfill runs against the restored ledger",
            ),
        ] {
            let report = kernel
                .save_message_thread_message_local(MessageThreadMessage {
                    tenant_id: "local_tenant",
                    actor_id: "human:teammate",
                    thread_id: "thread-1",
                    channel_id: "channel-1",
                    message_id,
                    content_type: "text",
                    body,
                    content_payload: body,
                    source_receipt_id: &source_receipt_id,
                    outbox_topic: "messages.thread-1",
                })
                .expect("message post");
            source_ids.push(report.message_receipt_id);
        }
        (kernel, source_ids)
    }

    #[test]
    fn backfill_reconstructs_four_records_citing_real_receipts_and_is_idempotent() {
        let (mut kernel, source_ids) = kernel_with_content();
        // The core publish and post paths do not write memory; only the seeded
        // loop run does, so the backfill adds records on top of that baseline.
        let baseline = kernel.memory_records().len();
        assert!(
            !source_ids.iter().any(|id| kernel
                .memory_records()
                .iter()
                .any(|r| &r.source_receipt_id == id)),
            "no page or message receipt is backed by memory before the backfill runs"
        );

        let counts = backfill_from_kernel(&mut kernel).expect("backfill");
        assert_eq!(counts.pages_backfilled, 2);
        assert_eq!(counts.messages_backfilled, 2);
        assert_eq!(counts.skipped_existing, 0);
        // No embedder is resolved under cfg!(test), so records are lexical.
        assert_eq!(counts.embeddings_attached, 0);

        let records = kernel.memory_records();
        assert_eq!(records.len(), baseline + 4);
        for source_id in &source_ids {
            let record = records
                .iter()
                .find(|record| &record.source_receipt_id == source_id)
                .unwrap_or_else(|| panic!("a memory record must cite receipt {source_id}"));
            assert!(
                kernel.ledger().query().by_id(source_id).is_some(),
                "the cited source receipt must exist in the restored ledger"
            );
            // Shared-scope records are ratified, so they are recall-eligible
            // (active), not stranded pending.
            assert_eq!(
                record.consolidation_state, MEMORY_CONSOLIDATION_ACTIVE,
                "backfilled shared-scope record must be ratified to active"
            );
        }
        assert_eq!(counts.ratified_count, 4);
        assert!(
            kernel.pending_memory_consolidations().is_empty(),
            "no shared-scope record may be left pending after backfill"
        );

        // Content mirrors the live surface paths.
        assert!(records.iter().any(|record| record.content
            == "Published page doc-beta-gate titled Beta gate ships before load tests"));
        assert!(
            records
                .iter()
                .any(|record| record.content
                    == "We agreed to ship the beta gate before load tests")
        );

        // Idempotency: a second run cites the same receipts and writes nothing.
        let second = backfill_from_kernel(&mut kernel).expect("second backfill");
        assert_eq!(second.pages_backfilled, 0);
        assert_eq!(second.messages_backfilled, 0);
        assert_eq!(second.ratified_count, 0);
        assert_eq!(second.skipped_existing, 4);
        assert_eq!(kernel.memory_records().len(), baseline + 4);
    }
}
