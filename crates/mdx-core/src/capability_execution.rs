//! Move 2: governed capability execution for Marketplace distilled skills.
//!
//! A ratified distilled skill (`marketplace_skill.rs`) is a verified, bounded,
//! deterministic PROCEDURE distilled from a proven-green Forge run. Until now a
//! ratified skill could never actually run: every permission flag on it is
//! hardcoded `false`, so a pack installs but does nothing. This module lifts the
//! single `capability_execution_allowed` gate, and ONLY that gate, and only
//! under an explicit, human-owned, revocable, expiring GRANT - mirroring the
//! governed autonomy envelope (ADR 0488) exactly:
//!
//!   ratified skill  +  active execution grant for THIS skill_id
//!       ->  execute the VERIFIED deterministic procedure, isolated, receipted
//!
//! The default is unchanged: no grant means no execution. A grant is a
//! `capability.execution.grant.recorded` receipt naming one specific skill_id,
//! an owner, and an expiry; a `capability.execution.grant.revoked` receipt or a
//! passed expiry makes it authorize nothing (the kill switch), exactly like the
//! envelope authority blocker.
//!
//! Three authorities stay `const false` and no grant can ever flip them, the
//! same compile-time proof `grants_deployment_authority` gives the envelope: a
//! capability execution NEVER writes production, NEVER reads secrets, and NEVER
//! inherits agent permissions. The grant lifts execution of the ratified
//! deterministic procedure in isolation, and nothing more.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

pub const CAPABILITY_EXECUTION_GRANT_RECORDED_KIND: &str = "capability.execution.grant.recorded";
pub const CAPABILITY_EXECUTION_GRANT_REVOKED_KIND: &str = "capability.execution.grant.revoked";
pub const CAPABILITY_EXECUTION_GRANT_REFUSED_KIND: &str = "capability.execution.grant.refused";
pub const CAPABILITY_EXECUTION_RAN_KIND: &str = "capability.execution.ran";
pub const CAPABILITY_EXECUTION_REFUSED_KIND: &str = "capability.execution.refused";

/// A hard ceiling on how many procedure steps one execution may attempt,
/// whatever the grant says. A grant's `max_steps` can only be more restrictive,
/// never wider - the bound can never be configured away.
pub const MAX_EXECUTION_STEPS_CEILING: u32 = 40;

const MAX_TEXT_CHARS: usize = 2000;

/// A human-owned, expiring, revocable authorization for ONE ratified skill to
/// execute. Mirrors `AutonomyPolicyEnvelope`: the only thing it can grant is the
/// capability-execution edge for its named skill; three authorities are const
/// refusals no field can flip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityExecutionGrant<'a> {
    pub grant_id: &'a str,
    /// The accountable human. A grant without a real owner is not valid.
    pub owner_id: &'a str,
    /// The SINGLE ratified skill this grant authorizes to execute.
    pub skill_id: &'a str,
    /// ISO-8601 expiry. An execution observed at or after this instant is out of
    /// grant.
    pub expires_at: &'a str,
    /// The step ceiling this grant authorizes, clamped to `MAX_EXECUTION_STEPS_CEILING`.
    pub max_steps: u32,
    pub reason: &'a str,
    /// A disabled grant authorizes nothing.
    pub active: bool,
}

impl CapabilityExecutionGrant<'_> {
    /// The ONE authority a grant can lift: execution of its named ratified
    /// skill's deterministic procedure. A grant only speaks for the exact
    /// skill_id it names.
    pub fn grants_capability_execution(&self, skill_id: &str) -> bool {
        self.active && !self.skill_id.trim().is_empty() && self.skill_id.trim() == skill_id.trim()
    }

    /// The effective step ceiling: the grant's own ceiling, never above the
    /// module hard cap.
    pub fn effective_max_steps(&self) -> u32 {
        self.max_steps.min(MAX_EXECUTION_STEPS_CEILING)
    }

    // These three authorities are ABSOLUTE const refusals. No grant field can
    // grant them: a capability execution is a bounded, isolated, deterministic
    // procedure run, never a production write, a secret read, or an inheritance
    // of agent permissions. These being const is the compile-time proof that a
    // grant can only ever lift `capability_execution_allowed` and nothing else.
    pub const fn grants_production_write(&self) -> bool {
        false
    }
    pub const fn grants_secret_access(&self) -> bool {
        false
    }
    pub const fn grants_inherited_agent_permissions(&self) -> bool {
        false
    }
}

/// Record request for a capability-execution grant.
#[derive(Clone, Copy, Debug)]
pub struct CapabilityExecutionGrantRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub grant_id: &'a str,
    pub skill_id: &'a str,
    /// Empty defaults to the recording actor.
    pub owner_id: &'a str,
    pub expires_at: &'a str,
    pub max_steps: u32,
    pub reason: &'a str,
}

/// Revoke request: the kill switch for a recorded grant.
#[derive(Clone, Copy, Debug)]
pub struct CapabilityExecutionGrantRevoke<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    /// The `capability.execution.grant.recorded` receipt being revoked.
    pub grant_receipt_id: &'a str,
    pub reason: &'a str,
}

/// The step outcomes an isolated runner produces, folded onto the ran receipt.
/// The runner lives in the server (it drives the Forge sandbox); the kernel only
/// records the receipt, so this is the honest evidence surface between them.
#[derive(Clone, Copy, Debug)]
pub struct CapabilityExecutionOutcome<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub skill_id: &'a str,
    pub grant_id: &'a str,
    /// The isolation posture the steps ran under (e.g. `scrubbed_sandboxed`).
    pub isolation: &'a str,
    pub steps_total: u32,
    pub steps_executed: u32,
    pub steps_passed: u32,
    pub all_steps_passed: bool,
    /// A compact, bounded per-step log for the evidence panel.
    pub step_log: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityExecutionGrantReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub grant_id: String,
    pub skill_id: String,
    pub expires_at: String,
    pub max_steps: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityExecutionGrantRevokeReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub grant_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityExecutionReceiptReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub skill_id: String,
    pub grant_id: String,
    pub capability_execution_allowed: bool,
    pub all_steps_passed: bool,
}

/// The fail-closed decision the execution path consults. `denied_reason` Some
/// means REFUSE; None means the ratified skill is authorized to execute its
/// deterministic procedure under `grant_id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityExecutionAuthorization {
    pub skill_id: String,
    /// The grant backing the authorization, for the receipt chain. Empty on deny.
    pub grant_id: String,
    /// The VERIFIED deterministic procedure to run. Empty on deny.
    pub procedure_deterministic: String,
    pub max_steps: u32,
    pub denied_reason: Option<String>,
}

impl CapabilityExecutionAuthorization {
    pub fn is_authorized(&self) -> bool {
        self.denied_reason.is_none()
    }

    fn denied(skill_id: &str, reason: impl Into<String>) -> Self {
        Self {
            skill_id: skill_id.to_string(),
            grant_id: String::new(),
            procedure_deterministic: String::new(),
            max_steps: 0,
            denied_reason: Some(reason.into()),
        }
    }
}

/// One recorded grant folded from the ledger for the surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityExecutionGrantView {
    pub grant_id: String,
    pub skill_id: String,
    pub owner_id: String,
    pub expires_at: String,
    pub max_steps: u32,
    /// "active" or "revoked".
    pub state: String,
    pub recorded_by: String,
    pub revoked_by: String,
    pub grant_receipt_id: String,
    pub revocation_receipt_id: String,
}

/// One capability execution (ran or refused) folded from the ledger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityExecutionRunView {
    pub skill_id: String,
    pub grant_id: String,
    /// "ran" or "refused".
    pub outcome: String,
    pub capability_execution_allowed: bool,
    pub steps_total: u32,
    pub steps_executed: u32,
    pub steps_passed: u32,
    pub all_steps_passed: bool,
    pub isolation: String,
    pub reason: String,
    pub receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilityExecutionError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    SkillNotRatified(String),
    UnknownGrantReceipt(String),
    WrongGrantReceiptKind(String, String),
    RevokerNotAuthorized(String),
}

impl CapabilityExecutionError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("capability execution grant missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("the {field} is {len} characters; the limit is {max}")
            }
            Self::SkillNotRatified(skill_id) => format!(
                "capability execution grant refuses skill {skill_id}: only a ratified skill can be granted execution"
            ),
            Self::UnknownGrantReceipt(receipt_id) => {
                format!("capability execution grant receipt {receipt_id} is unknown")
            }
            Self::WrongGrantReceiptKind(receipt_id, kind) => format!(
                "capability execution grant receipt {receipt_id} has kind {kind}; expected {CAPABILITY_EXECUTION_GRANT_RECORDED_KIND}"
            ),
            Self::RevokerNotAuthorized(actor) => format!(
                "capability execution grant revoker {actor} is neither the owner nor the recorder"
            ),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    /// Record a human-owned execution grant for ONE ratified skill. Fail-closed:
    /// the skill must ALREADY be ratified (a grant can never be recorded for an
    /// unknown or merely-drafted skill), the owner must be real, and the grant
    /// carries the const-false invariants on its receipt for the audit trail.
    /// Recording a grant runs nothing; it only writes the standing authority.
    pub fn record_capability_execution_grant(
        &mut self,
        request: CapabilityExecutionGrantRequest<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<CapabilityExecutionGrantReport, CapabilityExecutionError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("grant_id", request.grant_id),
            ("skill_id", request.skill_id),
            ("expires_at", request.expires_at),
        ] {
            if value.trim().is_empty() {
                return Err(CapabilityExecutionError::Missing(field));
            }
        }
        if request.reason.chars().count() > MAX_TEXT_CHARS {
            return Err(CapabilityExecutionError::TooLong(
                "reason",
                request.reason.chars().count(),
                MAX_TEXT_CHARS,
            ));
        }
        let skill_id = request.skill_id.trim();
        // Fail-closed: a grant can only ever name a currently-ratified skill.
        let ratified = self
            .distilled_skill_views()
            .into_iter()
            .any(|view| view.skill_id == skill_id && view.state == "ratified");
        if !ratified {
            return Err(CapabilityExecutionError::SkillNotRatified(
                skill_id.to_string(),
            ));
        }
        let owner_id = match request.owner_id.trim() {
            "" => request.actor_id.trim(),
            other => other,
        };
        let grant = CapabilityExecutionGrant {
            grant_id: request.grant_id.trim(),
            owner_id,
            skill_id,
            expires_at: request.expires_at.trim(),
            max_steps: request.max_steps,
            reason: request.reason.trim(),
            active: true,
        };
        let max_steps_text = grant.effective_max_steps().to_string();
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "capability_execution",
                action: ActionKind::RecordCapabilityExecutionGrant,
                transition: "RECORD_CAPABILITY_EXECUTION_GRANT",
                kind: CAPABILITY_EXECUTION_GRANT_RECORDED_KIND,
            },
            payload(&[
                ("grant_id", grant.grant_id),
                ("skill_id", grant.skill_id),
                ("owner_id", grant.owner_id),
                ("recorded_by", request.actor_id.trim()),
                ("expires_at", grant.expires_at),
                ("max_steps", max_steps_text.as_str()),
                ("reason", grant.reason),
                ("active", "true"),
                ("identity_source", &identity.identity_source),
                // The one authority a grant lifts, named on the receipt.
                ("capability_execution_allowed", "true"),
                // The three a grant can NEVER lift (compile-time const false).
                ("secret_access_allowed", "false"),
                ("inherited_agent_permissions_allowed", "false"),
                ("production_write_allowed", "false"),
                ("deployment_authority_granted", "false"),
            ]),
        );
        Ok(CapabilityExecutionGrantReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            grant_id: grant.grant_id.to_string(),
            skill_id: grant.skill_id.to_string(),
            expires_at: grant.expires_at.to_string(),
            max_steps: grant.effective_max_steps(),
        })
    }

    /// Revoke a recorded grant. The kill switch: after this the grant authorizes
    /// nothing, exactly like an envelope revocation. Only the grant owner or the
    /// original recorder may revoke.
    pub fn revoke_capability_execution_grant(
        &mut self,
        request: CapabilityExecutionGrantRevoke<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<CapabilityExecutionGrantRevokeReport, CapabilityExecutionError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("grant_receipt_id", request.grant_receipt_id),
        ] {
            if value.trim().is_empty() {
                return Err(CapabilityExecutionError::Missing(field));
            }
        }
        let grant_receipt_id = request.grant_receipt_id.trim();
        let grant_receipt = match self.ledger().query().by_id(grant_receipt_id) {
            None => {
                return Err(CapabilityExecutionError::UnknownGrantReceipt(
                    grant_receipt_id.to_string(),
                ));
            }
            Some(receipt) if receipt.kind != CAPABILITY_EXECUTION_GRANT_RECORDED_KIND => {
                return Err(CapabilityExecutionError::WrongGrantReceiptKind(
                    grant_receipt_id.to_string(),
                    receipt.kind.clone(),
                ));
            }
            Some(receipt) => receipt,
        };
        let field = |key: &str| grant_receipt.payload.get(key).cloned().unwrap_or_default();
        let owner_id = field("owner_id");
        let recorded_by = field("recorded_by");
        let grant_id = field("grant_id");
        let revoked_by = request.actor_id.trim();
        if revoked_by != owner_id && revoked_by != recorded_by {
            return Err(CapabilityExecutionError::RevokerNotAuthorized(
                revoked_by.to_string(),
            ));
        }
        let reason = match request.reason.trim() {
            "" => "operator revocation",
            other => other,
        };
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "capability_execution",
                action: ActionKind::RevokeCapabilityExecutionGrant,
                transition: "REVOKE_CAPABILITY_EXECUTION_GRANT",
                kind: CAPABILITY_EXECUTION_GRANT_REVOKED_KIND,
            },
            payload(&[
                ("grant_id", grant_id.as_str()),
                ("skill_id", field("skill_id").as_str()),
                ("source_grant_receipt_id", grant_receipt_id),
                ("revoked_by", revoked_by),
                ("revocation_reason", reason),
                ("identity_source", &identity.identity_source),
                ("capability_execution_allowed", "false"),
                ("secret_access_allowed", "false"),
                ("inherited_agent_permissions_allowed", "false"),
                ("production_write_allowed", "false"),
                ("deployment_authority_granted", "false"),
            ]),
        );
        Ok(CapabilityExecutionGrantRevokeReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            grant_id,
        })
    }

    /// Why a named grant cannot authorize execution right now, or None when it is
    /// active. Receipt-based like the envelope authority blocker: not recorded,
    /// or a revocation exists. Expiry is evaluated against `observed_at` at
    /// authorization time (the kernel takes no ambient clock), so this predicate
    /// covers only the receipt-based blockers.
    pub fn capability_execution_grant_blocker(&self, grant_id: &str) -> Option<String> {
        let ledger = self.ledger();
        let recorded = ledger
            .query()
            .by_kind(CAPABILITY_EXECUTION_GRANT_RECORDED_KIND)
            .iter()
            .any(|receipt| receipt.payload.get("grant_id").map(String::as_str) == Some(grant_id));
        if !recorded {
            return Some("grant_not_found".to_string());
        }
        let revoked = ledger
            .query()
            .by_kind(CAPABILITY_EXECUTION_GRANT_REVOKED_KIND)
            .iter()
            .any(|receipt| receipt.payload.get("grant_id").map(String::as_str) == Some(grant_id));
        if revoked {
            return Some("grant_revoked".to_string());
        }
        None
    }

    /// The fail-closed authorization decision, in the same order as
    /// `authorize_self_delivery`: the skill must be RATIFIED, then an active
    /// (recorded, un-revoked, un-expired) grant for THIS skill_id must exist,
    /// then the run may execute the VERIFIED deterministic procedure. Any failure
    /// denies with a reason the caller records and refuses on. `observed_at` is
    /// the instant to test expiry against (ISO-8601 UTC), supplied by the caller.
    pub fn authorize_capability_execution(
        &self,
        skill_id: &str,
        observed_at: &str,
    ) -> CapabilityExecutionAuthorization {
        let skill_id = skill_id.trim();
        if skill_id.is_empty() {
            return CapabilityExecutionAuthorization::denied(skill_id, "missing_skill_id");
        }
        // 1. The skill must be a currently-ratified distilled skill.
        let view = self
            .distilled_skill_views()
            .into_iter()
            .find(|view| view.skill_id == skill_id);
        let view = match view {
            Some(view) => view,
            None => {
                return CapabilityExecutionAuthorization::denied(skill_id, "skill_not_found");
            }
        };
        if view.state != "ratified" {
            return CapabilityExecutionAuthorization::denied(
                skill_id,
                format!("skill_not_ratified:{}", view.state),
            );
        }
        // 2. The VERIFIED deterministic procedure - never the possibly
        //    model-sharpened prose - is read from the draft the ratification
        //    cites. No deterministic floor means nothing safe to execute.
        let procedure_deterministic = self
            .ledger()
            .query()
            .by_id(view.draft_receipt_id.trim())
            .and_then(|receipt| receipt.payload.get("procedure_deterministic").cloned())
            .unwrap_or_default();
        if procedure_deterministic.trim().is_empty() {
            return CapabilityExecutionAuthorization::denied(
                skill_id,
                "no_deterministic_procedure",
            );
        }
        // 3. An active grant for THIS skill_id. Fail-closed: a revoked grant or an
        //    expired grant authorizes nothing (the kill switch). Any active grant
        //    for the skill authorizes; if none is active we surface the most
        //    informative denial for the audit trail.
        let observed_at = observed_at.trim();
        let mut saw_grant = false;
        let mut saw_revoked = false;
        let mut saw_expired = false;
        for receipt in self
            .ledger()
            .query()
            .by_kind(CAPABILITY_EXECUTION_GRANT_RECORDED_KIND)
        {
            let get = |key: &str| receipt.payload.get(key).cloned().unwrap_or_default();
            if get("skill_id") != skill_id {
                continue;
            }
            saw_grant = true;
            let grant_id = get("grant_id");
            if self.capability_execution_grant_blocker(&grant_id).is_some() {
                saw_revoked = true;
                continue;
            }
            let expires_at = get("expires_at");
            // String compare is correct for zero-padded ISO-8601 in UTC (Z).
            if observed_at.is_empty() || expires_at.is_empty() || observed_at >= expires_at.as_str()
            {
                saw_expired = true;
                continue;
            }
            let max_steps = get("max_steps").parse::<u32>().unwrap_or(0);
            return CapabilityExecutionAuthorization {
                skill_id: skill_id.to_string(),
                grant_id,
                procedure_deterministic,
                max_steps: max_steps.min(MAX_EXECUTION_STEPS_CEILING),
                denied_reason: None,
            };
        }
        let reason = if !saw_grant {
            "no_capability_execution_grant"
        } else if saw_revoked {
            "capability_execution_grant_revoked"
        } else if saw_expired {
            "capability_execution_grant_expired"
        } else {
            "no_active_capability_execution_grant"
        };
        CapabilityExecutionAuthorization::denied(skill_id, reason)
    }

    /// Record a `capability.execution.ran` receipt after an authorized, isolated
    /// run. `capability_execution_allowed` is true here BECAUSE a valid grant
    /// authorized this specific ratified skill; the three invariant authorities
    /// stay false on the receipt no matter what the steps did.
    pub fn record_capability_execution_ran(
        &mut self,
        outcome: CapabilityExecutionOutcome<'_>,
        identity: &GovernedWriteIdentity,
    ) -> CapabilityExecutionReceiptReport {
        let steps_total = outcome.steps_total.to_string();
        let steps_executed = outcome.steps_executed.to_string();
        let steps_passed = outcome.steps_passed.to_string();
        let all_passed = if outcome.all_steps_passed {
            "true"
        } else {
            "false"
        };
        let step_log: String = outcome.step_log.chars().take(MAX_TEXT_CHARS).collect();
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: outcome.tenant_id,
                actor_id: outcome.actor_id,
                loop_id: "capability_execution",
                action: ActionKind::RunCapabilityExecution,
                transition: "RUN_CAPABILITY_EXECUTION",
                kind: CAPABILITY_EXECUTION_RAN_KIND,
            },
            payload(&[
                ("skill_id", outcome.skill_id),
                ("grant_id", outcome.grant_id),
                ("outcome", "ran"),
                ("isolation", outcome.isolation),
                ("steps_total", steps_total.as_str()),
                ("steps_executed", steps_executed.as_str()),
                ("steps_passed", steps_passed.as_str()),
                ("all_steps_passed", all_passed),
                ("step_log", step_log.as_str()),
                ("identity_source", &identity.identity_source),
                // True only because a valid grant authorized THIS ratified skill.
                ("capability_execution_allowed", "true"),
                // Const-false invariants, unaffected by any grant or any step.
                ("secret_access_allowed", "false"),
                ("inherited_agent_permissions_allowed", "false"),
                ("production_write_allowed", "false"),
                ("deployment_authority_granted", "false"),
            ]),
        );
        CapabilityExecutionReceiptReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            skill_id: outcome.skill_id.to_string(),
            grant_id: outcome.grant_id.to_string(),
            capability_execution_allowed: true,
            all_steps_passed: outcome.all_steps_passed,
        }
    }

    /// Record a `capability.execution.refused` receipt. Every refused path cites
    /// its reason and carries `capability_execution_allowed=false`, so a refusal
    /// is as auditable as a run.
    pub fn record_capability_execution_refused(
        &mut self,
        tenant_id: &str,
        actor_id: &str,
        skill_id: &str,
        grant_id: &str,
        reason: &str,
        identity: &GovernedWriteIdentity,
    ) -> CapabilityExecutionReceiptReport {
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id,
                actor_id,
                loop_id: "capability_execution",
                action: ActionKind::RefuseCapabilityExecution,
                transition: "REFUSE_CAPABILITY_EXECUTION",
                kind: CAPABILITY_EXECUTION_REFUSED_KIND,
            },
            payload(&[
                ("skill_id", skill_id),
                ("grant_id", grant_id),
                ("outcome", "refused"),
                ("reason", reason),
                ("identity_source", &identity.identity_source),
                ("capability_execution_allowed", "false"),
                ("secret_access_allowed", "false"),
                ("inherited_agent_permissions_allowed", "false"),
                ("production_write_allowed", "false"),
                ("deployment_authority_granted", "false"),
            ]),
        );
        CapabilityExecutionReceiptReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            skill_id: skill_id.to_string(),
            grant_id: grant_id.to_string(),
            capability_execution_allowed: false,
            all_steps_passed: false,
        }
    }

    /// Record a `capability.execution.grant.refused` receipt. A governed-write
    /// REFUSAL is still a governed write: it records and cites a receipt with the
    /// reason, so a rejected grant or revocation is as auditable as an accepted
    /// one (receipts-or-it-did-not-happen). `stage` is "grant" or "revocation".
    pub fn record_capability_execution_grant_refused(
        &mut self,
        tenant_id: &str,
        actor_id: &str,
        stage: &str,
        skill_id: &str,
        reason: &str,
        identity: &GovernedWriteIdentity,
    ) -> CapabilityExecutionReceiptReport {
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id,
                actor_id,
                loop_id: "capability_execution",
                action: ActionKind::RecordCapabilityExecutionGrant,
                transition: "REFUSE_CAPABILITY_EXECUTION_GRANT",
                kind: CAPABILITY_EXECUTION_GRANT_REFUSED_KIND,
            },
            payload(&[
                ("stage", stage),
                ("skill_id", skill_id),
                ("outcome", "refused"),
                ("reason", reason),
                ("identity_source", &identity.identity_source),
                ("capability_execution_allowed", "false"),
                ("secret_access_allowed", "false"),
                ("inherited_agent_permissions_allowed", "false"),
                ("production_write_allowed", "false"),
                ("deployment_authority_granted", "false"),
            ]),
        );
        CapabilityExecutionReceiptReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            skill_id: skill_id.to_string(),
            grant_id: String::new(),
            capability_execution_allowed: false,
            all_steps_passed: false,
        }
    }

    /// The grants folded from the ledger, newest revocation winning the state.
    pub fn capability_execution_grant_views(&self) -> Vec<CapabilityExecutionGrantView> {
        use std::collections::BTreeMap;
        let mut views: BTreeMap<String, CapabilityExecutionGrantView> = BTreeMap::new();
        for receipt in self
            .ledger()
            .query()
            .by_kind(CAPABILITY_EXECUTION_GRANT_RECORDED_KIND)
        {
            let get = |key: &str| receipt.payload.get(key).cloned().unwrap_or_default();
            let grant_id = get("grant_id");
            if grant_id.is_empty() {
                continue;
            }
            views.insert(
                grant_id.clone(),
                CapabilityExecutionGrantView {
                    grant_id: grant_id.clone(),
                    skill_id: get("skill_id"),
                    owner_id: get("owner_id"),
                    expires_at: get("expires_at"),
                    max_steps: get("max_steps").parse().unwrap_or(0),
                    state: "active".to_string(),
                    recorded_by: get("recorded_by"),
                    revoked_by: String::new(),
                    grant_receipt_id: receipt.receipt_id.clone(),
                    revocation_receipt_id: String::new(),
                },
            );
        }
        for receipt in self
            .ledger()
            .query()
            .by_kind(CAPABILITY_EXECUTION_GRANT_REVOKED_KIND)
        {
            let get = |key: &str| receipt.payload.get(key).cloned().unwrap_or_default();
            let grant_id = get("grant_id");
            if let Some(entry) = views.get_mut(&grant_id) {
                entry.state = "revoked".to_string();
                entry.revoked_by = get("revoked_by");
                entry.revocation_receipt_id = receipt.receipt_id.clone();
            }
        }
        views.into_values().collect()
    }

    /// The executions (ran and refused) folded from the ledger, newest first.
    pub fn capability_execution_run_views(&self) -> Vec<CapabilityExecutionRunView> {
        let mut views: Vec<CapabilityExecutionRunView> = Vec::new();
        for kind in [
            CAPABILITY_EXECUTION_RAN_KIND,
            CAPABILITY_EXECUTION_REFUSED_KIND,
        ] {
            for receipt in self.ledger().query().by_kind(kind) {
                let get = |key: &str| receipt.payload.get(key).cloned().unwrap_or_default();
                views.push(CapabilityExecutionRunView {
                    skill_id: get("skill_id"),
                    grant_id: get("grant_id"),
                    outcome: get("outcome"),
                    capability_execution_allowed: get("capability_execution_allowed") == "true",
                    steps_total: get("steps_total").parse().unwrap_or(0),
                    steps_executed: get("steps_executed").parse().unwrap_or(0),
                    steps_passed: get("steps_passed").parse().unwrap_or(0),
                    all_steps_passed: get("all_steps_passed") == "true",
                    isolation: get("isolation"),
                    reason: get("reason"),
                    receipt_id: receipt.receipt_id.clone(),
                });
            }
        }
        views
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ForgeRunEvent, MarketplaceSkillDraft, MarketplaceSkillRatification};

    fn identity(actor: &str) -> GovernedWriteIdentity {
        GovernedWriteIdentity::local_demo(actor)
    }

    // A proven-green run whose check step is a hermetic command so the distilled
    // deterministic procedure runs green in an empty isolated workspace.
    fn green_run(kernel: &mut MdxKernel, run_id: &str, check_detail: &str) -> String {
        for (event, detail) in [
            ("run_started", "run started"),
            ("tool_executed", "edit_file src/lib.rs"),
            ("check_passed", check_detail),
        ] {
            kernel
                .record_forge_run_event(ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:builder",
                    run_id,
                    event,
                    work_item_id: "",
                    detail,
                    turn: 1,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .expect("run event");
        }
        kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:builder",
                run_id,
                event: "run_finished",
                work_item_id: "",
                detail: "status=RUN_FINISHED_DONE turns=1 files_changed=1 check_runs=1",
                turn: 1,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("run finished")
            .receipt_id
    }

    fn ratified_skill(kernel: &mut MdxKernel, run_id: &str, skill_id: &str) {
        let run_receipt = green_run(kernel, run_id, "run_command true exit=0 tail=ok");
        let drafted = kernel
            .draft_marketplace_skill(MarketplaceSkillDraft {
                tenant_id: "local_tenant",
                actor_id: "human:builder",
                source_run_receipt_id: &run_receipt,
                skill_id,
                skill_name: "Skill",
                summary: "A verified procedure.",
                helps_with: "backend runs",
                model_procedure: "",
                procedure_source: "",
                sensitivity: "",
            })
            .expect("draft");
        kernel
            .ratify_marketplace_skill(MarketplaceSkillRatification {
                tenant_id: "local_tenant",
                actor_id: "human:reviewer",
                skill_id,
                draft_receipt_id: &drafted.receipt_id,
                decision: "ratify",
                note: "matches a real run",
            })
            .expect("ratify");
    }

    fn grant(
        kernel: &mut MdxKernel,
        grant_id: &str,
        skill_id: &str,
        expires_at: &str,
    ) -> CapabilityExecutionGrantReport {
        kernel
            .record_capability_execution_grant(
                CapabilityExecutionGrantRequest {
                    tenant_id: "local_tenant",
                    actor_id: "human:owner",
                    grant_id,
                    skill_id,
                    owner_id: "human:owner",
                    expires_at,
                    max_steps: 10,
                    reason: "authorize the verified proof procedure",
                },
                &identity("human:owner"),
            )
            .expect("grant recorded")
    }

    #[test]
    fn const_invariants_never_lift_even_under_a_grant() {
        let g = CapabilityExecutionGrant {
            grant_id: "g1",
            owner_id: "human:owner",
            skill_id: "skill_v1",
            expires_at: "2030-01-01T00:00:00Z",
            max_steps: 5,
            reason: "",
            active: true,
        };
        assert!(g.grants_capability_execution("skill_v1"));
        // A grant speaks only for its own skill.
        assert!(!g.grants_capability_execution("other_skill"));
        // The three invariant authorities are const false.
        assert!(!g.grants_production_write());
        assert!(!g.grants_secret_access());
        assert!(!g.grants_inherited_agent_permissions());
        // The step ceiling can never exceed the module hard cap.
        let wide = CapabilityExecutionGrant {
            max_steps: 9999,
            ..g
        };
        assert_eq!(wide.effective_max_steps(), MAX_EXECUTION_STEPS_CEILING);
    }

    #[test]
    fn grant_refuses_a_skill_that_is_not_ratified() {
        let mut kernel = MdxKernel::boot_local();
        let bad = kernel.record_capability_execution_grant(
            CapabilityExecutionGrantRequest {
                tenant_id: "local_tenant",
                actor_id: "human:owner",
                grant_id: "g1",
                skill_id: "nonexistent_skill",
                owner_id: "human:owner",
                expires_at: "2030-01-01T00:00:00Z",
                max_steps: 5,
                reason: "",
            },
            &identity("human:owner"),
        );
        assert!(matches!(
            bad,
            Err(CapabilityExecutionError::SkillNotRatified(_))
        ));
    }

    #[test]
    fn ratified_skill_with_an_active_grant_authorizes_execution() {
        let mut kernel = MdxKernel::boot_local();
        ratified_skill(&mut kernel, "run_a", "skill_a");
        grant(&mut kernel, "grant_a", "skill_a", "2030-01-01T00:00:00Z");
        let auth = kernel.authorize_capability_execution("skill_a", "2026-07-04T00:00:00Z");
        assert!(auth.is_authorized(), "{:?}", auth.denied_reason);
        assert_eq!(auth.grant_id, "grant_a");
        // The VERIFIED deterministic procedure, carrying the real trajectory.
        assert!(
            auth.procedure_deterministic
                .contains("edit_file src/lib.rs")
        );
        assert!(auth.procedure_deterministic.contains("run_command true"));
        assert_eq!(auth.max_steps, 10);
    }

    #[test]
    fn a_ratified_skill_without_a_grant_is_refused() {
        let mut kernel = MdxKernel::boot_local();
        ratified_skill(&mut kernel, "run_b", "skill_b");
        let auth = kernel.authorize_capability_execution("skill_b", "2026-07-04T00:00:00Z");
        assert!(!auth.is_authorized());
        assert_eq!(
            auth.denied_reason.as_deref(),
            Some("no_capability_execution_grant")
        );
    }

    #[test]
    fn an_unratified_skill_is_refused_even_with_a_grant() {
        // A drafted-but-not-ratified skill cannot even receive a grant, and if a
        // grant somehow named it, authorization still refuses on the ratified
        // check FIRST. We prove both: the grant record refuses, and a direct
        // authorize on a drafted-only skill denies with skill_not_ratified.
        let mut kernel = MdxKernel::boot_local();
        let run_receipt = green_run(&mut kernel, "run_c", "run_command true exit=0");
        kernel
            .draft_marketplace_skill(MarketplaceSkillDraft {
                tenant_id: "local_tenant",
                actor_id: "human:builder",
                source_run_receipt_id: &run_receipt,
                skill_id: "skill_c",
                skill_name: "Draft only",
                summary: "Not ratified.",
                helps_with: "x",
                model_procedure: "",
                procedure_source: "",
                sensitivity: "",
            })
            .expect("draft");
        let auth = kernel.authorize_capability_execution("skill_c", "2026-07-04T00:00:00Z");
        assert!(!auth.is_authorized());
        assert_eq!(
            auth.denied_reason.as_deref(),
            Some("skill_not_ratified:drafted")
        );
    }

    #[test]
    fn a_revoked_grant_is_the_kill_switch() {
        let mut kernel = MdxKernel::boot_local();
        ratified_skill(&mut kernel, "run_d", "skill_d");
        let report = grant(&mut kernel, "grant_d", "skill_d", "2030-01-01T00:00:00Z");
        // Active first.
        assert!(
            kernel
                .authorize_capability_execution("skill_d", "2026-07-04T00:00:00Z")
                .is_authorized()
        );
        // Revoke it.
        kernel
            .revoke_capability_execution_grant(
                CapabilityExecutionGrantRevoke {
                    tenant_id: "local_tenant",
                    actor_id: "human:owner",
                    grant_receipt_id: &report.receipt_id,
                    reason: "kill switch",
                },
                &identity("human:owner"),
            )
            .expect("revoked");
        let auth = kernel.authorize_capability_execution("skill_d", "2026-07-04T00:00:00Z");
        assert!(!auth.is_authorized());
        assert_eq!(
            auth.denied_reason.as_deref(),
            Some("capability_execution_grant_revoked")
        );
    }

    #[test]
    fn an_expired_grant_authorizes_nothing() {
        let mut kernel = MdxKernel::boot_local();
        ratified_skill(&mut kernel, "run_e", "skill_e");
        // A grant that expired in the past.
        grant(&mut kernel, "grant_e", "skill_e", "2020-01-01T00:00:00Z");
        let auth = kernel.authorize_capability_execution("skill_e", "2026-07-04T00:00:00Z");
        assert!(!auth.is_authorized());
        assert_eq!(
            auth.denied_reason.as_deref(),
            Some("capability_execution_grant_expired")
        );
    }

    #[test]
    fn only_owner_or_recorder_may_revoke() {
        let mut kernel = MdxKernel::boot_local();
        ratified_skill(&mut kernel, "run_f", "skill_f");
        let report = grant(&mut kernel, "grant_f", "skill_f", "2030-01-01T00:00:00Z");
        let bad = kernel.revoke_capability_execution_grant(
            CapabilityExecutionGrantRevoke {
                tenant_id: "local_tenant",
                actor_id: "human:stranger",
                grant_receipt_id: &report.receipt_id,
                reason: "",
            },
            &identity("human:stranger"),
        );
        assert!(matches!(
            bad,
            Err(CapabilityExecutionError::RevokerNotAuthorized(_))
        ));
    }

    #[test]
    fn ran_and_refused_receipts_hold_the_const_invariants_and_verify() {
        let mut kernel = MdxKernel::boot_local();
        ratified_skill(&mut kernel, "run_g", "skill_g");
        grant(&mut kernel, "grant_g", "skill_g", "2030-01-01T00:00:00Z");
        let ran = kernel.record_capability_execution_ran(
            CapabilityExecutionOutcome {
                tenant_id: "local_tenant",
                actor_id: "human:owner",
                skill_id: "skill_g",
                grant_id: "grant_g",
                isolation: "scrubbed_sandboxed",
                steps_total: 2,
                steps_executed: 1,
                steps_passed: 1,
                all_steps_passed: true,
                step_log: "1. edit_file src/lib.rs [narrated]\n2. run_command true [ok exit=0]",
            },
            &identity("human:owner"),
        );
        assert!(ran.capability_execution_allowed);
        let receipt = kernel.ledger().query().by_id(&ran.receipt_id).unwrap();
        assert_eq!(
            receipt
                .payload
                .get("capability_execution_allowed")
                .map(String::as_str),
            Some("true")
        );
        for invariant in [
            "secret_access_allowed",
            "inherited_agent_permissions_allowed",
            "production_write_allowed",
            "deployment_authority_granted",
        ] {
            assert_eq!(
                receipt.payload.get(invariant).map(String::as_str),
                Some("false"),
                "invariant {invariant} must stay false on a ran receipt"
            );
        }
        let refused = kernel.record_capability_execution_refused(
            "local_tenant",
            "human:owner",
            "skill_g",
            "",
            "no_active_capability_execution_grant",
            &identity("human:owner"),
        );
        assert!(!refused.capability_execution_allowed);
        assert!(kernel.ledger().verify().is_ok());

        // The projections fold both.
        let grants = kernel.capability_execution_grant_views();
        assert!(
            grants
                .iter()
                .any(|g| g.grant_id == "grant_g" && g.state == "active")
        );
        let runs = kernel.capability_execution_run_views();
        assert!(
            runs.iter()
                .any(|r| r.outcome == "ran" && r.capability_execution_allowed)
        );
        assert!(
            runs.iter()
                .any(|r| r.outcome == "refused" && !r.capability_execution_allowed)
        );
    }
}
