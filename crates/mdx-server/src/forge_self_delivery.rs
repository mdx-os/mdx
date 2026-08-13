// Arc 7b: the self-delivery authorization core (ADR 0488).
//
// Arc 7a lifted the ship const behind a self-delivery scope and built the
// self-modification firewall. This module is the decision the delivery
// mechanics consult: given a finished run, how far does its envelope let it
// carry its own delivery - and does its actual diff clear the firewall and
// earn the fresh-eyes concurrence? Every answer is fail-closed. A missing
// envelope, a revoked or expired one, a protected path in the diff, or fewer
// than the required independent approvals all deny self-delivery and leave the
// human ship door exactly where it was.
//
// Nothing here deploys. Self-delivery is repo self-improvement; the deployment
// const refusal on the envelope is untouched.
use crate::forge_finish_evaluator;
use mdx_core::{MdxKernel, SelfDeliveryScope, path_is_protected};
use std::sync::{Arc, RwLock};

/// The number of independent fresh-eyes approvals a self-delivery must earn.
/// Floor of 1 so the gate can never be configured away; default 2 (ADR 0488).
pub(crate) fn required_concurrences() -> u32 {
    std::env::var("MDX_FORGE_SELF_DELIVERY_CONCURRENCES")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value >= 1)
        .unwrap_or(2)
}

/// The bounded number of governed fix iterations a self-converge run may spend
/// driving CI to green before it escalates to a human. Floor of 1, default 3.
/// The ceiling is the anti-runaway backstop: convergence never loops forever,
/// it escalates.
pub(crate) fn max_ci_fix_iterations() -> u32 {
    std::env::var("MDX_FORGE_SELF_DELIVERY_MAX_CI_FIXES")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value >= 1)
        .unwrap_or(3)
}

/// How many CI fix runs have already been spawned for a base run, counted from
/// their run_started receipts. This is what the ceiling bounds.
pub(crate) fn ci_fix_iteration_count(kernel: &Arc<RwLock<MdxKernel>>, base_run_id: &str) -> u32 {
    let Ok(guard) = kernel.read() else {
        return 0;
    };
    guard
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| {
            receipt.payload.get("event").map(String::as_str) == Some("run_started")
                && receipt
                    .payload
                    .get("self_delivery_ci_fix")
                    .map(String::as_str)
                    == Some("true")
                && receipt
                    .payload
                    .get("self_delivery_base_run_id")
                    .map(String::as_str)
                    == Some(base_run_id)
        })
        .count() as u32
}

/// The path scopes an envelope allows its runs to write, read from its recorded
/// receipt (latest amendment wins). The self-delivery fix run's writes are
/// bounded to these; a protected path among them is still caught at the merge
/// gate, so the firewall holds regardless.
pub(crate) fn envelope_allowed_path_scopes(
    kernel: &Arc<RwLock<MdxKernel>>,
    envelope_id: &str,
) -> Vec<String> {
    let Ok(guard) = kernel.read() else {
        return Vec::new();
    };
    guard
        .ledger()
        .query()
        .by_kind("autonomy.envelope.recorded")
        .iter()
        .chain(
            guard
                .ledger()
                .query()
                .by_kind("autonomy.envelope.amended")
                .iter(),
        )
        .filter(|receipt| {
            receipt.payload.get("envelope_id").map(String::as_str) == Some(envelope_id)
        })
        .filter_map(|receipt| receipt.payload.get("allowed_path_scopes").cloned())
        .next_back()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|scope| !scope.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The decision the delivery mechanics consult for a run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SelfDeliveryAuthorization {
    /// The effective self-delivery scope the run's active envelope grants.
    pub scope: SelfDeliveryScope,
    /// The envelope backing the grant, for the receipt chain. Empty when none.
    pub envelope_id: String,
    /// Some(reason) means DENY and escalate; None means the requested tier is
    /// authorized. A denial never falls through to delivery.
    pub denied_reason: Option<String>,
}

impl SelfDeliveryAuthorization {
    #[cfg(test)]
    pub fn is_authorized(&self) -> bool {
        self.denied_reason.is_none()
    }

    fn denied(scope: SelfDeliveryScope, envelope_id: String, reason: impl Into<String>) -> Self {
        Self {
            scope,
            envelope_id,
            denied_reason: Some(reason.into()),
        }
    }
}

/// The envelope id a run bound itself to at admission, read from its
/// run_started receipt (`autonomy_envelope_id`). Empty when the run named no
/// envelope.
pub(crate) fn run_envelope_id(kernel: &Arc<RwLock<MdxKernel>>, run_id: &str) -> String {
    let Ok(kernel) = kernel.read() else {
        return String::new();
    };
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .find(|receipt| {
            receipt.payload.get("run_id").map(String::as_str) == Some(run_id)
                && receipt.payload.get("event").map(String::as_str) == Some("run_started")
        })
        .and_then(|receipt| receipt.payload.get("autonomy_envelope_id").cloned())
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// The operator intent a run was admitted with, read from its run_started
/// receipt. This is the task a fresh-eyes reviewer must judge the diff
/// against: the PR handoff title ("Forge run X: ready_for_review") names no
/// work at all, and a reviewer given that as the task can only guess, which
/// turns the concurrence gate into a coin flip instead of a check. Empty when
/// the run recorded no intent.
pub(crate) fn run_operator_intent(kernel: &Arc<RwLock<MdxKernel>>, run_id: &str) -> String {
    let Ok(kernel) = kernel.read() else {
        return String::new();
    };
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .find(|receipt| {
            receipt.payload.get("run_id").map(String::as_str) == Some(run_id)
                && receipt.payload.get("event").map(String::as_str) == Some("run_started")
        })
        .and_then(|receipt| receipt.payload.get("operator_intent").cloned())
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// The effective self-delivery scope for a run: the scope recorded on its
/// envelope (latest amendment wins), but ONLY when the envelope is still active,
/// the same authority blocker the run loop polls. A missing, revoked, or expired
/// envelope resolves to None, so a lapsed grant grants nothing.
pub(crate) fn run_self_delivery_scope(
    kernel: &Arc<RwLock<MdxKernel>>,
    run_id: &str,
) -> (SelfDeliveryScope, String) {
    let envelope_id = run_envelope_id(kernel, run_id);
    if envelope_id.is_empty() {
        return (SelfDeliveryScope::None, envelope_id);
    }
    let Ok(guard) = kernel.read() else {
        return (SelfDeliveryScope::None, envelope_id);
    };
    // A revoked or expired envelope is not a self-delivery authority, whatever
    // scope it once recorded.
    if guard
        .autonomy_envelope_authority_blocker(&envelope_id)
        .is_some()
    {
        return (SelfDeliveryScope::None, envelope_id);
    }
    // Latest recorded/amended scope for this envelope wins.
    let scope = guard
        .ledger()
        .query()
        .by_kind("autonomy.envelope.recorded")
        .iter()
        .chain(
            guard
                .ledger()
                .query()
                .by_kind("autonomy.envelope.amended")
                .iter(),
        )
        .filter(|receipt| {
            receipt.payload.get("envelope_id").map(String::as_str) == Some(envelope_id.as_str())
        })
        .filter_map(|receipt| receipt.payload.get("self_delivery_scope").cloned())
        .map(|value| SelfDeliveryScope::from_wire(&value))
        .next_back()
        .unwrap_or(SelfDeliveryScope::None);
    (scope, envelope_id)
}

/// The firewall run against the run's ACTUAL committed diff for the WHOLE
/// branch relative to its merge base, so a protected edit hidden in an earlier
/// commit cannot slip past a clean tip. Every changed file must sit outside
/// PROTECTED_PATHS. Returns Some(reason) naming the first protected path -
/// escalate - or None when the whole diff is clear. The firewall FAILS CLOSED:
/// a diff that cannot be read, or a merge base that cannot be resolved, is
/// treated as unclear and escalates; it never falls back to the tip commit.
pub(crate) fn diff_firewall_reason(
    kernel: &Arc<RwLock<MdxKernel>>,
    run_id: &str,
    base_branch: &str,
) -> Option<String> {
    let files =
        match crate::forge_diff_route::run_diff_files_against_base(kernel, run_id, base_branch) {
            Ok(Some(files)) => files,
            // No branch, or an unresolvable merge base: FAIL CLOSED. The firewall
            // must never fall back to the tip-only diff, which could hide a
            // protected edit made in an earlier commit.
            Ok(None) => return Some("self_delivery_firewall:base_unresolvable".to_string()),
            Err(_) => return Some("self_delivery_firewall:diff_unreadable".to_string()),
        };
    if files.is_empty() {
        return Some("self_delivery_firewall:empty_diff".to_string());
    }
    for file in &files {
        if path_is_protected(&file.path) {
            return Some(format!("self_modification_firewall:{}", file.path));
        }
    }
    None
}

/// True when the run's diff earns at least `required` independent fresh-eyes
/// APPROVE verdicts. Fail-CLOSED, the opposite of the finish evaluator's
/// fail-open default: no reviewer model, an unreadable diff, or any REVISE
/// means self-delivery is NOT authorized. Self-delivery must earn its
/// concurrence; it is never granted by absence.
///
/// This gate does NOT re-check protected paths: `authorize_self_delivery` runs
/// `diff_firewall_reason` first and denies before it ever reaches concurrence,
/// so the fail-closed base guarantee (an unresolvable merge base escalates)
/// comes from the firewall. The reviewer may fall back to the tip diff for
/// readability without weakening that guarantee.
pub(crate) fn concurrence_reached(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    run_id: &str,
    task: &str,
    base_branch: &str,
    required: u32,
) -> bool {
    // The reviewer reads the whole branch against its merge base when the base
    // resolves; a base that cannot be resolved falls back to the tip commit so
    // fresh eyes still see the change (the firewall, not the reviewer, is what
    // fails closed on an unresolvable base).
    let diff =
        match crate::forge_diff_route::run_diff_text_against_base(kernel, run_id, base_branch) {
            Ok(Some(diff)) if !diff.trim().is_empty() => diff,
            _ => match crate::forge_diff_route::run_diff_text(kernel, run_id) {
                Ok(Some(diff)) if !diff.trim().is_empty() => diff,
                _ => return false,
            },
        };
    let mut approvals = 0;
    for _ in 0..required {
        match forge_finish_evaluator::evaluate_finish(kernel, tenant_id, task, &diff, "") {
            Some(verdict) if verdict.approved => approvals += 1,
            // A REVISE or a missing reviewer model both fail the concurrence.
            _ => return false,
        }
    }
    approvals >= required
}

/// The authorization decision for a run at a requested delivery tier. Checks,
/// in fail-closed order: the run's envelope grants at least the requested
/// scope; the diff clears the self-modification firewall; and (for any actual
/// delivery tier) the fresh-eyes concurrence is earned. Any failure denies with
/// a reason the caller records and escalates.
pub(crate) fn authorize_self_delivery(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    run_id: &str,
    task: &str,
    base_branch: &str,
    required_scope: SelfDeliveryScope,
) -> SelfDeliveryAuthorization {
    let (scope, envelope_id) = run_self_delivery_scope(kernel, run_id);
    if scope < required_scope || !scope.is_self_delivery() {
        return SelfDeliveryAuthorization::denied(
            scope,
            envelope_id,
            format!(
                "self_delivery_scope_insufficient:have={},need={}",
                scope.as_str(),
                required_scope.as_str()
            ),
        );
    }
    if let Some(reason) = diff_firewall_reason(kernel, run_id, base_branch) {
        return SelfDeliveryAuthorization::denied(scope, envelope_id, reason);
    }
    let required = required_concurrences();
    // Judge the diff against what the run was actually asked to do. The
    // caller's task string is a fallback only: at both delivery call sites it
    // is the PR handoff title, which carries no task content.
    let intent = run_operator_intent(kernel, run_id);
    let review_task = if intent.is_empty() { task } else { &intent };
    if !concurrence_reached(
        kernel,
        tenant_id,
        run_id,
        review_task,
        base_branch,
        required,
    ) {
        return SelfDeliveryAuthorization::denied(
            scope,
            envelope_id,
            format!("self_delivery_concurrence_not_reached:required={required}"),
        );
    }
    SelfDeliveryAuthorization {
        scope,
        envelope_id,
        denied_reason: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::{
        AutonomyEnvelopeRecordRequest, AutonomyEnvelopeRevokeRequest, AutonomyPolicyEnvelope,
        AutonomyRiskClass, GovernedWriteIdentity, HarnessApprovalMode, HarnessBudgetPolicy,
        HarnessPermissionPolicy, HarnessRunManifest, HarnessRunMode, HarnessTrustBoundary,
        LocalAutonomyEnvelopeRegistry,
    };

    fn boot() -> Arc<RwLock<MdxKernel>> {
        Arc::new(RwLock::new(MdxKernel::boot_local()))
    }

    // A minimal valid manifest - the envelope registry reads only tenant_id and
    // actor_id from it for correlation, but the struct requires every field.
    fn test_manifest() -> HarnessRunManifest<'static> {
        HarnessRunManifest {
            run_id: "self_delivery_test_manifest",
            tenant_id: "local_tenant",
            actor_id: "human_platform_lead",
            origin_surface: "forge_intake",
            mode: HarnessRunMode::PlanOnly,
            goal_summary: "record a self-delivery envelope for tests",
            definition_of_done: "envelope recorded",
            input_artifacts: &["docs/FORGE-AUTONOMOUS-DELIVERY.md"],
            allowed_write_scope: &["crates/mdx-core/src/"],
            blocked_paths: &[".git/", ".env", ".mdx-local/"],
            allowed_tools: &[],
            blocked_tools: &["shell", "git", "patch", "browser", "mcp", "network"],
            default_parallelism: 1,
            allowed_model_profiles: &["deterministic_stub"],
            provider_fail_closed: true,
            allowed_context_sources: &["manifest", "source_map", "verification_manifest"],
            context_redaction_required: true,
            permission_policy: HarnessPermissionPolicy::plan_only_local(),
            budget_policy: HarnessBudgetPolicy {
                max_turns: 1,
                max_tool_calls: 0,
                max_runtime_ms: 250,
                max_context_tokens: 4096,
                max_output_tokens: 1024,
                max_event_bytes: 8192,
                max_transcript_bytes: 16384,
                max_cost_cents: 0,
            },
            quality_gates: &["cargo test -p mdx-core"],
            approval_mode: HarnessApprovalMode::PlanHash,
            approval_receipt_required: true,
            required_receipt_kinds: &[
                "harness.run.admitted",
                "harness.plan.emitted",
                "harness.write.refused",
                "harness.provider.profile.selected",
            ],
            telemetry_redaction_required: true,
            exit_criteria: &["evidence_ready_for_human", "human_decision_recorded"],
            trust_boundary: HarnessTrustBoundary {
                workspace_trusted: true,
                project_local_execution_allowed: false,
                mcp_startup_allowed: false,
                network_allowed: false,
                secrets_available: false,
                approval_receipt_id: Some("human_approved_self_delivery_test"),
            },
            policy_profile: Some("core"),
            enterprise_pack: Some("none"),
        }
    }

    fn record_self_merge_envelope(kernel: &Arc<RwLock<MdxKernel>>, envelope_id: &str) -> String {
        let mut guard = kernel.write().expect("kernel");
        LocalAutonomyEnvelopeRegistry
            .record(
                &mut guard,
                &test_manifest(),
                &AutonomyEnvelopeRecordRequest {
                    envelope: AutonomyPolicyEnvelope {
                        envelope_id,
                        owner_id: "human_platform_lead",
                        preapproved_work_classes: &["self_improvement"],
                        max_risk_class: AutonomyRiskClass::Low,
                        allowed_path_scopes: &["crates/", "docs/"],
                        rollback_required: false,
                        eval_required: false,
                        escalation_triggers: &[],
                        expires_at: "2030-01-01T00:00:00Z",
                        active: true,
                        self_delivery_scope: SelfDeliveryScope::SelfMerge,
                    },
                    recorded_by: "human_platform_lead",
                },
            )
            .expect("envelope recorded")
    }

    fn record_run_started(kernel: &Arc<RwLock<MdxKernel>>, run_id: &str, envelope_id: &str) {
        let mut guard = kernel.write().expect("kernel");
        guard
            .record_forge_run_event_with_evidence_fields(
                mdx_core::ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "agent:forge",
                    run_id,
                    event: "run_started",
                    work_item_id: "",
                    detail: "run started",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &GovernedWriteIdentity::local_demo("agent:forge"),
                &[("autonomy_envelope_id", envelope_id)],
            )
            .expect("run_started recorded");
    }

    #[test]
    fn run_operator_intent_reads_the_admitted_task_not_the_pr_title() {
        let kernel = boot();
        {
            let mut guard = kernel.write().expect("kernel");
            guard
                .record_forge_run_event_with_evidence_fields(
                    mdx_core::ForgeRunEvent {
                        tenant_id: "local_tenant",
                        actor_id: "agent:forge",
                        run_id: "run_with_intent",
                        event: "run_started",
                        work_item_id: "",
                        detail: "run started",
                        turn: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &GovernedWriteIdentity::local_demo("agent:forge"),
                    &[
                        ("autonomy_envelope_id", "env_intent"),
                        (
                            "operator_intent",
                            "add a short subsection to the delivery doc",
                        ),
                    ],
                )
                .expect("run_started recorded");
        }
        assert_eq!(
            run_operator_intent(&kernel, "run_with_intent"),
            "add a short subsection to the delivery doc"
        );
        // A run that recorded no intent yields empty, so the caller's task
        // string still applies as the fallback.
        record_run_started(&kernel, "run_without_intent", "");
        assert_eq!(run_operator_intent(&kernel, "run_without_intent"), "");
    }

    #[test]
    fn no_envelope_means_no_self_delivery_scope() {
        let kernel = boot();
        record_run_started(&kernel, "run_no_env", "");
        let (scope, envelope_id) = run_self_delivery_scope(&kernel, "run_no_env");
        assert_eq!(scope, SelfDeliveryScope::None);
        assert!(envelope_id.is_empty());
    }

    #[test]
    fn active_self_merge_envelope_resolves_its_scope() {
        let kernel = boot();
        record_self_merge_envelope(&kernel, "env_self_merge");
        record_run_started(&kernel, "run_with_env", "env_self_merge");
        let (scope, envelope_id) = run_self_delivery_scope(&kernel, "run_with_env");
        assert_eq!(scope, SelfDeliveryScope::SelfMerge);
        assert_eq!(envelope_id, "env_self_merge");
    }

    #[test]
    fn revoked_envelope_grants_nothing_even_with_a_recorded_scope() {
        let kernel = boot();
        let receipt_id = record_self_merge_envelope(&kernel, "env_to_revoke");
        record_run_started(&kernel, "run_revoked", "env_to_revoke");
        // The kill switch: revoke the envelope.
        {
            let mut guard = kernel.write().expect("kernel");
            LocalAutonomyEnvelopeRegistry
                .revoke(
                    &mut guard,
                    &test_manifest(),
                    &AutonomyEnvelopeRevokeRequest {
                        envelope_receipt_id: &receipt_id,
                        revoked_by: "human_platform_lead",
                        revocation_reason: "kill switch test",
                    },
                )
                .expect("revoked");
        }
        let (scope, _) = run_self_delivery_scope(&kernel, "run_revoked");
        assert_eq!(
            scope,
            SelfDeliveryScope::None,
            "a revoked envelope is not a self-delivery authority"
        );
    }

    #[test]
    fn authorization_denies_below_the_required_scope() {
        let kernel = boot();
        // No envelope at all: scope None, cannot meet SelfOpenPr.
        record_run_started(&kernel, "run_unscoped", "");
        let auth = authorize_self_delivery(
            &kernel,
            "local_tenant",
            "run_unscoped",
            "fix a test",
            "main",
            SelfDeliveryScope::SelfOpenPr,
        );
        assert!(!auth.is_authorized());
        assert!(
            auth.denied_reason
                .as_deref()
                .unwrap()
                .starts_with("self_delivery_scope_insufficient")
        );
    }

    #[test]
    fn concurrence_is_fail_closed_without_a_reviewer_model() {
        // No reviewer model is configured in a plain boot, and there is no
        // committed diff, so the concurrence must be false - self-delivery is
        // never granted by absence.
        let kernel = boot();
        assert!(!concurrence_reached(
            &kernel,
            "local_tenant",
            "run_no_diff",
            "fix a test",
            "main",
            2
        ));
    }

    #[test]
    fn max_ci_fix_iterations_has_a_floor_of_one() {
        unsafe {
            std::env::set_var("MDX_FORGE_SELF_DELIVERY_MAX_CI_FIXES", "0");
        }
        assert!(max_ci_fix_iterations() >= 1);
        unsafe {
            std::env::remove_var("MDX_FORGE_SELF_DELIVERY_MAX_CI_FIXES");
        }
    }

    #[test]
    fn ci_fix_iteration_count_tracks_spawned_fix_runs_for_a_base_run() {
        let kernel = boot();
        assert_eq!(ci_fix_iteration_count(&kernel, "base_run"), 0);
        // Record two fix-run starts for base_run and one for a different run.
        for (fix_id, base) in [
            ("base_run_ci_fix_001", "base_run"),
            ("base_run_ci_fix_002", "base_run"),
            ("other_run_ci_fix_001", "other_run"),
        ] {
            let mut guard = kernel.write().expect("kernel");
            guard
                .record_forge_run_event_with_evidence_fields(
                    mdx_core::ForgeRunEvent {
                        tenant_id: "local_tenant",
                        actor_id: "agent:forge_self_delivery",
                        run_id: fix_id,
                        event: "run_started",
                        work_item_id: "",
                        detail: "branch=b",
                        turn: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &GovernedWriteIdentity::local_demo("agent:forge_self_delivery"),
                    &[
                        ("self_delivery_ci_fix", "true"),
                        ("self_delivery_base_run_id", base),
                    ],
                )
                .expect("fix run recorded");
        }
        assert_eq!(ci_fix_iteration_count(&kernel, "base_run"), 2);
        assert_eq!(ci_fix_iteration_count(&kernel, "other_run"), 1);
    }

    #[test]
    fn envelope_allowed_path_scopes_reads_the_recorded_scopes() {
        let kernel = boot();
        record_self_merge_envelope(&kernel, "env_scopes");
        let scopes = envelope_allowed_path_scopes(&kernel, "env_scopes");
        assert_eq!(scopes, vec!["crates/".to_string(), "docs/".to_string()]);
    }

    #[test]
    fn required_concurrences_has_a_floor_of_one() {
        unsafe {
            std::env::set_var("MDX_FORGE_SELF_DELIVERY_CONCURRENCES", "0");
        }
        assert!(required_concurrences() >= 1);
        unsafe {
            std::env::remove_var("MDX_FORGE_SELF_DELIVERY_CONCURRENCES");
        }
    }

    // The keystone of the base-aware firewall: a branch whose TIP commit is
    // clean but whose earlier commit edited a PROTECTED_PATHS file must still
    // be refused. A tip-only firewall would miss the protected edit; the
    // base-aware firewall sees the whole branch against its merge base.
    #[test]
    fn firewall_refuses_a_protected_edit_hidden_in_an_earlier_commit() {
        use std::process::Command;

        let temp = std::env::temp_dir().join(format!(
            "mdx_self_delivery_firewall_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&temp).expect("create temp repo dir");

        let git = |args: &[&str]| {
            let output = Command::new("git")
                .arg("-C")
                .arg(&temp)
                .args(args)
                .output()
                .expect("run git");
            assert!(
                output.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
            output
        };

        git(&["init", "-q"]);
        git(&["config", "user.email", "forge@example.com"]);
        git(&["config", "user.name", "Forge Test"]);
        git(&["config", "commit.gpgsign", "false"]);

        // Base branch (reviewable shape) with an initial clean commit.
        git(&["checkout", "-q", "-b", "forge/base"]);
        std::fs::write(temp.join("README.md"), "start\n").expect("write readme");
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "initial commit"]);

        // Work branch off the base.
        git(&["checkout", "-q", "-b", "forge/run-firewall-test"]);
        // Commit 1: edit a PROTECTED_PATHS file (a workflow file).
        let workflow_dir = temp.join(".github").join("workflows");
        std::fs::create_dir_all(&workflow_dir).expect("create workflow dir");
        let protected = ".github/workflows/ci.yml";
        assert!(
            path_is_protected(protected),
            "expected {protected} to be protected"
        );
        std::fs::write(workflow_dir.join("ci.yml"), "name: ci\n").expect("write workflow");
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "edit protected workflow"]);
        // Commit 2 (the tip): edit a clean file only.
        std::fs::write(temp.join("README.md"), "start\nmore\n").expect("update readme");
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "edit clean file"]);

        // Wire a run whose branch is the work branch and whose repo_root is the
        // temp repo, so the firewall reads this branch against the base.
        let kernel = boot();
        {
            let mut guard = kernel.write().expect("kernel");
            guard
                .record_forge_run_event_with_evidence_fields(
                    mdx_core::ForgeRunEvent {
                        tenant_id: "local_tenant",
                        actor_id: "agent:forge",
                        run_id: "run_firewall_earlier_commit",
                        event: "run_started",
                        work_item_id: "",
                        detail: "branch=forge/run-firewall-test",
                        turn: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &GovernedWriteIdentity::local_demo("agent:forge"),
                    &[("repo_root", temp.to_str().unwrap())],
                )
                .expect("run_started recorded");
        }

        let reason = diff_firewall_reason(&kernel, "run_firewall_earlier_commit", "forge/base");
        assert!(
            reason
                .as_deref()
                .is_some_and(|reason| reason.starts_with("self_modification_firewall:")),
            "the firewall must refuse the branch for the protected edit in the earlier commit, got {reason:?}"
        );

        std::fs::remove_dir_all(&temp).ok();
    }

    // The happy path the base-branch bug masked: base_branch is the ordinary
    // trunk `main` (no forge/ prefix), the work branch is a clean forge/
    // branch, and the firewall clears it. Before is_safe_base_branch existed,
    // git_diff_against_base rejected `main` and the firewall denied every
    // normal delivery with base_unresolvable.
    #[test]
    fn firewall_clears_a_clean_branch_against_a_main_base() {
        use std::process::Command;

        let temp = std::env::temp_dir().join(format!(
            "mdx_self_delivery_firewall_main_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&temp).expect("create temp repo dir");

        let git = |args: &[&str]| {
            let output = Command::new("git")
                .arg("-C")
                .arg(&temp)
                .args(args)
                .output()
                .expect("run git");
            assert!(
                output.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
            output
        };

        git(&["init", "-q"]);
        git(&["config", "user.email", "forge@example.com"]);
        git(&["config", "user.name", "Forge Test"]);
        git(&["config", "commit.gpgsign", "false"]);

        // Ordinary trunk base branch named `main` with an initial commit.
        git(&["checkout", "-q", "-b", "main"]);
        std::fs::write(temp.join("README.md"), "start\n").expect("write readme");
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "initial commit"]);

        // Work branch off main that edits a clean file only.
        git(&["checkout", "-q", "-b", "forge/run-clean-test"]);
        std::fs::write(temp.join("README.md"), "start\nmore\n").expect("update readme");
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "edit clean file"]);

        let kernel = boot();
        {
            let mut guard = kernel.write().expect("kernel");
            guard
                .record_forge_run_event_with_evidence_fields(
                    mdx_core::ForgeRunEvent {
                        tenant_id: "local_tenant",
                        actor_id: "agent:forge",
                        run_id: "run_firewall_clean_main_base",
                        event: "run_started",
                        work_item_id: "",
                        detail: "branch=forge/run-clean-test",
                        turn: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &GovernedWriteIdentity::local_demo("agent:forge"),
                    &[("repo_root", temp.to_str().unwrap())],
                )
                .expect("run_started recorded");
        }

        let reason = diff_firewall_reason(&kernel, "run_firewall_clean_main_base", "main");
        assert_eq!(
            reason, None,
            "a clean forge/ branch against a main base must clear the firewall, got {reason:?}"
        );

        std::fs::remove_dir_all(&temp).ok();
    }
}
