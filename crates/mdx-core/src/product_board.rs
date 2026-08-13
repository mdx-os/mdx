//! The product pipeline's verbs. A bet is a card one altitude below a
//! strategy direction: what we build next and what it must prove. Four
//! verbs make the pipeline real:
//!
//! - SHAPE a bet draft: create it with one sentence, then re-shape it by
//!   bet_id as its anatomy fills in. Every shape is a receipt; the
//!   projection folds to the latest.
//! - state a CONDITION on a bet ("what would have to be true" - the
//!   bet's assumptions), and UPDATE it with the same evidence discipline
//!   as the strategy board: holds and broken refuse without evidence.
//! - RESOLVE a bet: landed, killed, or dismissed - landing carries the
//!   metric result, and "measurement unavailable" is a legal, honest
//!   state, never a fake closure.
//!
//! None of these verbs opens authority. Backing a bet stays the human
//! ratification verb; staffing it stays the Talent chain.

use crate::strategy_board::BoardWrite;
use crate::{
    ActionKind, GovernedWriteIdentity, MdxKernel, STRATEGY_CONDITION_STATUSES, StorageProvider,
    payload,
};

/// One condition vocabulary across the whole company: bets reuse the
/// strategy board's statuses so evidence reads the same everywhere.
pub use crate::STRATEGY_CONDITION_STATUSES as PRODUCT_CONDITION_STATUSES;

/// How a bet may leave the pipeline.
pub const PRODUCT_RESOLUTION_OUTCOMES: &[&str] = &["landed", "killed", "dismissed"];

const MAX_BET_CHARS: usize = 300;
const MAX_FIELD_CHARS: usize = 500;
const MAX_LONG_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug)]
pub struct ProductBetDraft<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    /// Empty means mint a new bet; a known id means re-shape it.
    pub bet_id: &'a str,
    /// The bet in one sentence: what we build, for whom.
    pub bet: &'a str,
    pub for_whom: &'a str,
    pub success_metric: &'a str,
    pub kill_condition: &'a str,
    /// The strategy card or direction box this bet serves.
    pub direction_ref: &'a str,
    /// What ships first.
    pub slice: &'a str,
    /// Explicitly not in the slice.
    pub slice_not: &'a str,
    /// Where the opportunity came from: a signal, a page, a receipt.
    pub signal_ref: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub struct ProductBetCondition<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub bet_id: &'a str,
    pub claim: &'a str,
    pub make_or_break: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ProductBetConditionUpdate<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub condition_id: &'a str,
    pub status: &'a str,
    pub evidence_ref: &'a str,
    pub note: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub struct ProductBetResolution<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub bet_id: &'a str,
    pub outcome: &'a str,
    /// The funded metric's actual result, when it landed.
    pub metric_result: &'a str,
    /// True when the metric could not be observed - said plainly instead
    /// of faking closure.
    pub measurement_unavailable: bool,
    pub learned: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductBoardWriteReport {
    pub record_id: String,
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub terminal_state: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProductBoardError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    UnknownStatus(String),
    UnknownOutcome(String),
    EvidenceRequired(String),
    MetricRequired,
}

impl ProductBoardError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("product board write missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("product board {field} is {len} characters; the limit is {max}")
            }
            Self::UnknownStatus(other) => format!(
                "condition status must be one of {}; got {other}",
                STRATEGY_CONDITION_STATUSES.join(", ")
            ),
            Self::UnknownOutcome(other) => format!(
                "bet resolution outcome must be one of {}; got {other}",
                PRODUCT_RESOLUTION_OUTCOMES.join(", ")
            ),
            Self::EvidenceRequired(status) => format!(
                "a bet condition can only move to {status} with an evidence_ref - \
                 a verdict without evidence is an opinion"
            ),
            Self::MetricRequired => "a landed bet records its metric result, or says plainly \
                 that measurement was unavailable - never neither"
                .to_string(),
        }
    }
}

fn check_len(field: &'static str, value: &str, max: usize) -> Result<(), ProductBoardError> {
    let len = value.chars().count();
    if len > max {
        return Err(ProductBoardError::TooLong(field, len, max));
    }
    Ok(())
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_product_bet_draft_local_with_identity(
        &mut self,
        request: ProductBetDraft<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ProductBoardWriteReport, ProductBoardError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("bet", request.bet),
        ] {
            if value.trim().is_empty() {
                return Err(ProductBoardError::Missing(field));
            }
        }
        check_len("bet", request.bet, MAX_BET_CHARS)?;
        for (field, value) in [
            ("for_whom", request.for_whom),
            ("success_metric", request.success_metric),
            ("kill_condition", request.kill_condition),
            ("direction_ref", request.direction_ref),
            ("signal_ref", request.signal_ref),
        ] {
            check_len(field, value, MAX_FIELD_CHARS)?;
        }
        check_len("slice", request.slice, MAX_LONG_CHARS)?;
        check_len("slice_not", request.slice_not, MAX_LONG_CHARS)?;
        let bet_id = if request.bet_id.trim().is_empty() {
            self.ids.next("product_bet")
        } else {
            request.bet_id.trim().to_string()
        };
        let requested_signal_ref = request.signal_ref.trim();
        let source_signal_receipt_id = if !requested_signal_ref.is_empty()
            && self.ledger().query().by_id(requested_signal_ref).is_some()
        {
            requested_signal_ref.to_string()
        } else {
            self.record_board_write(
                BoardWrite {
                    tenant_id: request.tenant_id,
                    actor_id: request.actor_id,
                    loop_id: "product_shaping",
                    action: ActionKind::IngestProductSignal,
                    transition: "INGEST_PRODUCT_SIGNAL",
                    kind: "product.signal.ingested",
                },
                payload(&[
                    (
                        "signal_source",
                        if requested_signal_ref.is_empty() {
                            "product_direction"
                        } else {
                            requested_signal_ref
                        },
                    ),
                    ("signal_kind", "operator_request"),
                    ("source_surface", "product"),
                    ("identity_source", &identity.identity_source),
                    ("authority_opened", "none"),
                    ("production_write_allowed", "false"),
                ]),
            )
            .0
        };
        let signal_ref = if requested_signal_ref.is_empty() {
            source_signal_receipt_id.as_str()
        } else {
            requested_signal_ref
        };
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "product_shaping",
                action: ActionKind::RecordProductBetDraft,
                transition: "RECORD_PRODUCT_BET_DRAFT",
                kind: "product.bet.shaped",
            },
            payload(&[
                ("bet_id", &bet_id),
                ("bet", request.bet),
                ("for_whom", request.for_whom),
                ("success_metric", request.success_metric),
                ("kill_condition", request.kill_condition),
                ("direction_ref", request.direction_ref),
                ("slice", request.slice),
                ("slice_not", request.slice_not),
                ("source_signal_receipt_id", &source_signal_receipt_id),
                ("signal_ref", signal_ref),
                ("identity_source", &identity.identity_source),
                ("ratification_required", "true"),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(ProductBoardWriteReport {
            record_id: bet_id,
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            terminal_state: "PRODUCT_BET_SHAPED_AWAITING_HUMAN_RATIFICATION",
        })
    }

    pub fn save_product_bet_condition_local_with_identity(
        &mut self,
        request: ProductBetCondition<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ProductBoardWriteReport, ProductBoardError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("bet_id", request.bet_id),
            ("claim", request.claim),
        ] {
            if value.trim().is_empty() {
                return Err(ProductBoardError::Missing(field));
            }
        }
        check_len("claim", request.claim, MAX_BET_CHARS)?;
        let condition_id = self.ids.next("product_condition");
        let make_or_break = if request.make_or_break {
            "true"
        } else {
            "false"
        };
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "product_shaping",
                action: ActionKind::RecordProductBetCondition,
                transition: "RECORD_PRODUCT_BET_CONDITION",
                kind: "product.condition.stated",
            },
            payload(&[
                ("condition_id", &condition_id),
                ("bet_id", request.bet_id),
                ("claim", request.claim),
                ("make_or_break", make_or_break),
                ("status", "assumed"),
                ("identity_source", &identity.identity_source),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(ProductBoardWriteReport {
            record_id: condition_id,
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            terminal_state: "PRODUCT_BET_CONDITION_STATED_ASSUMED",
        })
    }

    pub fn save_product_bet_condition_update_local_with_identity(
        &mut self,
        request: ProductBetConditionUpdate<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ProductBoardWriteReport, ProductBoardError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("condition_id", request.condition_id),
            ("status", request.status),
        ] {
            if value.trim().is_empty() {
                return Err(ProductBoardError::Missing(field));
            }
        }
        if !STRATEGY_CONDITION_STATUSES.contains(&request.status) {
            return Err(ProductBoardError::UnknownStatus(request.status.to_string()));
        }
        if matches!(request.status, "holds" | "broken") && request.evidence_ref.trim().is_empty() {
            return Err(ProductBoardError::EvidenceRequired(
                request.status.to_string(),
            ));
        }
        check_len("note", request.note, MAX_FIELD_CHARS)?;
        let update_id = self.ids.next("product_condition_update");
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "product_shaping",
                action: ActionKind::RecordProductBetConditionUpdate,
                transition: "RECORD_PRODUCT_BET_CONDITION_UPDATE",
                kind: "product.condition.updated",
            },
            payload(&[
                ("update_id", &update_id),
                ("condition_id", request.condition_id),
                ("status", request.status),
                ("evidence_ref", request.evidence_ref),
                ("note", request.note),
                ("identity_source", &identity.identity_source),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(ProductBoardWriteReport {
            record_id: update_id,
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            terminal_state: "PRODUCT_BET_CONDITION_UPDATED",
        })
    }

    pub fn save_product_bet_resolution_local_with_identity(
        &mut self,
        request: ProductBetResolution<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ProductBoardWriteReport, ProductBoardError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("bet_id", request.bet_id),
            ("outcome", request.outcome),
        ] {
            if value.trim().is_empty() {
                return Err(ProductBoardError::Missing(field));
            }
        }
        if !PRODUCT_RESOLUTION_OUTCOMES.contains(&request.outcome) {
            return Err(ProductBoardError::UnknownOutcome(
                request.outcome.to_string(),
            ));
        }
        if request.outcome == "landed"
            && request.metric_result.trim().is_empty()
            && !request.measurement_unavailable
        {
            return Err(ProductBoardError::MetricRequired);
        }
        check_len("metric_result", request.metric_result, MAX_FIELD_CHARS)?;
        check_len("learned", request.learned, MAX_LONG_CHARS)?;
        let resolution_id = self.ids.next("product_resolution");
        let measurement = if request.measurement_unavailable {
            "true"
        } else {
            "false"
        };
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "product_shaping",
                action: ActionKind::RecordProductBetResolution,
                transition: "RECORD_PRODUCT_BET_RESOLUTION",
                kind: "product.bet.resolved",
            },
            payload(&[
                ("resolution_id", &resolution_id),
                ("bet_id", request.bet_id),
                ("outcome", request.outcome),
                ("metric_result", request.metric_result),
                ("measurement_unavailable", measurement),
                ("learned", request.learned),
                ("identity_source", &identity.identity_source),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(ProductBoardWriteReport {
            record_id: resolution_id,
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            terminal_state: "PRODUCT_BET_RESOLVED",
        })
    }
}
