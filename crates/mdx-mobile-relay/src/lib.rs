use redis::Commands;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::sync::OnceLock;
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const MAX_SUMMARY_CHARS: usize = 500;
const DEFAULT_QUEUE_CAPACITY: usize = 512;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionTarget {
    pub kind: String,
    pub target_id: String,
    pub tenant_id: String,
    pub display_name: String,
    pub capability_revision: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MobileRelayEnvelope {
    pub schema_version: u32,
    pub event_id: String,
    pub session_id: String,
    pub session_version: u64,
    pub sequence: u64,
    pub tenant_id: String,
    pub target: ExecutionTarget,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_id: Option<String>,
    pub receipt_id: Option<String>,
    pub safe_summary: String,
    pub evidence_refs: Vec<String>,
    pub occurred_at: String,
    pub redaction_status: String,
    pub contains_secret_values: bool,
    pub grants_authority: bool,
}

impl MobileRelayEnvelope {
    #[allow(clippy::too_many_arguments)]
    pub fn forge_event(
        tenant_id: &str,
        session_id: &str,
        sequence: u64,
        kind: &str,
        stage: &str,
        terminal_status: Option<&str>,
        summary: &str,
        receipt_id: &str,
        execution_target_kind: &str,
        execution_target_id: &str,
    ) -> Option<Self> {
        if !valid_identifier(tenant_id) || !valid_identifier(session_id) || sequence == 0 {
            return None;
        }
        let compact_summary = summary.split_whitespace().collect::<Vec<_>>().join(" ");
        let safe_summary = sanitize_summary(summary);
        let redaction_status = if safe_summary == compact_summary {
            "not_required"
        } else {
            "redacted"
        };
        let target_kind = canonical_target_kind(execution_target_kind);
        let target_id = if valid_identifier(execution_target_id) {
            execution_target_id.to_string()
        } else {
            "local_host".to_string()
        };
        let receipt_id = valid_identifier(receipt_id).then(|| receipt_id.to_string());
        Some(Self {
            schema_version: 1,
            event_id: format!("{session_id}:{sequence}"),
            session_id: session_id.to_string(),
            session_version: sequence,
            sequence,
            tenant_id: tenant_id.to_string(),
            target: ExecutionTarget {
                display_name: target_display_name(target_kind).to_string(),
                kind: target_kind.to_string(),
                target_id,
                tenant_id: tenant_id.to_string(),
                capability_revision: 1,
            },
            kind: canonical_event_kind(kind, stage, terminal_status).to_string(),
            command_id: None,
            evidence_refs: receipt_id.iter().cloned().collect(),
            receipt_id,
            safe_summary: if safe_summary.is_empty() {
                "Forge reported progress".to_string()
            } else {
                safe_summary
            },
            occurred_at: OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string()),
            redaction_status: redaction_status.to_string(),
            contains_secret_values: false,
            grants_authority: false,
        })
    }

    pub fn topic(&self) -> String {
        format!("mdx:forge:{}:{}", self.tenant_id, self.session_id)
    }

    pub fn is_contract_valid(&self) -> bool {
        self.schema_version == 1
            && valid_identifier(&self.session_id)
            && self.event_id == format!("{}:{}", self.session_id, self.sequence)
            && self.session_version == self.sequence
            && self.sequence > 0
            && valid_identifier(&self.tenant_id)
            && self.target.tenant_id == self.tenant_id
            && matches!(
                self.target.kind.as_str(),
                "paired_host" | "mdx_cloud" | "customer_managed"
            )
            && valid_identifier(&self.target.target_id)
            && !self.target.display_name.trim().is_empty()
            && self.target.display_name.chars().count() <= 120
            && self.target.capability_revision > 0
            && matches!(
                self.kind.as_str(),
                "accepted"
                    | "started"
                    | "progress"
                    | "needs_user"
                    | "checkpointed"
                    | "handed_off"
                    | "review_ready"
                    | "completed"
                    | "failed"
                    | "stopped"
                    | "refused"
            )
            && self.command_id.as_deref().is_none_or(valid_identifier)
            && self.receipt_id.as_deref().is_none_or(valid_identifier)
            && !self.safe_summary.trim().is_empty()
            && self.safe_summary.chars().count() <= MAX_SUMMARY_CHARS
            && sanitize_summary(&self.safe_summary) == self.safe_summary
            && self
                .evidence_refs
                .iter()
                .all(|value| valid_identifier(value))
            && self.evidence_refs.iter().collect::<BTreeSet<_>>().len() == self.evidence_refs.len()
            && OffsetDateTime::parse(&self.occurred_at, &Rfc3339).is_ok()
            && matches!(
                self.redaction_status.as_str(),
                "not_required" | "redacted" | "withheld"
            )
            && !self.contains_secret_values
            && !self.grants_authority
    }
}

pub fn sanitize_summary(value: &str) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut safe = compact;
    for marker in [
        "api_key=",
        "apikey=",
        "authorization:",
        "bearer ",
        "password=",
        "secret=",
        "token=",
    ] {
        safe = redact_assignment(&safe, marker);
    }
    safe = redact_known_token_shapes(&safe);
    if safe.contains("-----BEGIN") {
        safe = "Sensitive credential material was suppressed".to_string();
    }
    safe.chars().take(MAX_SUMMARY_CHARS).collect()
}

fn redact_known_token_shapes(value: &str) -> String {
    value
        .split_whitespace()
        .map(|token| {
            let normalized = token
                .trim_matches(|character: char| {
                    matches!(
                        character,
                        '"' | '\'' | '`' | '(' | ')' | '[' | ']' | ',' | ';'
                    )
                })
                .to_ascii_lowercase();
            let sensitive = [
                "ghp_",
                "github_pat_",
                "sk-proj-",
                "sk-ant-",
                "xoxb-",
                "xoxp-",
            ]
            .iter()
            .any(|prefix| normalized.starts_with(prefix))
                || (normalized.starts_with("akia") && normalized.len() >= 20);
            if sensitive { "[REDACTED]" } else { token }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn redact_assignment(value: &str, marker: &str) -> String {
    let mut output = value.to_string();
    let mut search_from = 0;
    loop {
        let lower = output.to_ascii_lowercase();
        let Some(relative_start) = lower[search_from..].find(marker) else {
            break;
        };
        let value_start = search_from + relative_start + marker.len();
        let end = output[value_start..]
            .find(char::is_whitespace)
            .map(|offset| value_start + offset)
            .unwrap_or(output.len());
        output.replace_range(value_start..end, "[REDACTED]");
        search_from = value_start + "[REDACTED]".len();
        if search_from >= output.len() {
            break;
        }
    }
    output
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn canonical_target_kind(value: &str) -> &'static str {
    match value {
        "paired_host" => "paired_host",
        "mdx_cloud" => "mdx_cloud",
        "customer_managed" => "customer_managed",
        _ => "paired_host",
    }
}

fn target_display_name(kind: &str) -> &'static str {
    match kind {
        "mdx_cloud" => "MDx Cloud",
        "customer_managed" => "Private Cloud",
        _ => "Paired Mac",
    }
}

fn canonical_event_kind(kind: &str, stage: &str, terminal_status: Option<&str>) -> &'static str {
    match kind {
        "terminal" => terminal_event_kind(terminal_status),
        "diff_ready" => "review_ready",
        "refused" | "error" => "refused",
        "stopped" => "stopped",
        "stage_start" if stage == "reading" => "started",
        "evidence" if stage == "done" => "checkpointed",
        _ => "progress",
    }
}

pub fn terminal_event_kind(terminal_status: Option<&str>) -> &'static str {
    match terminal_status {
        Some("RUN_FINISHED_DONE" | "RUN_FINISHED_NO_CHANGE") => "completed",
        Some("RUN_STOPPED" | "RUN_STOPPED_BY_OPERATOR") => "stopped",
        _ => "failed",
    }
}

#[derive(Clone)]
struct PublishCommand {
    topic: String,
    payload: String,
}

struct Publisher {
    sender: Option<SyncSender<PublishCommand>>,
}

impl Publisher {
    fn from_environment() -> Self {
        let enabled = std::env::var("MDX_MOBILE_RELAY_PUBLISH")
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);
        if !enabled {
            return Self { sender: None };
        }
        let relay_url = std::env::var("MDX_MOBILE_RELAY_URL")
            .or_else(|_| std::env::var("REDIS_URL"))
            .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let capacity = std::env::var("MDX_MOBILE_RELAY_QUEUE_CAPACITY")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|value| *value > 0 && *value <= 16_384)
            .unwrap_or(DEFAULT_QUEUE_CAPACITY);
        let (sender, receiver) = sync_channel::<PublishCommand>(capacity);
        std::thread::Builder::new()
            .name("mdx-mobile-relay-publisher".to_string())
            .spawn(move || {
                let Ok(client) = redis::Client::open(relay_url) else {
                    return;
                };
                let Ok(mut connection) = client.get_connection() else {
                    return;
                };
                while let Ok(command) = receiver.recv() {
                    let _: redis::RedisResult<usize> =
                        connection.publish(command.topic, command.payload);
                }
            })
            .ok();
        Self {
            sender: Some(sender),
        }
    }

    fn publish(&self, envelope: &MobileRelayEnvelope) -> bool {
        let Some(sender) = &self.sender else {
            return false;
        };
        if !envelope.is_contract_valid() {
            return false;
        }
        let Ok(payload) = serde_json::to_string(envelope) else {
            return false;
        };
        match sender.try_send(PublishCommand {
            topic: envelope.topic(),
            payload,
        }) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => false,
        }
    }
}

static PUBLISHER: OnceLock<Publisher> = OnceLock::new();

pub fn publish(envelope: &MobileRelayEnvelope) -> bool {
    PUBLISHER
        .get_or_init(Publisher::from_environment)
        .publish(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_never_carries_raw_secret_values() {
        let envelope = MobileRelayEnvelope::forge_event(
            "tenant_one",
            "forge_run_one",
            3,
            "tool_result",
            "proof",
            None,
            "run token=abc123 Authorization: Bearer xyz password=hunter2 ghp_123456789 done",
            "receipt_one",
            "paired_host",
            "host_one",
        )
        .expect("valid envelope");
        let wire = serde_json::to_string(&envelope).expect("serialize");
        assert!(!wire.contains("abc123"));
        assert!(!wire.contains("hunter2"));
        assert!(!wire.contains("Bearer xyz"));
        assert!(!wire.contains("ghp_123456789"));
        assert!(wire.contains("[REDACTED]"));
        assert!(wire.contains("\"contains_secret_values\":false"));
        assert!(wire.contains("\"grants_authority\":false"));
        assert!(!wire.contains("production_write_allowed"));
    }

    #[test]
    fn envelope_is_bounded_and_topic_is_tenant_scoped() {
        let envelope = MobileRelayEnvelope::forge_event(
            "tenant_one",
            "forge_run_one",
            1,
            "event",
            "reading",
            None,
            &"x".repeat(4_000),
            "",
            "paired_host",
            "host_one",
        )
        .expect("valid envelope");
        assert_eq!(envelope.safe_summary.chars().count(), MAX_SUMMARY_CHARS);
        assert_eq!(envelope.topic(), "mdx:forge:tenant_one:forge_run_one");
        assert_eq!(envelope.kind, "progress");
    }

    #[test]
    fn invalid_routing_identifiers_fail_closed() {
        assert!(
            MobileRelayEnvelope::forge_event(
                "tenant:escape",
                "forge_run_one",
                1,
                "event",
                "reading",
                None,
                "safe",
                "",
                "paired_host",
                "host_one",
            )
            .is_none()
        );
    }

    #[test]
    fn wire_event_validates_against_the_pinned_session_event_schema() {
        let envelope = MobileRelayEnvelope::forge_event(
            "tenant_one",
            "forge_run_one",
            7,
            "diff_ready",
            "done",
            None,
            "The focused diff is ready",
            "receipt_7",
            "paired_host",
            "host_one",
        )
        .expect("valid envelope");
        let mut schema: serde_json::Value = serde_json::from_str(include_str!(
            "../../../generated/response-schemas/session-event.schema.json"
        ))
        .expect("session event schema");
        let target_schema: serde_json::Value = serde_json::from_str(include_str!(
            "../../../generated/response-schemas/execution-target.schema.json"
        ))
        .expect("execution target schema");
        schema["properties"]["target"] = target_schema;
        let validator = jsonschema::validator_for(&schema).expect("compiled session event schema");
        let value = serde_json::to_value(&envelope).expect("event JSON");
        assert!(
            validator.is_valid(&value),
            "canonical mobile event did not match session-event.schema.json: {value}"
        );
        assert!(envelope.is_contract_valid());
    }

    #[test]
    fn terminal_event_kind_is_shared_and_fails_closed() {
        assert_eq!(terminal_event_kind(Some("RUN_FINISHED_DONE")), "completed");
        assert_eq!(
            terminal_event_kind(Some("RUN_FINISHED_NO_CHANGE")),
            "completed"
        );
        assert_eq!(terminal_event_kind(Some("RUN_STOPPED")), "stopped");
        assert_eq!(
            terminal_event_kind(Some("RUN_STOPPED_BY_OPERATOR")),
            "stopped"
        );
        assert_eq!(terminal_event_kind(Some("RUN_BUDGET_EXHAUSTED")), "failed");
        assert_eq!(
            terminal_event_kind(Some("RUN_FINISHED_CANNOT_PROCEED")),
            "failed"
        );
        assert_eq!(terminal_event_kind(Some("RUN_FINISHED_UNKNOWN")), "failed");
        assert_eq!(terminal_event_kind(None), "failed");
    }

    #[test]
    fn contract_validation_rejects_cross_tenant_or_unknown_events() {
        let mut envelope = MobileRelayEnvelope::forge_event(
            "tenant_one",
            "forge_run_one",
            7,
            "terminal",
            "done",
            Some("RUN_FINISHED_DONE"),
            "Finished with RUN_FINISHED_DONE",
            "receipt_7",
            "paired_host",
            "host_one",
        )
        .expect("valid envelope");
        assert!(envelope.is_contract_valid());
        envelope.target.tenant_id = "tenant_other".to_string();
        assert!(!envelope.is_contract_valid());
        envelope.target.tenant_id = "tenant_one".to_string();
        envelope.kind = "terminal".to_string();
        assert!(!envelope.is_contract_valid());
    }
}
