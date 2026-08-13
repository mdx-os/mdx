// The governed Advisor consult receipt contract (ADR 0490).
//
// The advisor is the premium judgment role: a strong model consulted rarely, at
// high-leverage moments (plan review before a wide run, stuck-state recovery,
// mission shaping), that never executes. Every consult must leave a receipt so
// premium intelligence spend is a first-class, auditable line in the ledger
// rather than an invisible cost.
//
// This module is pure: no network, no storage, no secret access, no clock. It
// builds and validates the `advisor.call.recorded` receipt payload the server
// appends at each consult seam. Two invariants are structural here, not left to
// the caller:
//   - the advisor NEVER holds execution authority
//     (`advisor_grants_execution_authority = false` is stamped on every
//     payload, mirroring work classification); and
//   - the payload records a DIGEST of the advice, never the advice text, so a
//     receipt cannot leak model output. The digest is bounded and whitespace
//     free so a raw answer cannot be smuggled through it.
//
// The payload field set, the trigger vocabulary, and the outcome vocabulary are
// locked to `contracts/advisor-call.schema.json` by `make advisor-call-contract
// -check` (xtask), so the schema and this code cannot drift.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, MdxKernel, StorageProvider};
use std::collections::BTreeMap;

/// The receipt kind every advisor consult appends.
pub const ADVISOR_CALL_RECEIPT_KIND: &str = "advisor.call.recorded";

/// The advisor is a judgment role, never an executor. This is stamped on every
/// receipt so the invariant is auditable, mirroring work classification.
pub const ADVISOR_GRANTS_EXECUTION_AUTHORITY: bool = false;

/// The high-leverage moments an advisor may be consulted at. A consult naming
/// any other trigger is refused so the rail stays for judgment, not routine
/// work.
pub const ADVISOR_TRIGGERS: &[&str] = &[
    "plan_review",     // before a wide fleet launch (the wide-plan-review seam)
    "mission_shaping", // shaping a mission at admission
    "architecture",    // architecture decomposition
    "stuck_recovery",  // stuck or repeated-failure recovery
    "sizing_judgment", // "too big / too small / misrouted" on an ask
    "operator_requested",
];

/// What became of a consult. Every consult resolves to exactly one outcome;
/// nothing is silent.
pub const ADVISOR_OUTCOMES: &[&str] = &[
    "advised",  // the advisor returned advice
    "fallback", // the assigned advisor was unavailable/ineligible; a compliant model answered
    "refused",  // the model refused (e.g. a provider classifier refusal)
    "skipped",  // policy or eligibility skipped the consult (recorded, not hidden)
];

/// The payload keys every advisor-call receipt carries. Kept in lockstep with
/// `contracts/advisor-call.schema.json`.
pub const ADVISOR_CALL_PAYLOAD_FIELDS: &[&str] = &[
    "trigger",
    "advisor_model_id",
    "provider_family",
    "input_tokens",
    "output_tokens",
    "cost_cents",
    "advice_digest",
    "input_scope_hash",
    "outcome",
    "fallback_model_id",
    "refusal_reason",
    "advisor_grants_execution_authority",
];

/// A digest must stay a digest: bounded and whitespace-free, so advice text
/// cannot be smuggled into the ledger through it.
pub const ADVISOR_MAX_DIGEST_LEN: usize = 64;

/// A single advisor consult, ready to be recorded. All content is safe-context:
/// a digest of the advice, token and cost counts, and the scope hash of the
/// input, never the prompt or the advice itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdvisorCall {
    pub trigger: String,
    pub advisor_model_id: String,
    pub provider_family: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_cents: u64,
    /// A hash/digest of the advice, NEVER the advice text.
    pub advice_digest: String,
    /// A hash of the input scope (what the advisor was shown), NEVER the input.
    pub input_scope_hash: String,
    pub outcome: String,
    /// Set only when `outcome == "fallback"`: the compliant model that answered.
    pub fallback_model_id: String,
    /// Set only when `outcome == "refused"`: the refusal reason CODE, not text.
    pub refusal_reason: String,
}

/// Why an advisor consult could not be recorded. Every refusal is explicit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdvisorCallError {
    Missing(&'static str),
    UnknownTrigger(String),
    UnknownOutcome(String),
    /// A digest exceeded `ADVISOR_MAX_DIGEST_LEN` or contained whitespace, so it could be
    /// carrying advice text rather than a digest.
    DigestNotDigestLike {
        field: &'static str,
        reason: &'static str,
    },
    /// The outcome required a field that was empty (fallback needs a model,
    /// refusal needs a reason).
    OutcomeFieldMissing {
        outcome: String,
        field: &'static str,
    },
}

fn digest_like(field: &'static str, value: &str) -> Result<(), AdvisorCallError> {
    if value.len() > ADVISOR_MAX_DIGEST_LEN {
        return Err(AdvisorCallError::DigestNotDigestLike {
            field,
            reason: "too_long",
        });
    }
    if value.chars().any(|c| c.is_whitespace()) {
        return Err(AdvisorCallError::DigestNotDigestLike {
            field,
            reason: "contains_whitespace",
        });
    }
    Ok(())
}

impl AdvisorCall {
    /// Validate the consult against the contract. Fails closed: an unknown
    /// trigger or outcome, a missing required value, or a digest that could be
    /// carrying advice text is refused, never recorded.
    pub fn validate(&self) -> Result<(), AdvisorCallError> {
        if self.trigger.trim().is_empty() {
            return Err(AdvisorCallError::Missing("trigger"));
        }
        if !ADVISOR_TRIGGERS.contains(&self.trigger.as_str()) {
            return Err(AdvisorCallError::UnknownTrigger(self.trigger.clone()));
        }
        if self.advisor_model_id.trim().is_empty() {
            return Err(AdvisorCallError::Missing("advisor_model_id"));
        }
        if self.provider_family.trim().is_empty() {
            return Err(AdvisorCallError::Missing("provider_family"));
        }
        if self.input_scope_hash.trim().is_empty() {
            return Err(AdvisorCallError::Missing("input_scope_hash"));
        }
        if !ADVISOR_OUTCOMES.contains(&self.outcome.as_str()) {
            return Err(AdvisorCallError::UnknownOutcome(self.outcome.clone()));
        }
        // Digests must stay digests whenever present.
        digest_like("advice_digest", &self.advice_digest)?;
        digest_like("input_scope_hash", &self.input_scope_hash)?;
        // When the advisor actually answered, an advice digest must be present.
        if matches!(self.outcome.as_str(), "advised" | "fallback")
            && self.advice_digest.trim().is_empty()
        {
            return Err(AdvisorCallError::OutcomeFieldMissing {
                outcome: self.outcome.clone(),
                field: "advice_digest",
            });
        }
        if self.outcome == "fallback" && self.fallback_model_id.trim().is_empty() {
            return Err(AdvisorCallError::OutcomeFieldMissing {
                outcome: self.outcome.clone(),
                field: "fallback_model_id",
            });
        }
        if self.outcome == "refused" && self.refusal_reason.trim().is_empty() {
            return Err(AdvisorCallError::OutcomeFieldMissing {
                outcome: self.outcome.clone(),
                field: "refusal_reason",
            });
        }
        Ok(())
    }

    /// Build the validated receipt payload map. Always stamps
    /// `advisor_grants_execution_authority=false`. Returns the same
    /// `AdvisorCallError` as [`validate`](Self::validate) on a contract breach so
    /// a bad consult fails closed instead of recording a partial receipt.
    pub fn to_payload(&self) -> Result<BTreeMap<String, String>, AdvisorCallError> {
        self.validate()?;
        let mut payload = BTreeMap::new();
        payload.insert("trigger".to_string(), self.trigger.clone());
        payload.insert(
            "advisor_model_id".to_string(),
            self.advisor_model_id.clone(),
        );
        payload.insert("provider_family".to_string(), self.provider_family.clone());
        payload.insert("input_tokens".to_string(), self.input_tokens.to_string());
        payload.insert("output_tokens".to_string(), self.output_tokens.to_string());
        payload.insert("cost_cents".to_string(), self.cost_cents.to_string());
        payload.insert("advice_digest".to_string(), self.advice_digest.clone());
        payload.insert(
            "input_scope_hash".to_string(),
            self.input_scope_hash.clone(),
        );
        payload.insert("outcome".to_string(), self.outcome.clone());
        payload.insert(
            "fallback_model_id".to_string(),
            self.fallback_model_id.clone(),
        );
        payload.insert("refusal_reason".to_string(), self.refusal_reason.clone());
        payload.insert(
            "advisor_grants_execution_authority".to_string(),
            ADVISOR_GRANTS_EXECUTION_AUTHORITY.to_string(),
        );
        Ok(payload)
    }
}

/// The result of recording an advisor consult.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdvisorCallReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub outcome: String,
}

/// A request to record one advisor consult in the ledger. The actor on the
/// receipt envelope is the accountable writer; the consult carries only
/// safe-context fields (see [`AdvisorCall`]).
pub struct RecordAdvisorCall<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub call: &'a AdvisorCall,
}

impl<S: StorageProvider> MdxKernel<S> {
    /// Record an advisor consult as an `advisor.call.recorded` receipt. Fails
    /// closed: a consult that breaches the payload contract is refused with the
    /// same [`AdvisorCallError`] as validation, so a malformed consult never
    /// lands a partial receipt. The advisor never holds execution authority; the
    /// payload stamps that invariant. A skipped consult is recorded too, so a
    /// policy refusal is auditable rather than invisible.
    pub fn record_advisor_call(
        &mut self,
        request: RecordAdvisorCall<'_>,
    ) -> Result<AdvisorCallReport, AdvisorCallError> {
        let payload = request.call.to_payload()?;
        let (receipt_id, policy_decision_id) = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "forge_advisor",
                action: ActionKind::ConsultAdvisor,
                transition: "CONSULT_ADVISOR",
                kind: ADVISOR_CALL_RECEIPT_KIND,
            },
            payload,
        );
        Ok(AdvisorCallReport {
            receipt_id,
            policy_decision_id,
            outcome: request.call.outcome.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advised() -> AdvisorCall {
        AdvisorCall {
            trigger: "plan_review".to_string(),
            advisor_model_id: "claude-opus-4-8".to_string(),
            provider_family: "anthropic".to_string(),
            input_tokens: 4200,
            output_tokens: 610,
            cost_cents: 37,
            advice_digest: "sha256:ab12cd34".to_string(),
            input_scope_hash: "sha256:ef56".to_string(),
            outcome: "advised".to_string(),
            fallback_model_id: String::new(),
            refusal_reason: String::new(),
        }
    }

    #[test]
    fn advised_consult_builds_payload_with_all_required_fields() {
        let payload = advised().to_payload().expect("valid advised consult");
        for field in ADVISOR_CALL_PAYLOAD_FIELDS {
            assert!(payload.contains_key(*field), "missing {field}");
        }
        assert_eq!(payload.len(), ADVISOR_CALL_PAYLOAD_FIELDS.len());
        assert_eq!(
            payload.get("advisor_grants_execution_authority").unwrap(),
            "false"
        );
    }

    #[test]
    fn authority_invariant_is_always_false() {
        // Bind through a value so this stays a runtime check on the emitted
        // payload rather than a constant assertion; the advisor never holds
        // execution authority.
        let authority = advised()
            .to_payload()
            .expect("valid consult")
            .get("advisor_grants_execution_authority")
            .cloned()
            .unwrap_or_default();
        assert_eq!(authority, "false");
    }

    #[test]
    fn unknown_trigger_is_refused() {
        let mut call = advised();
        call.trigger = "routine_build".to_string();
        assert!(matches!(
            call.validate(),
            Err(AdvisorCallError::UnknownTrigger(_))
        ));
    }

    #[test]
    fn unknown_outcome_is_refused() {
        let mut call = advised();
        call.outcome = "shipped".to_string();
        assert!(matches!(
            call.validate(),
            Err(AdvisorCallError::UnknownOutcome(_))
        ));
    }

    #[test]
    fn advice_text_cannot_be_smuggled_through_the_digest() {
        let mut call = advised();
        call.advice_digest =
            "refactor the router and split the plan into three streams".to_string();
        assert!(matches!(
            call.validate(),
            Err(AdvisorCallError::DigestNotDigestLike { .. })
        ));
    }

    #[test]
    fn fallback_requires_a_model_and_refusal_requires_a_reason() {
        let mut fallback = advised();
        fallback.outcome = "fallback".to_string();
        assert!(matches!(
            fallback.validate(),
            Err(AdvisorCallError::OutcomeFieldMissing { .. })
        ));
        fallback.fallback_model_id = "gpt-5".to_string();
        assert!(fallback.validate().is_ok());

        let mut refused = advised();
        refused.outcome = "refused".to_string();
        refused.advice_digest = String::new();
        assert!(matches!(
            refused.validate(),
            Err(AdvisorCallError::OutcomeFieldMissing { .. })
        ));
        refused.refusal_reason = "provider_classifier_refusal".to_string();
        assert!(refused.validate().is_ok());
    }

    #[test]
    fn skipped_consult_records_without_an_advice_digest() {
        let mut skipped = advised();
        skipped.outcome = "skipped".to_string();
        skipped.advice_digest = String::new();
        let payload = skipped.to_payload().expect("skipped consult records");
        assert_eq!(payload.get("outcome").unwrap(), "skipped");
        assert_eq!(payload.get("advice_digest").unwrap(), "");
    }

    #[test]
    fn record_advisor_call_lands_a_receipt_of_the_right_kind() {
        let mut kernel = MdxKernel::boot_local();
        let call = advised();
        let report = kernel
            .record_advisor_call(RecordAdvisorCall {
                tenant_id: "local_tenant",
                actor_id: "local_operator",
                call: &call,
            })
            .expect("advisor consult records");
        assert!(!report.receipt_id.is_empty());
        assert_eq!(report.outcome, "advised");

        let receipts = kernel.ledger().query().by_kind(ADVISOR_CALL_RECEIPT_KIND);
        assert_eq!(receipts.len(), 1);
        let payload = &receipts[0].payload;
        assert_eq!(payload.get("advisor_model_id").unwrap(), "claude-opus-4-8");
        assert_eq!(
            payload.get("advisor_grants_execution_authority").unwrap(),
            "false"
        );
    }

    #[test]
    fn record_advisor_call_refuses_a_contract_breach() {
        let mut kernel = MdxKernel::boot_local();
        let mut bad = advised();
        bad.trigger = "routine_build".to_string();
        assert!(
            kernel
                .record_advisor_call(RecordAdvisorCall {
                    tenant_id: "local_tenant",
                    actor_id: "local_operator",
                    call: &bad,
                })
                .is_err()
        );
        assert!(
            kernel
                .ledger()
                .query()
                .by_kind(ADVISOR_CALL_RECEIPT_KIND)
                .is_empty()
        );
    }
}
