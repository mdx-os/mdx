//! The Message bridge to outside chat systems (Slack, Teams). Inbound is the
//! connector rail: a Slack or Teams message ingested as an external item
//! (origin=external, sovereign-gated) through `record_external_item`. Outbound
//! is this module: queuing an MDx message to be posted into a Slack/Teams
//! channel. It is governed and recorded, but it does NOT deliver - live
//! transport sits behind the suspend-for-external-call gate, exactly like the
//! connector rail's live fetch. This slice governs the SHAPE of a bidirectional
//! bridge (what an outbound post is, who it targets, the residency line), not
//! the wire.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

/// The outside systems the bridge speaks to. A closed set keeps the bridge
/// legible and refuses anything it has not been taught.
pub const MESSAGE_BRIDGE_KINDS: &[&str] = &["slack", "teams"];

pub const MESSAGE_BRIDGE_POST_KIND: &str = "message.bridge.post.queued";

const MAX_BRIDGE_BODY_CHARS: usize = 4000;

#[derive(Clone, Copy, Debug)]
pub struct BridgePost<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    /// One of MESSAGE_BRIDGE_KINDS.
    pub target_kind: &'a str,
    /// The outside channel id/name, e.g. "slack:C123" or "teams:general".
    pub target_channel: &'a str,
    pub body: &'a str,
    /// The MDx message this echoes outward, if any - provenance back into the
    /// ledger. Empty for a fresh outbound post.
    pub source_message_receipt_id: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BridgePostReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub target_kind: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BridgeError {
    Missing(&'static str),
    TooLong(usize, usize),
    UnknownKind(String),
}

impl BridgeError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("queue a bridge post: missing {field}"),
            Self::TooLong(len, max) => {
                format!("bridge post body is {len} characters; the limit is {max}")
            }
            Self::UnknownKind(other) => format!(
                "bridge target must be one of {}; got {other}",
                MESSAGE_BRIDGE_KINDS.join(", ")
            ),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn record_message_bridge_post_with_identity(
        &mut self,
        request: BridgePost<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<BridgePostReport, BridgeError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("target_channel", request.target_channel),
            ("body", request.body),
        ] {
            if value.trim().is_empty() {
                return Err(BridgeError::Missing(field));
            }
        }
        if request.body.chars().count() > MAX_BRIDGE_BODY_CHARS {
            return Err(BridgeError::TooLong(
                request.body.chars().count(),
                MAX_BRIDGE_BODY_CHARS,
            ));
        }
        let target_kind = request.target_kind.trim().to_ascii_lowercase();
        let target_kind = MESSAGE_BRIDGE_KINDS
            .iter()
            .copied()
            .find(|kind| *kind == target_kind)
            .ok_or_else(|| BridgeError::UnknownKind(target_kind.clone()))?;
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "message_bridge",
                action: ActionKind::RecordMessageBridgePost,
                transition: "QUEUE_MESSAGE_BRIDGE_POST",
                kind: MESSAGE_BRIDGE_POST_KIND,
            },
            payload(&[
                ("target_kind", target_kind),
                ("target_channel", request.target_channel.trim()),
                ("body", request.body.trim()),
                (
                    "source_message_receipt_id",
                    request.source_message_receipt_id,
                ),
                ("origin", "outbound"),
                ("identity_source", &identity.identity_source),
                // Recorded, not delivered: live transport sits behind the
                // suspend-for-external-call gate.
                ("production_delivery_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(BridgePostReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            target_kind,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_outbound_post_is_recorded_but_not_delivered_and_unknown_targets_refuse() {
        let mut kernel = MdxKernel::boot_local();
        let identity = GovernedWriteIdentity::local_demo("human:md");
        let report = kernel
            .record_message_bridge_post_with_identity(
                BridgePost {
                    tenant_id: "local_tenant",
                    actor_id: "human:md",
                    target_kind: "slack",
                    target_channel: "slack:C123",
                    body: "shipping the channels arc",
                    source_message_receipt_id: "",
                },
                &identity,
            )
            .expect("queue outbound");
        assert_eq!(report.target_kind, "slack");
        let receipt = kernel
            .ledger()
            .query()
            .by_kind(MESSAGE_BRIDGE_POST_KIND)
            .into_iter()
            .next()
            .expect("bridge receipt");
        assert_eq!(receipt.payload["production_delivery_allowed"], "false");
        assert_eq!(receipt.payload["target_kind"], "slack");

        let refused = kernel.record_message_bridge_post_with_identity(
            BridgePost {
                tenant_id: "local_tenant",
                actor_id: "human:md",
                target_kind: "carrier_pigeon",
                target_channel: "x",
                body: "x",
                source_message_receipt_id: "",
            },
            &identity,
        );
        assert!(matches!(refused, Err(BridgeError::UnknownKind(_))));
    }
}
