//! The Forge review panel, recorded. A diverse set of models read a finished
//! run's change cold, each from a distinct lens, and the room records what
//! they agreed on, what they DIDN'T, and the concerns only one of them caught.
//! The defining choice (from the Fusion shape, MDx-native): dissent is a
//! first-class artifact, not collapsed into a single verdict. The kernel
//! records the structured outcome; the server makes the model calls and hands
//! the result here. Recording opens no authority - it is the panel's judgment
//! made durable, never a ship.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

/// One reviewer's read: which model, what lens, its verdict, and the concern
/// it raised in its own words (capped).
#[derive(Clone, Debug)]
pub struct PanelMember {
    pub model: String,
    pub stance: String,
    pub verdict: String,
    pub concern: String,
}

/// The structured panel outcome the server assembled from the reviewers.
pub struct ForgeReviewPanel<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub run_id: &'a str,
    /// ready | needs_work | inconclusive - conservative: any "needs work" holds.
    pub consensus: &'a str,
    /// high (unanimous) | medium (split) | low (reviewers failed).
    pub confidence: &'a str,
    /// The residency class of the reviewer models, for transparency.
    pub residency: &'a str,
    pub members: &'a [PanelMember],
    /// Models whose verdict differed from the consensus - the minority view.
    pub dissent: &'a [String],
    /// Concerns only one lens caught - the derived blind spots.
    pub blind_spots: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeReviewPanelReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForgeReviewPanelError {
    Missing(&'static str),
    NoReviewers,
}

impl ForgeReviewPanelError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("record a review panel: missing {field}"),
            Self::NoReviewers => {
                "the panel recorded no reviewers - configure at least one reviewer model"
                    .to_string()
            }
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn record_forge_review_panel(
        &mut self,
        request: ForgeReviewPanel<'_>,
    ) -> Result<ForgeReviewPanelReport, ForgeReviewPanelError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.record_forge_review_panel_with_identity(request, &identity)
    }

    pub fn record_forge_review_panel_with_identity(
        &mut self,
        request: ForgeReviewPanel<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ForgeReviewPanelReport, ForgeReviewPanelError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("run_id", request.run_id),
        ] {
            if value.trim().is_empty() {
                return Err(ForgeReviewPanelError::Missing(field));
            }
        }
        if request.members.is_empty() {
            return Err(ForgeReviewPanelError::NoReviewers);
        }
        // Members serialize as member_<i>_<field>, so the projection can read
        // each reviewer's lens and verdict back without a side store.
        let mut fields: Vec<(String, String)> = vec![
            ("run_id".to_string(), request.run_id.to_string()),
            ("consensus".to_string(), request.consensus.to_string()),
            ("confidence".to_string(), request.confidence.to_string()),
            ("residency".to_string(), request.residency.to_string()),
            (
                "member_count".to_string(),
                request.members.len().to_string(),
            ),
            ("dissent".to_string(), request.dissent.join(", ")),
            (
                "dissent_count".to_string(),
                request.dissent.len().to_string(),
            ),
            ("blind_spots".to_string(), request.blind_spots.to_string()),
            (
                "identity_source".to_string(),
                identity.identity_source.clone(),
            ),
            ("authority_opened".to_string(), "none".to_string()),
            ("production_write_allowed".to_string(), "false".to_string()),
        ];
        for (index, member) in request.members.iter().enumerate() {
            fields.push((format!("member_{index}_model"), member.model.clone()));
            fields.push((format!("member_{index}_stance"), member.stance.clone()));
            fields.push((format!("member_{index}_verdict"), member.verdict.clone()));
            fields.push((
                format!("member_{index}_concern"),
                member.concern.chars().take(400).collect(),
            ));
        }
        let pairs: Vec<(&str, &str)> = fields
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "forge_review_panel",
                action: ActionKind::RecordForgeReviewPanel,
                transition: "RECORD_FORGE_REVIEW_PANEL",
                kind: "forge.review.panel.recorded",
            },
            payload(&pairs),
        );
        Ok(ForgeReviewPanelReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panel_records_its_members_consensus_and_dissent() {
        let mut kernel = MdxKernel::boot_local();
        let members = vec![
            PanelMember {
                model: "grok-build-0.1".to_string(),
                stance: "HOLISTIC".to_string(),
                verdict: "ready".to_string(),
                concern: String::new(),
            },
            PanelMember {
                model: "claude-opus".to_string(),
                stance: "ADVERSARIAL".to_string(),
                verdict: "needs work".to_string(),
                concern: "the error path is never tested".to_string(),
            },
        ];
        let report = kernel
            .record_forge_review_panel(ForgeReviewPanel {
                tenant_id: "t",
                actor_id: "human:md",
                run_id: "forge_run_1",
                consensus: "needs_work",
                confidence: "medium",
                residency: "frontier",
                members: &members,
                dissent: &["grok-build-0.1".to_string()],
                blind_spots: "the error path is never tested",
            })
            .expect("record");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .expect("panel receipt");
        assert_eq!(receipt.kind, "forge.review.panel.recorded");
        assert_eq!(receipt.payload["consensus"], "needs_work");
        assert_eq!(receipt.payload["member_count"], "2");
        assert_eq!(receipt.payload["member_1_verdict"], "needs work");

        // No reviewers is refused - a panel with no eyes is not a panel.
        let empty = kernel.record_forge_review_panel(ForgeReviewPanel {
            tenant_id: "t",
            actor_id: "human:md",
            run_id: "forge_run_1",
            consensus: "inconclusive",
            confidence: "low",
            residency: "frontier",
            members: &[],
            dissent: &[],
            blind_spots: "",
        });
        assert!(matches!(empty, Err(ForgeReviewPanelError::NoReviewers)));
    }
}
