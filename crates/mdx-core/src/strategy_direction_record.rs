//! "Our direction": the standing strategy record. One page, five boxes,
//! always current - and diffable, because every version is a receipt.
//! A new version says which boxes changed and which proposal drove the
//! change, so the company's strategy has provenance the way its code
//! does. Recording a version opens no authority; it is the company
//! saying out loud what it currently believes.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

const MAX_BOX_CHARS: usize = 1200;

#[derive(Clone, Copy, Debug)]
pub struct StrategyDirectionRecord<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    /// What winning looks like - with someone specific, never as activity.
    pub winning: &'a str,
    /// Customers, offer, channel, geography - and the explicit not-list.
    pub where_we_play: &'a str,
    /// The edge a sane rival could choose the opposite of.
    pub how_we_win: &'a str,
    /// The 3-5 things the company must be great at.
    pub great_at: &'a str,
    /// What the company is counting on - the standing conditions.
    pub counting_on: &'a str,
    /// The ratified proposal that drove this version, when one did.
    pub amended_by: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrategyDirectionRecordReport {
    pub version: u64,
    pub record_receipt_id: String,
    pub policy_decision_id: String,
    pub terminal_state: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StrategyDirectionRecordError {
    Missing(&'static str),
    TooLong(&'static str, usize),
}

impl StrategyDirectionRecordError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("our-direction record missing {field}"),
            Self::TooLong(field, len) => format!(
                "our-direction {field} is {len} characters; the limit is {MAX_BOX_CHARS} - \
                 the whole record fits one page, that is the point"
            ),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_strategy_direction_record_local_with_identity(
        &mut self,
        request: StrategyDirectionRecord<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<StrategyDirectionRecordReport, StrategyDirectionRecordError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("winning", request.winning),
        ] {
            if value.trim().is_empty() {
                return Err(StrategyDirectionRecordError::Missing(field));
            }
        }
        for (field, value) in [
            ("winning", request.winning),
            ("where_we_play", request.where_we_play),
            ("how_we_win", request.how_we_win),
            ("great_at", request.great_at),
            ("counting_on", request.counting_on),
        ] {
            let len = value.chars().count();
            if len > MAX_BOX_CHARS {
                return Err(StrategyDirectionRecordError::TooLong(field, len));
            }
        }
        let version = (self
            .ledger()
            .query()
            .by_kind("strategy.direction.recorded")
            .len() as u64)
            + 1;
        let version_text = version.to_string();
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "strategy_ratification",
                action: ActionKind::RecordStrategyDirectionRecord,
                transition: "RECORD_STRATEGY_DIRECTION_RECORD",
                kind: "strategy.direction.recorded",
            },
            payload(&[
                ("version", &version_text),
                ("winning", request.winning),
                ("where_we_play", request.where_we_play),
                ("how_we_win", request.how_we_win),
                ("great_at", request.great_at),
                ("counting_on", request.counting_on),
                ("amended_by", request.amended_by),
                ("identity_source", &identity.identity_source),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(StrategyDirectionRecordReport {
            version,
            record_receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            terminal_state: "STRATEGY_DIRECTION_RECORDED_VERSIONED",
        })
    }
}
