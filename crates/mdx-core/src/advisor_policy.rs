// The governed Advisor policy gate (ADR 0490).
//
// The advisor is premium intelligence spend, so it is governed by policy, not a
// toggle. A consult is bounded by: a maximum number of advisor calls per run, a
// maximum premium cost in cents per mission, an allowed set of work classes, and
// retention eligibility (a tenant whose profile forbids non-ZDR providers never
// routes a consult to one). The gate is fail-closed: the default policy allows
// nothing, so with no premium slot configured and no policy opting in, every
// consult is skipped and recorded as skipped, never silently run.
//
// This module is pure: no network, no storage, no clock. It decides; the server
// seam acts on the decision and records the outcome as an advisor_call receipt.

/// The bounds an operator sets on advisor consults. The default is closed:
/// nothing is allowed until a policy opts in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdvisorPolicy {
    /// Maximum advisor consults allowed within a single run. 0 disables the rail.
    pub max_calls_per_run: u32,
    /// Maximum premium spend, in cents, across a mission.
    pub max_premium_cents_per_mission: u64,
    /// The work classes a consult is allowed for. Empty allows none.
    pub allowed_work_classes: Vec<String>,
    /// When true, only zero-data-retention-eligible advisor providers may be
    /// consulted; a non-ZDR advisor is skipped (or must fall back to a compliant
    /// model at the seam).
    pub require_zdr: bool,
}

impl AdvisorPolicy {
    /// The fail-closed default: no calls, no budget, no work classes, ZDR
    /// required. Every consult is skipped until an operator opts in.
    pub fn closed() -> Self {
        Self {
            max_calls_per_run: 0,
            max_premium_cents_per_mission: 0,
            allowed_work_classes: Vec::new(),
            require_zdr: true,
        }
    }
}

impl Default for AdvisorPolicy {
    fn default() -> Self {
        Self::closed()
    }
}

/// What is true at the moment a consult is considered. All counters are the
/// spend BEFORE this consult, so the gate refuses when a limit is already
/// reached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdvisorConsultContext<'a> {
    pub work_class: &'a str,
    /// Advisor consults already made in this run.
    pub calls_made_this_run: u32,
    /// Premium cents already spent in this mission.
    pub premium_cents_spent_this_mission: u64,
    /// Whether an advisor model is resolvable at all (assigned or fallback).
    pub advisor_assigned: bool,
    /// Whether the resolved advisor provider is zero-data-retention eligible.
    pub advisor_provider_zdr_eligible: bool,
}

/// The gate decision. A skip carries a stable reason CODE so the recorded
/// advisor_call receipt names why the consult did not run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdvisorGateDecision {
    Allow,
    Skip { reason: &'static str },
}

impl AdvisorGateDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, AdvisorGateDecision::Allow)
    }

    /// The skip reason CODE, or empty when allowed.
    pub fn reason(&self) -> &'static str {
        match self {
            AdvisorGateDecision::Allow => "",
            AdvisorGateDecision::Skip { reason } => reason,
        }
    }
}

/// Decide whether an advisor consult may run, fail-closed. The checks are
/// ordered so the most fundamental refusal wins: no advisor, then the rail
/// disabled, then per-run and per-mission limits, then work class, then
/// retention eligibility.
pub fn decide_advisor_consult(
    policy: &AdvisorPolicy,
    context: &AdvisorConsultContext<'_>,
) -> AdvisorGateDecision {
    if !context.advisor_assigned {
        return AdvisorGateDecision::Skip {
            reason: "no_advisor_configured",
        };
    }
    if policy.max_calls_per_run == 0 {
        return AdvisorGateDecision::Skip {
            reason: "advisor_disabled_by_policy",
        };
    }
    if context.calls_made_this_run >= policy.max_calls_per_run {
        return AdvisorGateDecision::Skip {
            reason: "max_advisor_calls_per_run_reached",
        };
    }
    if policy.max_premium_cents_per_mission == 0
        || context.premium_cents_spent_this_mission >= policy.max_premium_cents_per_mission
    {
        return AdvisorGateDecision::Skip {
            reason: "premium_budget_exhausted",
        };
    }
    let work_class = context.work_class.trim();
    if work_class.is_empty()
        || !policy
            .allowed_work_classes
            .iter()
            .any(|allowed| allowed == work_class)
    {
        return AdvisorGateDecision::Skip {
            reason: "work_class_not_allowed",
        };
    }
    if policy.require_zdr && !context.advisor_provider_zdr_eligible {
        return AdvisorGateDecision::Skip {
            reason: "retention_ineligible_provider",
        };
    }
    AdvisorGateDecision::Allow
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_policy() -> AdvisorPolicy {
        AdvisorPolicy {
            max_calls_per_run: 2,
            max_premium_cents_per_mission: 500,
            allowed_work_classes: vec!["wide_plan_review".to_string()],
            require_zdr: false,
        }
    }

    fn eligible_context() -> AdvisorConsultContext<'static> {
        AdvisorConsultContext {
            work_class: "wide_plan_review",
            calls_made_this_run: 0,
            premium_cents_spent_this_mission: 0,
            advisor_assigned: true,
            advisor_provider_zdr_eligible: true,
        }
    }

    #[test]
    fn default_policy_is_fail_closed() {
        let decision = decide_advisor_consult(&AdvisorPolicy::closed(), &eligible_context());
        assert_eq!(decision.reason(), "advisor_disabled_by_policy");
        assert!(!decision.is_allowed());
    }

    #[test]
    fn an_opted_in_eligible_consult_is_allowed() {
        assert!(decide_advisor_consult(&open_policy(), &eligible_context()).is_allowed());
    }

    #[test]
    fn no_advisor_configured_skips_first() {
        let mut context = eligible_context();
        context.advisor_assigned = false;
        assert_eq!(
            decide_advisor_consult(&open_policy(), &context).reason(),
            "no_advisor_configured"
        );
    }

    #[test]
    fn per_run_limit_is_enforced() {
        let mut context = eligible_context();
        context.calls_made_this_run = 2;
        assert_eq!(
            decide_advisor_consult(&open_policy(), &context).reason(),
            "max_advisor_calls_per_run_reached"
        );
    }

    #[test]
    fn mission_budget_is_enforced() {
        let mut context = eligible_context();
        context.premium_cents_spent_this_mission = 500;
        assert_eq!(
            decide_advisor_consult(&open_policy(), &context).reason(),
            "premium_budget_exhausted"
        );
    }

    #[test]
    fn work_class_must_be_allowed() {
        let mut context = eligible_context();
        context.work_class = "routine_build";
        assert_eq!(
            decide_advisor_consult(&open_policy(), &context).reason(),
            "work_class_not_allowed"
        );
    }

    #[test]
    fn non_zdr_provider_is_refused_when_policy_requires_zdr() {
        let mut policy = open_policy();
        policy.require_zdr = true;
        let mut context = eligible_context();
        context.advisor_provider_zdr_eligible = false;
        assert_eq!(
            decide_advisor_consult(&policy, &context).reason(),
            "retention_ineligible_provider"
        );
    }
}
