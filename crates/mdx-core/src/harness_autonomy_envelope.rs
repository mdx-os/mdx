// Slice D: the autonomy policy envelope contract.
//
// Scaling the factory to many engineers means not every low-risk change can
// wait for a per-change human click. An autonomy policy envelope is how a named
// human OWNER pre-authorizes a BOUNDED class of work to complete without that
// per-change escalation: which work classes, up to which risk ceiling, inside
// which path scopes, with rollback and eval requirements, named escalation
// triggers, and an expiry. Everything outside the envelope escalates to a human.
//
// This module is the contract and the pure classifier only. It records nothing
// and it grants nothing on its own.
//
// One invariant remains absolute and encoded as const, not policy: an envelope
// NEVER grants deployment or production-write authority - self-delivery is repo
// self-improvement, never a production deploy (ADR 0139). SHIP ratification was
// also a const refusal until ADR 0488: an envelope MAY now grant the ship edge
// through an explicit, owner-recorded, revocable self-delivery scope, guarded by
// the self-modification firewall (a self-delivery run can never edit the code
// that limits it). Absent such a scope, the default envelope still grants
// nothing and the autonomous path stops at the human ship door.

/// How far an envelope lets a run carry its OWN delivery, replacing the
/// per-change human ship decision with a delegated, revocable grant (ADR 0488).
/// Ordered and monotonic: each tier includes the ones before it. `None` is the
/// default and the historical behavior - the autonomous path stops at the human
/// ship door. This ladder governs SHIP (repo self-delivery) only; it never
/// touches deployment or production-write authority, which stay const-refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum SelfDeliveryScope {
    /// No self-delivery. The run stops at READY_FOR_HUMAN_RATIFICATION.
    #[default]
    None,
    /// The run may open its own pull request instead of waiting for a human
    /// ship decision.
    SelfOpenPr,
    /// The run may additionally drive CI to green through bounded governed fix
    /// iterations after its PR is open.
    SelfConvergeCi,
    /// The run may additionally merge its own PR into the repository's default
    /// branch on green CI plus fresh-eyes concurrence. Repo self-improvement -
    /// never a production deploy.
    SelfMerge,
}

impl SelfDeliveryScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::SelfOpenPr => "self_open_pr",
            Self::SelfConvergeCi => "self_converge_ci",
            Self::SelfMerge => "self_merge",
        }
    }

    /// Parse a recorded scope string back into a scope. Unknown or empty values
    /// fall closed to `None` - an unrecognized grant authorizes nothing.
    pub fn from_wire(value: &str) -> Self {
        match value.trim() {
            "self_open_pr" => Self::SelfOpenPr,
            "self_converge_ci" => Self::SelfConvergeCi,
            "self_merge" => Self::SelfMerge,
            _ => Self::None,
        }
    }

    /// True when this scope authorizes the run to open its own PR (the floor of
    /// self-delivery). Every stronger tier is also a self-delivery tier.
    pub fn is_self_delivery(self) -> bool {
        self >= Self::SelfOpenPr
    }
}

/// The paths a self-delivery run may NEVER touch, whatever its envelope's
/// `allowed_path_scopes` says. This is the self-modification firewall: Forge
/// cannot edit the code that limits Forge. The list is const so no envelope
/// field can widen it, and it deliberately covers the governance kernel, the
/// authority and admission code, the envelope machinery (including this file),
/// the ship and merge gates, the finish evaluator, the CI evidence adapter,
/// the request-security and auth verifier, the CI configuration, and the ADRs.
/// A change touching any of these escalates to a human regardless of scope.
pub const PROTECTED_PATHS: &[&str] = &[
    "crates/mdx-core/src/lib.rs",
    "crates/mdx-core/src/actor_admission.rs",
    "crates/mdx-core/src/harness_autonomy_envelope.rs",
    "crates/mdx-core/src/harness_envelope_registry.rs",
    "crates/mdx-core/src/forge_run_ship.rs",
    "crates/mdx-server/src/forge_run_ship_route.rs",
    "crates/mdx-server/src/forge_source_host_live_delivery_route.rs",
    "crates/mdx-server/src/forge_finish_evaluator.rs",
    "crates/mdx-server/src/harness_ci_evidence_adapter.rs",
    "crates/mdx-server/src/request_security.rs",
    "crates/mdx-server/src/auth_verifier.rs",
    ".github/",
    "docs/adr/",
];

/// True when a touched path is inside the self-modification firewall. Robust by
/// construction and fail-closed: the path is normalized (backslashes to
/// forward slashes, leading `./` and `/` stripped), and ANY remaining `..`
/// segment - a traversal attempt - is treated as protected. A protected entry
/// ending in `/` is a directory prefix (matches everything under it); an entry
/// without a trailing slash matches that exact file or anything under it as a
/// directory, but never a mere string-prefix sibling (`lib.rs` does not shield,
/// nor is shielded by, `lib.rs.bak`).
pub fn path_is_protected(touched: &str) -> bool {
    let normalized = normalize_repo_path(touched);
    if normalized.is_empty() {
        // An unresolvable or empty path is refused, not waved through.
        return true;
    }
    if normalized.split('/').any(|segment| segment == "..") {
        // Traversal never reaches a decision; it is protected by default.
        return true;
    }
    PROTECTED_PATHS.iter().any(|protected| {
        let protected = *protected;
        if let Some(dir) = protected.strip_suffix('/') {
            normalized == dir || normalized.starts_with(&format!("{dir}/"))
        } else {
            normalized == protected || normalized.starts_with(&format!("{protected}/"))
        }
    })
}

fn normalize_repo_path(path: &str) -> String {
    let mut normalized = path.trim().replace('\\', "/");
    while let Some(rest) = normalized.strip_prefix("./") {
        normalized = rest.to_string();
    }
    normalized = normalized.trim_start_matches('/').to_string();
    normalized
}

/// The risk ceiling an envelope authorizes, and the risk a unit of work carries.
/// Ordered: Low < Medium < High.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AutonomyRiskClass {
    Low,
    Medium,
    High,
}

impl AutonomyRiskClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// A human-owned, time-bounded pre-authorization for a class of autonomous work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutonomyPolicyEnvelope<'a> {
    pub envelope_id: &'a str,
    /// The accountable human. An envelope without a real owner is not valid.
    pub owner_id: &'a str,
    /// The work classes this envelope pre-authorizes (e.g. "dependency_bump").
    pub preapproved_work_classes: &'a [&'a str],
    /// The highest risk class the envelope authorizes to complete autonomously.
    pub max_risk_class: AutonomyRiskClass,
    /// Path prefixes the work must stay within.
    pub allowed_path_scopes: &'a [&'a str],
    pub rollback_required: bool,
    pub eval_required: bool,
    /// Named conditions that, if observed in the work, force a human decision
    /// even when everything else is in-envelope.
    pub escalation_triggers: &'a [&'a str],
    /// ISO-8601 expiry. Work observed at or after this instant is out of envelope.
    pub expires_at: &'a str,
    /// A disabled envelope authorizes nothing.
    pub active: bool,
    /// How far this envelope lets a run carry its own delivery (ADR 0488).
    /// `None` (the default) keeps the historical behavior: the run stops at the
    /// human ship door. Any stronger scope is an explicit, owner-recorded grant.
    pub self_delivery_scope: SelfDeliveryScope,
}

impl AutonomyPolicyEnvelope<'_> {
    /// SHIP ratification. Historically a const refusal; now an envelope MAY grant
    /// the ship edge when its owner recorded a self-delivery scope (ADR 0488).
    /// Still gated: only a scope of SelfOpenPr or stronger grants it, so the
    /// default envelope grants nothing. This governs repo self-delivery only.
    pub fn grants_ship_ratification(&self) -> bool {
        self.active && self.self_delivery_scope.is_self_delivery()
    }

    /// The self-delivery scope this envelope grants, or None when inactive.
    /// Callers use this to know how far a run may carry its own delivery.
    pub fn effective_self_delivery_scope(&self) -> SelfDeliveryScope {
        if self.active {
            self.self_delivery_scope
        } else {
            SelfDeliveryScope::None
        }
    }

    // DEPLOYMENT and production-write authority remain absolute const refusals.
    // No envelope field can grant them: self-delivery is repo self-improvement,
    // never a production deploy (ADR 0139 is unchanged). These being const is
    // the compile-time proof that Tier 3 self-merge cannot become self-deploy.
    pub const fn grants_deployment_authority(&self) -> bool {
        false
    }
    pub const fn grants_production_write(&self) -> bool {
        false
    }

    fn is_valid(&self) -> Option<&'static str> {
        if self.envelope_id.trim().is_empty() {
            return Some("missing_envelope_id");
        }
        if self.owner_id.trim().is_empty() {
            return Some("missing_envelope_owner");
        }
        if !self.active {
            return Some("envelope_inactive");
        }
        if self.expires_at.trim().is_empty() {
            return Some("missing_envelope_expiry");
        }
        None
    }
}

/// One unit of reviewed, autonomous work being classified against an envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutonomyWorkClassification<'a> {
    pub work_class: &'a str,
    pub risk_class: AutonomyRiskClass,
    /// The path scopes the change actually touched.
    pub touched_path_scopes: &'a [&'a str],
    pub rollback_ready: bool,
    pub eval_passed: bool,
    /// Escalation conditions observed during this work (must be empty to stay
    /// in-envelope).
    pub triggered_conditions: &'a [&'a str],
    /// ISO-8601 instant the work was observed, compared to the envelope expiry.
    pub observed_at: &'a str,
}

/// The disposition of a unit of work against an envelope. Either the envelope's
/// standing authorization covers completing it, or it escalates to a human with
/// a reason. There is no third option that ships or deploys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AutonomyDisposition {
    /// The envelope authorizes COMPLETING this bounded work without a per-change
    /// human click. It does NOT authorize ship ratification or deployment.
    EnvelopeAuthorizedCompletion,
    /// The work falls outside the envelope; a human must decide. Carries why.
    EscalateToHuman(String),
}

impl AutonomyDisposition {
    pub fn is_authorized(&self) -> bool {
        matches!(self, Self::EnvelopeAuthorizedCompletion)
    }
    pub fn escalation_reason(&self) -> Option<&str> {
        match self {
            Self::EscalateToHuman(reason) => Some(reason),
            Self::EnvelopeAuthorizedCompletion => None,
        }
    }
}

/// The pure classifier. Fail-closed: any condition not met escalates to a human.
/// An expired, inactive, out-of-class, over-risk, out-of-scope, rollback-short,
/// eval-short, or trigger-hit unit of work escalates. Only fully in-envelope
/// work returns EnvelopeAuthorizedCompletion - and even that never ships.
pub fn classify_autonomy(
    envelope: &AutonomyPolicyEnvelope<'_>,
    work: &AutonomyWorkClassification<'_>,
) -> AutonomyDisposition {
    if let Some(reason) = envelope.is_valid() {
        return AutonomyDisposition::EscalateToHuman(reason.to_string());
    }
    // Expiry: work observed at or after the envelope's expiry is out of envelope.
    // String compare is correct for zero-padded ISO-8601 in UTC (Z).
    if work.observed_at.trim() >= envelope.expires_at.trim() {
        return AutonomyDisposition::EscalateToHuman("envelope_expired".to_string());
    }
    if !contains(envelope.preapproved_work_classes, work.work_class.trim()) {
        return AutonomyDisposition::EscalateToHuman(format!(
            "work_class_not_preapproved:{}",
            work.work_class.trim()
        ));
    }
    if work.risk_class > envelope.max_risk_class {
        return AutonomyDisposition::EscalateToHuman(format!(
            "risk_exceeds_envelope:{}>{}",
            work.risk_class.as_str(),
            envelope.max_risk_class.as_str()
        ));
    }
    // The self-modification firewall runs FIRST when this envelope grants any
    // self-delivery: a run that can carry its own delivery must never edit the
    // code that limits it, even if allowed_path_scopes would permit it. This is
    // checked before the ordinary scope check so a protected path always
    // escalates with the firewall reason, never a generic out-of-scope one.
    if envelope.effective_self_delivery_scope().is_self_delivery() {
        for touched in work.touched_path_scopes {
            if path_is_protected(touched) {
                return AutonomyDisposition::EscalateToHuman(format!(
                    "self_modification_firewall:{}",
                    touched.trim()
                ));
            }
        }
    }
    for touched in work.touched_path_scopes {
        if !path_in_scope(envelope.allowed_path_scopes, touched.trim()) {
            return AutonomyDisposition::EscalateToHuman(format!(
                "path_out_of_scope:{}",
                touched.trim()
            ));
        }
    }
    if envelope.rollback_required && !work.rollback_ready {
        return AutonomyDisposition::EscalateToHuman("rollback_not_ready".to_string());
    }
    if envelope.eval_required && !work.eval_passed {
        return AutonomyDisposition::EscalateToHuman("eval_not_passed".to_string());
    }
    // Any observed escalation trigger that the envelope names forces a human.
    for condition in work.triggered_conditions {
        let condition = condition.trim();
        if contains(envelope.escalation_triggers, condition) {
            return AutonomyDisposition::EscalateToHuman(format!(
                "escalation_triggered:{condition}"
            ));
        }
    }
    AutonomyDisposition::EnvelopeAuthorizedCompletion
}

fn contains(haystack: &[&str], needle: &str) -> bool {
    haystack.iter().any(|item| item.trim() == needle)
}

// A touched path is in scope when it sits under one of the allowed prefixes.
// An empty scope list authorizes nothing.
fn path_in_scope(allowed: &[&str], touched: &str) -> bool {
    if touched.is_empty() {
        return false;
    }
    allowed
        .iter()
        .any(|prefix| !prefix.trim().is_empty() && touched.starts_with(prefix.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope() -> AutonomyPolicyEnvelope<'static> {
        AutonomyPolicyEnvelope {
            envelope_id: "env_low_risk_deps",
            owner_id: "human_platform_lead",
            preapproved_work_classes: &["dependency_bump", "doc_update"],
            max_risk_class: AutonomyRiskClass::Low,
            allowed_path_scopes: &["crates/", "docs/"],
            rollback_required: true,
            eval_required: true,
            escalation_triggers: &["security_advisory", "public_api_change"],
            expires_at: "2026-12-31T00:00:00Z",
            active: true,
            self_delivery_scope: SelfDeliveryScope::None,
        }
    }

    fn in_envelope_work() -> AutonomyWorkClassification<'static> {
        AutonomyWorkClassification {
            work_class: "dependency_bump",
            risk_class: AutonomyRiskClass::Low,
            touched_path_scopes: &["crates/mdx-core/Cargo.toml"],
            rollback_ready: true,
            eval_passed: true,
            triggered_conditions: &[],
            observed_at: "2026-06-07T12:00:00Z",
        }
    }

    fn self_merge_envelope() -> AutonomyPolicyEnvelope<'static> {
        let mut env = envelope();
        env.self_delivery_scope = SelfDeliveryScope::SelfMerge;
        env
    }

    fn work_touching<'a>(paths: &'a [&'a str]) -> AutonomyWorkClassification<'a> {
        AutonomyWorkClassification {
            work_class: "dependency_bump",
            risk_class: AutonomyRiskClass::Low,
            touched_path_scopes: paths,
            rollback_ready: true,
            eval_passed: true,
            triggered_conditions: &[],
            observed_at: "2026-06-07T12:00:00Z",
        }
    }

    #[test]
    fn self_delivery_scope_ladder_is_monotonic() {
        assert!(SelfDeliveryScope::None < SelfDeliveryScope::SelfOpenPr);
        assert!(SelfDeliveryScope::SelfOpenPr < SelfDeliveryScope::SelfConvergeCi);
        assert!(SelfDeliveryScope::SelfConvergeCi < SelfDeliveryScope::SelfMerge);
        assert!(!SelfDeliveryScope::None.is_self_delivery());
        assert!(SelfDeliveryScope::SelfOpenPr.is_self_delivery());
        assert!(SelfDeliveryScope::SelfMerge.is_self_delivery());
        // Round-trips through its wire string.
        for scope in [
            SelfDeliveryScope::None,
            SelfDeliveryScope::SelfOpenPr,
            SelfDeliveryScope::SelfConvergeCi,
            SelfDeliveryScope::SelfMerge,
        ] {
            assert_eq!(SelfDeliveryScope::from_wire(scope.as_str()), scope);
        }
        assert_eq!(
            SelfDeliveryScope::from_wire("garbage"),
            SelfDeliveryScope::None
        );
    }

    #[test]
    fn ship_grant_follows_the_scope_but_deploy_never_does() {
        // Default envelope grants nothing - today's behavior.
        assert!(!envelope().grants_ship_ratification());
        // A self-delivery scope grants the ship edge...
        for scope in [
            SelfDeliveryScope::SelfOpenPr,
            SelfDeliveryScope::SelfConvergeCi,
            SelfDeliveryScope::SelfMerge,
        ] {
            let mut env = envelope();
            env.self_delivery_scope = scope;
            assert!(env.grants_ship_ratification(), "{}", scope.as_str());
            // ...but deployment and production-write are refused for EVERY scope.
            assert!(!env.grants_deployment_authority(), "{}", scope.as_str());
            assert!(!env.grants_production_write(), "{}", scope.as_str());
        }
        // An inactive envelope grants nothing even with a scope set.
        let mut inactive = self_merge_envelope();
        inactive.active = false;
        assert!(!inactive.grants_ship_ratification());
        assert_eq!(
            inactive.effective_self_delivery_scope(),
            SelfDeliveryScope::None
        );
    }

    #[test]
    fn firewall_blocks_every_protected_path_under_any_allowed_scope() {
        // Even an envelope whose allowed_path_scopes permits all of crates/ and
        // docs/ cannot let a self-delivery run touch a protected path.
        let mut env = self_merge_envelope();
        env.allowed_path_scopes = &["crates/", "docs/", ".github/"];
        for protected in [
            "crates/mdx-core/src/lib.rs",
            "crates/mdx-core/src/actor_admission.rs",
            "crates/mdx-core/src/harness_autonomy_envelope.rs",
            "crates/mdx-core/src/harness_envelope_registry.rs",
            "crates/mdx-server/src/forge_run_ship_route.rs",
            "crates/mdx-server/src/forge_finish_evaluator.rs",
            "crates/mdx-server/src/request_security.rs",
            ".github/workflows/ci.yml",
            "docs/adr/0488-governed-autonomous-delivery-envelope.md",
        ] {
            let disposition = classify_autonomy(&env, &work_touching(&[protected]));
            assert_eq!(
                disposition.escalation_reason(),
                Some(format!("self_modification_firewall:{protected}").as_str()),
                "protected path {protected} must escalate through the firewall"
            );
        }
    }

    #[test]
    fn firewall_resists_traversal_and_prefix_evasion() {
        assert!(path_is_protected("crates/mdx-core/src/lib.rs"));
        assert!(path_is_protected("./crates/mdx-core/src/lib.rs"));
        assert!(path_is_protected("/crates/mdx-core/src/lib.rs"));
        assert!(path_is_protected("crates\\mdx-core\\src\\lib.rs"));
        // Any traversal is protected by default (fail-closed).
        assert!(path_is_protected("crates/mdx-core/src/../../../etc/passwd"));
        assert!(path_is_protected(""));
        // Directory prefixes shield everything under them.
        assert!(path_is_protected(".github/workflows/verify.yml"));
        assert!(path_is_protected("docs/adr/0001-anything.md"));
        // A sibling that merely shares a string prefix is NOT shielded.
        assert!(!path_is_protected("crates/mdx-core/src/lib.rs.bak"));
        assert!(!path_is_protected("crates/mdx-core/src/lib_helpers.rs"));
        assert!(!path_is_protected("docs/adr-notes.md"));
        // Ordinary work paths are free.
        assert!(!path_is_protected("crates/mdx-core/src/forge_run.rs"));
        assert!(!path_is_protected("crates/mdx-core/Cargo.toml"));
    }

    #[test]
    fn firewall_is_inert_without_a_self_delivery_scope() {
        // A None-scope envelope is NOT a self-delivery run, so the firewall does
        // not apply - a protected path just follows the ordinary scope rules
        // (here allowed, since crates/ is in scope). The firewall only guards
        // runs that can actually carry their own delivery.
        let env = envelope();
        assert_eq!(env.effective_self_delivery_scope(), SelfDeliveryScope::None);
        let disposition = classify_autonomy(&env, &work_touching(&["crates/mdx-core/src/lib.rs"]));
        assert_eq!(
            disposition,
            AutonomyDisposition::EnvelopeAuthorizedCompletion
        );
    }

    #[test]
    fn self_delivery_run_completes_ordinary_work_and_still_never_deploys() {
        let env = self_merge_envelope();
        let disposition = classify_autonomy(&env, &in_envelope_work());
        assert_eq!(
            disposition,
            AutonomyDisposition::EnvelopeAuthorizedCompletion
        );
        assert!(env.grants_ship_ratification());
        assert!(!env.grants_deployment_authority());
    }

    #[test]
    fn fully_in_envelope_work_is_authorized_to_complete_but_never_ships() {
        let env = envelope();
        let disposition = classify_autonomy(&env, &in_envelope_work());
        assert_eq!(
            disposition,
            AutonomyDisposition::EnvelopeAuthorizedCompletion
        );
        assert!(disposition.is_authorized());
        // The absolute invariants hold no matter the envelope.
        assert!(!env.grants_ship_ratification());
        assert!(!env.grants_deployment_authority());
        assert!(!env.grants_production_write());
    }

    #[test]
    fn an_inactive_or_expired_envelope_escalates() {
        let mut env = envelope();
        env.active = false;
        assert_eq!(
            classify_autonomy(&env, &in_envelope_work()).escalation_reason(),
            Some("envelope_inactive")
        );

        let env = envelope();
        let mut work = in_envelope_work();
        work.observed_at = "2027-01-01T00:00:00Z";
        assert_eq!(
            classify_autonomy(&env, &work).escalation_reason(),
            Some("envelope_expired")
        );
    }

    #[test]
    fn out_of_class_over_risk_or_out_of_scope_work_escalates() {
        let env = envelope();

        let mut wrong_class = in_envelope_work();
        wrong_class.work_class = "schema_migration";
        assert_eq!(
            classify_autonomy(&env, &wrong_class).escalation_reason(),
            Some("work_class_not_preapproved:schema_migration")
        );

        let mut high_risk = in_envelope_work();
        high_risk.risk_class = AutonomyRiskClass::High;
        assert_eq!(
            classify_autonomy(&env, &high_risk).escalation_reason(),
            Some("risk_exceeds_envelope:high>low")
        );

        let mut out_of_scope = in_envelope_work();
        out_of_scope.touched_path_scopes = &["infra/terraform/main.tf"];
        assert_eq!(
            classify_autonomy(&env, &out_of_scope).escalation_reason(),
            Some("path_out_of_scope:infra/terraform/main.tf")
        );
    }

    #[test]
    fn missing_rollback_eval_or_a_trigger_escalates() {
        let env = envelope();

        let mut no_rollback = in_envelope_work();
        no_rollback.rollback_ready = false;
        assert_eq!(
            classify_autonomy(&env, &no_rollback).escalation_reason(),
            Some("rollback_not_ready")
        );

        let mut no_eval = in_envelope_work();
        no_eval.eval_passed = false;
        assert_eq!(
            classify_autonomy(&env, &no_eval).escalation_reason(),
            Some("eval_not_passed")
        );

        let mut triggered = in_envelope_work();
        triggered.triggered_conditions = &["security_advisory"];
        assert_eq!(
            classify_autonomy(&env, &triggered).escalation_reason(),
            Some("escalation_triggered:security_advisory")
        );
    }

    #[test]
    fn an_envelope_with_no_owner_or_no_scope_authorizes_nothing() {
        let mut no_owner = envelope();
        no_owner.owner_id = "  ";
        assert_eq!(
            classify_autonomy(&no_owner, &in_envelope_work()).escalation_reason(),
            Some("missing_envelope_owner")
        );

        let mut no_scope = envelope();
        no_scope.allowed_path_scopes = &[];
        assert_eq!(
            classify_autonomy(&no_scope, &in_envelope_work()).escalation_reason(),
            Some("path_out_of_scope:crates/mdx-core/Cargo.toml")
        );
    }
}
