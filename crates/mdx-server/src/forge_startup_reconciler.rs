//! Forge crash-boundary reconciliation at server startup.
//!
//! Startup cleans dead MDx-owned workspaces, validates checkpoints, collects
//! orphan refs and temp files, and marks unfinished direct runs as pending
//! recovery. It never calls a model or spends a run budget. Fleet execution
//! keeps its separate durable conductor and resumes after this sweep.

use mdx_core::{ForgeRunEvent, GovernedWriteIdentity, MdxKernel, Receipt};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StartupReconciliation {
    pub(crate) repositories_scanned: usize,
    pub(crate) valid_checkpoints: usize,
    pub(crate) checkpoint_metadata_quarantined: usize,
    pub(crate) orphan_checkpoint_refs_removed: usize,
    pub(crate) stale_checkpoint_temps_removed: usize,
    pub(crate) stale_transcript_temps_removed: usize,
    pub(crate) stale_worktrees_removed: usize,
    pub(crate) live_worktrees_preserved: usize,
    pub(crate) runs_marked_recovery_pending: usize,
    pub(crate) errors: Vec<String>,
}

impl StartupReconciliation {
    pub(crate) fn summary(&self) -> String {
        format!(
            "repos={} valid_checkpoints={} quarantined_metadata={} orphan_refs_removed={} stale_checkpoint_temps_removed={} stale_transcript_temps_removed={} stale_worktrees_removed={} live_worktrees_preserved={} runs_recovery_pending={} errors={}",
            self.repositories_scanned,
            self.valid_checkpoints,
            self.checkpoint_metadata_quarantined,
            self.orphan_checkpoint_refs_removed,
            self.stale_checkpoint_temps_removed,
            self.stale_transcript_temps_removed,
            self.stale_worktrees_removed,
            self.live_worktrees_preserved,
            self.runs_marked_recovery_pending,
            self.errors.len()
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct OrphanedRun {
    run_id: String,
    tenant_id: String,
    work_item_id: String,
    repo_root: String,
    last_event: String,
    last_turn: u32,
    started: bool,
    fleet_managed: bool,
}

pub(crate) fn reconcile(kernel: &Arc<RwLock<MdxKernel>>) -> Result<StartupReconciliation, String> {
    let (roots, orphaned_runs) = {
        let guard = kernel
            .read()
            .map_err(|_| "kernel lock poisoned during Forge startup scan".to_string())?;
        (
            repository_roots(guard.ledger().entries()),
            orphaned_direct_runs(guard.ledger().entries()),
        )
    };
    let mut report = StartupReconciliation::default();
    for root in roots {
        report.repositories_scanned += 1;
        let checkpoints = crate::forge_workspace_checkpoint::reconcile_repository(&root);
        report.valid_checkpoints += checkpoints.valid_checkpoints;
        report.checkpoint_metadata_quarantined += checkpoints.quarantined_metadata;
        report.orphan_checkpoint_refs_removed += checkpoints.orphan_refs_removed;
        report.stale_checkpoint_temps_removed += checkpoints.stale_temp_files_removed;
        report.errors.extend(
            checkpoints
                .errors
                .into_iter()
                .map(|error| format!("{}: {error}", root.display())),
        );
        let worktrees = crate::harness_worker_workspace::reconcile_stale_worktrees(&root);
        report.stale_worktrees_removed += worktrees.stale_worktrees_removed;
        report.live_worktrees_preserved += worktrees.live_worktrees_preserved;
        report.errors.extend(
            worktrees
                .errors
                .into_iter()
                .map(|error| format!("{}: {error}", root.display())),
        );
    }
    match crate::forge_transcript_store::reconcile_stale_temps() {
        Ok(count) => report.stale_transcript_temps_removed = count,
        Err(error) => report
            .errors
            .push(format!("transcript temp reconciliation: {error}")),
    }

    if !orphaned_runs.is_empty() {
        let mut guard = kernel
            .write()
            .map_err(|_| "kernel lock poisoned during Forge run reconciliation".to_string())?;
        for run in orphaned_runs {
            let repo_root = if run.repo_root.is_empty() {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            } else {
                PathBuf::from(&run.repo_root)
            };
            let checkpoint_available =
                crate::forge_workspace_checkpoint::exists(&repo_root, &run.run_id).unwrap_or(false);
            let transcript_available =
                crate::forge_transcript_store::load_transcript_by_run(&run.run_id).is_some();
            let recovery_available = checkpoint_available || transcript_available;
            let checkpoint_value = checkpoint_available.to_string();
            let transcript_value = transcript_available.to_string();
            let recovery_value = recovery_available.to_string();
            let last_turn = run.last_turn.to_string();
            let detail = if recovery_available {
                format!(
                    "startup found an interrupted run after {}; explicit resume can recover{}{}",
                    run.last_event,
                    if checkpoint_available {
                        " its scoped workspace"
                    } else {
                        ""
                    },
                    if transcript_available {
                        " and transcript"
                    } else {
                        ""
                    }
                )
            } else {
                format!(
                    "startup found an interrupted run after {}; no durable checkpoint or transcript was available",
                    run.last_event
                )
            };
            let identity = GovernedWriteIdentity::local_demo("agent:forge_recovery");
            let recorded = guard.record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: &run.tenant_id,
                    actor_id: "agent:forge_recovery",
                    run_id: &run.run_id,
                    event: "run_recovery_pending",
                    work_item_id: &run.work_item_id,
                    detail: &detail,
                    turn: run.last_turn,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[
                    ("startup_reconciled", "true"),
                    ("interrupted_after_event", run.last_event.as_str()),
                    ("interrupted_after_turn", last_turn.as_str()),
                    ("workspace_checkpoint_available", checkpoint_value.as_str()),
                    ("transcript_available", transcript_value.as_str()),
                    ("explicit_resume_available", recovery_value.as_str()),
                    ("automatic_model_restart", "false"),
                    ("repo_root", run.repo_root.as_str()),
                ],
            );
            match recorded {
                Ok(_) => report.runs_marked_recovery_pending += 1,
                Err(error) => report.errors.push(format!(
                    "could not mark run {} recovery-pending: {error:?}",
                    run.run_id
                )),
            }
        }
    }
    Ok(report)
}

fn repository_roots(receipts: &[Receipt]) -> Vec<PathBuf> {
    let mut roots = BTreeSet::new();
    if let Ok(current) = std::env::current_dir()
        && current.join(".git").exists()
    {
        roots.insert(canonical_or_original(current));
    }
    for receipt in receipts {
        let root = if receipt.kind == "forge.repo.connected" {
            receipt.payload.get("root")
        } else if receipt.kind == "forge.run.event" {
            receipt.payload.get("repo_root")
        } else {
            None
        };
        let Some(root) = root
            .map(String::as_str)
            .filter(|root| !root.trim().is_empty())
        else {
            continue;
        };
        let path = PathBuf::from(root);
        if path.join(".git").exists() {
            roots.insert(canonical_or_original(path));
        }
    }
    roots.into_iter().collect()
}

fn canonical_or_original(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

fn orphaned_direct_runs(receipts: &[Receipt]) -> Vec<OrphanedRun> {
    let mut runs: BTreeMap<String, OrphanedRun> = BTreeMap::new();
    for receipt in receipts
        .iter()
        .filter(|receipt| receipt.kind == "forge.run.event")
    {
        let value = |key: &str| receipt.payload.get(key).map(String::as_str).unwrap_or("");
        let run_id = value("run_id");
        if run_id.is_empty() {
            continue;
        }
        let state = runs
            .entry(run_id.to_string())
            .or_insert_with(|| OrphanedRun {
                run_id: run_id.to_string(),
                tenant_id: receipt.tenant_id.as_str().to_string(),
                ..OrphanedRun::default()
            });
        let event = value("event");
        state.last_event = event.to_string();
        state.last_turn = value("turn").parse().unwrap_or(state.last_turn);
        state.started |= event == "run_started";
        state.fleet_managed |= !value("fleet_id").is_empty()
            || !value("fleet_stream_id").is_empty()
            || value("execution_geometry_lane") == "fleet_stream";
        if !value("work_item_id").is_empty() {
            state.work_item_id = value("work_item_id").to_string();
        }
        if !value("repo_root").is_empty() {
            state.repo_root = value("repo_root").to_string();
        }
    }
    runs.into_values()
        .filter(|run| run.started)
        .filter(|run| !run.fleet_managed)
        .filter(|run| {
            !matches!(
                run.last_event.as_str(),
                "run_finished" | "run_recovery_pending"
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::{ActorId, LoopId, TenantId, TraceId, WorkflowId};
    use std::path::Path;

    #[test]
    fn scan_finds_only_unfinished_direct_runs_and_is_idempotent() {
        let repo = temp_git_repo();
        let mut receipts = vec![
            run_receipt(&repo, "direct_done", "run_started", ""),
            run_receipt(&repo, "direct_done", "run_finished", ""),
            run_receipt(&repo, "direct_interrupted", "run_started", ""),
            run_receipt(&repo, "direct_interrupted", "tool_executed", ""),
            run_receipt(&repo, "fleet_interrupted", "run_started", "fleet_1"),
        ];
        assert!(repository_roots(&receipts).contains(&repo.canonicalize().expect("canonical")));
        let orphaned = orphaned_direct_runs(&receipts);
        assert_eq!(orphaned.len(), 1);
        assert_eq!(orphaned[0].run_id, "direct_interrupted");
        receipts.push(run_receipt(
            &repo,
            "direct_interrupted",
            "run_recovery_pending",
            "",
        ));
        assert!(orphaned_direct_runs(&receipts).is_empty());
        receipts.push(run_receipt(&repo, "direct_interrupted", "run_started", ""));
        assert_eq!(orphaned_direct_runs(&receipts).len(), 1);
        let _ = std::fs::remove_dir_all(repo);
    }

    fn run_receipt(repo: &Path, run_id: &str, event: &str, fleet_id: &str) -> Receipt {
        let mut payload = BTreeMap::new();
        payload.insert("run_id".to_string(), run_id.to_string());
        payload.insert("event".to_string(), event.to_string());
        payload.insert("turn".to_string(), "1".to_string());
        payload.insert("repo_root".to_string(), repo.to_string_lossy().into_owned());
        payload.insert("fleet_id".to_string(), fleet_id.to_string());
        Receipt {
            receipt_id: format!("receipt_{run_id}_{event}"),
            tenant_id: TenantId::new("tenant_test"),
            trace_id: TraceId::new("trace_test"),
            actor_id: ActorId::new("human:test"),
            loop_id: LoopId::new("forge"),
            workflow_id: WorkflowId::new("workflow_test"),
            kind: "forge.run.event".to_string(),
            policy_decision_id: None,
            payload,
            previous_hash: None,
            receipt_timestamp: String::new(),
            hash_version: 1,
            hash: String::new(),
        }
    }

    fn temp_git_repo() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "mdx_forge_startup_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).expect("temp repo");
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(&path)
                .args(["init", "-q"])
                .status()
                .expect("git init")
                .success()
        );
        path
    }
}
