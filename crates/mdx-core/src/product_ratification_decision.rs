//! The Product ratification decision: the bet's sibling of the Strategy
//! verb. The product surface presents the shaped bet and demands human
//! ratification; this records whether the human backed it, held it for more
//! signal, or sent it back for reshaping - on the chain, with the verified
//! identity, satisfying the contract's required product.ratification.recorded
//! receipt with a real record instead of a promise.
//!
//! Recording a decision opens no authority: strategy staging, budget,
//! worker handoff, Forge, and ship paths stay blocked exactly as before.
//! The decision is human judgment made durable, not a key that turns.

use crate::{
    ActionKind, ActorId, CorrelationIds, GovernedWriteIdentity, LoopId, LoopRun, MdxKernel,
    StorageProvider, TenantId, TraceId, WorkflowId, payload,
};

/// The three options the product surface presents. A decision outside this
/// set is refused: the vocabulary belongs to the surface contract.
pub const PRODUCT_RATIFICATION_DECISIONS: &[&str] = &[
    "back_the_bet",
    "hold_for_more_signal",
    "send_back_for_reshaping",
];

const MAX_REASON_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug)]
pub struct ProductRatificationDecision<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub bet_id: &'a str,
    pub decision: &'a str,
    pub reason: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductRatificationDecisionReport {
    pub decision_receipt_id: String,
    pub policy_decision_id: String,
    pub decision: String,
    pub terminal_state: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProductRatificationDecisionError {
    Missing(&'static str),
    UnknownDecision(String),
    ReasonTooLong(usize),
}

impl ProductRatificationDecisionError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("product ratification decision missing {field}"),
            Self::UnknownDecision(other) => format!(
                "product ratification decision must be one of {}; got {other}",
                PRODUCT_RATIFICATION_DECISIONS.join(", ")
            ),
            Self::ReasonTooLong(len) => format!(
                "product ratification reason is {len} characters; the limit is {MAX_REASON_CHARS}"
            ),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_product_ratification_decision_local(
        &mut self,
        request: ProductRatificationDecision<'_>,
    ) -> Result<ProductRatificationDecisionReport, ProductRatificationDecisionError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_product_ratification_decision_local_with_identity(request, &identity)
    }

    pub fn save_product_ratification_decision_local_with_identity(
        &mut self,
        request: ProductRatificationDecision<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ProductRatificationDecisionReport, ProductRatificationDecisionError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("bet_id", request.bet_id),
            ("decision", request.decision),
        ] {
            if value.trim().is_empty() {
                return Err(ProductRatificationDecisionError::Missing(field));
            }
        }
        if !PRODUCT_RATIFICATION_DECISIONS.contains(&request.decision) {
            return Err(ProductRatificationDecisionError::UnknownDecision(
                request.decision.to_string(),
            ));
        }
        if request.reason.chars().count() > MAX_REASON_CHARS {
            return Err(ProductRatificationDecisionError::ReasonTooLong(
                request.reason.chars().count(),
            ));
        }
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("product_ratification"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision =
            self.decide_with_receipt(&correlation, ActionKind::RecordProductRatificationDecision);
        let receipt = self.transition_receipt(
            &run_id,
            "RECORD_PRODUCT_RATIFICATION_DECISION",
            &correlation,
            &decision,
            "product.ratification.recorded",
            payload(&[
                ("bet_id", request.bet_id),
                ("decision", request.decision),
                ("reason", request.reason),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(ProductRatificationDecisionReport {
            decision_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            decision: request.decision.to_string(),
            terminal_state: "PRODUCT_RATIFICATION_RECORDED_AUTHORITY_UNCHANGED",
        })
    }
}
