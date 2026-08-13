use crate::RouteResponse;
use crate::pages_search::percent_decode_query_value;
use mdx_core::{
    GovernedWriteIdentity, MdxKernel, MessageThreadMessage, json_string_literal,
    message_receipt_kind_for_actor_id,
};
use redis::Commands;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

static RELAY_PUBLISHER: OnceLock<Mutex<Option<RelayPublisher>>> = OnceLock::new();

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path == "/messages/thread-messages.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let post = match kernel.write() {
            Ok(mut kernel) => render_local_post_json(body, &mut kernel),
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(post.and_then(|post| {
            // Consolidation runs after the kernel lock drops: a send must
            // never block or fail on the memory stage.
            if let Some(intake) = post.memory_intake {
                crate::memory_extraction::consolidate_surface_event(kernel, intake);
            }
            if let Some(delivery) = post.delivery {
                publish_relay_delivery(delivery)?;
            }
            Ok(RouteResponse::json("200 OK", post.body))
        }));
    }
    if let Some(suffix) = path.strip_prefix("/messages/thread-messages/projection.json") {
        if !suffix.is_empty() && !suffix.starts_with('?') {
            return None;
        }
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let window = ProjectionWindow::from_query(suffix);
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_local_projection_json(&kernel, &window)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    None
}

// Cursor pagination over the saved conversation: optional, so every
// existing caller (the bare path) keeps the whole record exactly as
// before. channel_id narrows to one room, before_sequence walks history
// backward, limit keeps the page bounded - the timeline asks for what it
// can show instead of everything that ever happened.
#[derive(Default)]
struct ProjectionWindow {
    channel_id: String,
    before_sequence: usize,
    limit: usize,
    q: String,
    thread_id: String,
}

impl ProjectionWindow {
    fn from_query(suffix: &str) -> Self {
        let mut window = Self::default();
        let Some(query) = suffix.strip_prefix('?') else {
            return window;
        };
        for pair in query.split('&') {
            if let Some(value) = pair.strip_prefix("channel_id=") {
                window.channel_id = value.to_string();
            } else if let Some(value) = pair.strip_prefix("before_sequence=") {
                window.before_sequence = value.parse().unwrap_or(0);
            } else if let Some(value) = pair.strip_prefix("limit=") {
                window.limit = value.parse().unwrap_or(0);
            } else if let Some(value) = pair.strip_prefix("q=") {
                window.q = percent_decode_query_value(value).to_lowercase();
            } else if let Some(value) = pair.strip_prefix("thread_id=") {
                window.thread_id = percent_decode_query_value(value);
            }
        }
        window
    }
}

fn render_local_post_json(body: &str, kernel: &mut MdxKernel) -> Result<LocalMessagePost, String> {
    let source_receipt_id = match json_string_field(body, "source_receipt_id") {
        Some(value) if !value.trim().is_empty() => value,
        _ => source_receipt_id(kernel)?,
    };
    let thread_id =
        json_string_field(body, "thread_id").unwrap_or_else(|| "thread_local_receipts".to_string());
    let channel_id =
        json_string_field(body, "channel_id").unwrap_or_else(|| "local-ops".to_string());
    let message_id =
        json_string_field(body, "message_id").unwrap_or_else(|| "message_local_ui_001".to_string());
    let body_text = json_string_field(body, "body").unwrap_or_else(|| {
        "Record a governed local Message event without realtime fanout.".to_string()
    });
    let content_type =
        json_string_field(body, "content_type").unwrap_or_else(|| "text".to_string());
    let content_payload =
        json_string_field(body, "content_payload").unwrap_or_else(|| body_text.clone());
    // The verified trusted session is the identity; otherwise the local-demo
    // fixture (kernel-save actor `human:local_user`, response auth `local_user`).
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let verified = resolved.auth_session_status == "VERIFIED_TRUSTED_SESSION";
    let tenant_id = if verified {
        resolved.tenant_id.clone()
    } else {
        json_string_field(body, "tenant_id").unwrap_or_else(|| "local_tenant".to_string())
    };
    let actor_id = if verified {
        resolved.actor_id.clone()
    } else {
        json_string_field(body, "actor_id").unwrap_or_else(|| "human:local_user".to_string())
    };
    let identity = if verified {
        resolved.identity.clone()
    } else {
        GovernedWriteIdentity::local_demo(&actor_id)
    };
    let message_receipt_kind = message_receipt_kind_for_actor_id(&actor_id);
    let outbox_topic =
        json_string_field(body, "outbox_topic").unwrap_or_else(|| message_receipt_kind.to_string());
    // Operator controls, enforced where the write happens: a banned agent
    // post or an oversized body is REFUSED with a plain reason - no
    // record, no relay delivery, never a silent client-side filter.
    let controls = crate::message_controls::MessageControls::load();
    let non_human = ["agent:", "pipeline:", "worker:"]
        .iter()
        .any(|prefix| actor_id.starts_with(prefix));
    let refusal = if non_human && !controls.enabled("agent_posting_allowed") {
        Some("agent posting is off for this workspace - an operator can turn it back on in admin")
    } else if thread_id.starts_with("react:") && !controls.enabled("reactions_enabled") {
        Some("reactions are off for this workspace")
    } else if body_text.chars().count() > controls.max_body_chars() {
        Some("message is over this workspace's size limit")
    } else {
        None
    };
    if let Some(reason) = refusal {
        return Ok(LocalMessagePost {
            body: format!(
                r#"{{"name":"mdx-message-thread-message-local-post","status":"REFUSED","reason":{},"controls_route":"/messages/controls.json","message_receipt_id":"","sequence":0}}"#,
                json_string_literal(reason)
            ),
            delivery: None,
            memory_intake: None,
        });
    }
    let report = kernel
        .save_message_thread_message_local_with_identity(
            MessageThreadMessage {
                tenant_id: &tenant_id,
                actor_id: &actor_id,
                thread_id: &thread_id,
                channel_id: &channel_id,
                message_id: &message_id,
                content_type: &content_type,
                body: &body_text,
                content_payload: &content_payload,
                source_receipt_id: &source_receipt_id,
                outbox_topic: &outbox_topic,
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    // Message bodies are the most sensitive memory class: consolidation
    // happens after the route drops the lock, and extraction may only use a
    // boundary-locked model (verbatim fallback otherwise).
    let memory_intake = Some(crate::memory_extraction::SurfaceMemoryIntake {
        tenant_id: tenant_id.clone(),
        actor_id: actor_id.clone(),
        source_surface: "messages",
        memory_scope: "team_memory",
        source_receipt_id: report.message_receipt_id.clone(),
        content: body_text.clone(),
    });
    let delivery = Some(RelayDelivery {
        tenant_id: tenant_id.clone(),
        actor_id: actor_id.clone(),
        thread_id: report.thread_id.clone(),
        channel_id: report.channel_id.clone(),
        message_id: report.message_id.clone(),
        message_receipt_kind: report.message_receipt_kind.to_string(),
        actor_type: report.actor_type.to_string(),
        content_type: report.content_type.to_string(),
        body: body_text.clone(),
        content_payload: report.content_payload.clone(),
        message_receipt_id: report.message_receipt_id.clone(),
        sequence: report.sequence,
        created_at: report.created_at.clone(),
    });
    let body = format!(
        r#"{{"name":"mdx-message-thread-message-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","auth_session_tenant_id":{},"auth_session_user_id":{},"auth_session_role":{},"writes_route":"/messages/thread-messages.json","projection_route":"/messages/thread-messages/projection.json","thread_id":{},"channel_id":{},"message_id":{},"message_receipt_id":{},"message_receipt_kind":{},"policy_decision_id":{},"actor_type":{},"actor_role":{},"content_type":{},"content_payload":{},"source_receipt_id":{},"source_receipt_kind":{},"trusted_context_citation":{},"trusted_context_source_id":{},"trusted_context_receipt_id":{},"trusted_context_projection_route":{},"outbox_topic":{},"sequence":{},"created_at":{},"llm_allowed_on_hot_path":{},"realtime_fanout_allowed":{},"live_substrate_required":false,"production_write_allowed":false}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&resolved.tenant_id),
        json_string_literal(&resolved.actor_id),
        json_string_literal(&resolved.actor_role),
        json_string_literal(&report.thread_id),
        json_string_literal(&report.channel_id),
        json_string_literal(&report.message_id),
        json_string_literal(&report.message_receipt_id),
        json_string_literal(report.message_receipt_kind),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(report.actor_type),
        json_string_literal(report.actor_role),
        json_string_literal(report.content_type),
        json_string_literal(&report.content_payload),
        json_string_literal(&report.source_receipt_id),
        json_string_literal(&report.source_receipt_kind),
        report.trusted_context_citation,
        json_string_literal(&report.trusted_context_source_id),
        json_string_literal(&report.trusted_context_receipt_id),
        json_string_literal(report.trusted_context_projection_route),
        json_string_literal(&report.outbox_topic),
        report.sequence,
        json_string_literal(&report.created_at),
        report.llm_allowed_on_hot_path,
        report.realtime_fanout_allowed
    );
    Ok(LocalMessagePost {
        body,
        delivery,
        memory_intake,
    })
}

struct LocalMessagePost {
    body: String,
    delivery: Option<RelayDelivery>,
    memory_intake: Option<crate::memory_extraction::SurfaceMemoryIntake>,
}

struct RelayDelivery {
    tenant_id: String,
    actor_id: String,
    thread_id: String,
    channel_id: String,
    message_id: String,
    message_receipt_kind: String,
    actor_type: String,
    content_type: String,
    body: String,
    content_payload: String,
    message_receipt_id: String,
    sequence: usize,
    created_at: String,
}

struct RelayPublisher {
    relay_url: String,
    connection: redis::Connection,
}

impl RelayPublisher {
    fn connect(relay_url: &str) -> redis::RedisResult<Self> {
        let client = redis::Client::open(relay_url)?;
        let connection = client.get_connection()?;
        Ok(Self {
            relay_url: relay_url.to_string(),
            connection,
        })
    }
}

fn publish_relay_delivery(delivery: RelayDelivery) -> Result<(), String> {
    let Some(relay_url) = std::env::var("MDX_MESSAGE_RELAY_URL").ok() else {
        return Ok(());
    };
    let topic = format!("mdx:message:{}:{}", delivery.tenant_id, delivery.channel_id);
    let payload = format!(
        r#"{{"type":"envelope","driver":"valkey","tenant_id":{},"thread_id":{},"channel_id":{},"message_id":{},"message_receipt_id":{},"message_receipt_kind":{},"actor_id":{},"actor_type":{},"content_type":{},"body":{},"content_payload":{},"sequence":{},"created_at":{},"llm_allowed_on_hot_path":false,"websocket_fanout_allowed":true,"production_delivery_allowed":false}}"#,
        json_string_literal(&delivery.tenant_id),
        json_string_literal(&delivery.thread_id),
        json_string_literal(&delivery.channel_id),
        json_string_literal(&delivery.message_id),
        json_string_literal(&delivery.message_receipt_id),
        json_string_literal(&delivery.message_receipt_kind),
        json_string_literal(&delivery.actor_id),
        json_string_literal(&delivery.actor_type),
        json_string_literal(&delivery.content_type),
        json_string_literal(&delivery.body),
        json_string_literal(&delivery.content_payload),
        delivery.sequence,
        json_string_literal(&delivery.created_at)
    );
    let publish_result = publish_with_cached_connection(&relay_url, &topic, &payload);
    match publish_result {
        Ok(_) => Ok(()),
        Err(error) if std::env::var("MDX_MESSAGE_RELAY_STRICT").ok().as_deref() == Some("1") => {
            Err(format!("message relay publish failed: {error}"))
        }
        Err(error) => {
            eprintln!("message relay publish skipped: {error}");
            Ok(())
        }
    }
}

fn publish_with_cached_connection(
    relay_url: &str,
    topic: &str,
    payload: &str,
) -> redis::RedisResult<usize> {
    let cache = RELAY_PUBLISHER.get_or_init(|| Mutex::new(None));
    let mut publisher = cache.lock().map_err(|_| {
        redis::RedisError::from((
            redis::ErrorKind::IoError,
            "message relay publisher cache poisoned",
        ))
    })?;

    let should_connect = publisher
        .as_ref()
        .map(|existing| existing.relay_url.as_str() != relay_url)
        .unwrap_or(true);
    if should_connect {
        *publisher = Some(RelayPublisher::connect(relay_url)?);
    }

    let Some(active_publisher) = publisher.as_mut() else {
        return Err(redis::RedisError::from((
            redis::ErrorKind::IoError,
            "message relay publisher cache missing after connect",
        )));
    };
    let publish_result = active_publisher.connection.publish(topic, payload);
    if publish_result.is_ok() {
        return publish_result;
    }

    *publisher = Some(RelayPublisher::connect(relay_url)?);
    let Some(active_publisher) = publisher.as_mut() else {
        return Err(redis::RedisError::from((
            redis::ErrorKind::IoError,
            "message relay publisher cache missing after reconnect",
        )));
    };
    active_publisher.connection.publish(topic, payload)
}

fn render_local_projection_json(
    kernel: &MdxKernel,
    window: &ProjectionWindow,
) -> Result<String, String> {
    // Fail closed on private channels: messages in a channel the local reader
    // is not a member of are never returned. The reader on the local serving
    // path is the deterministic local session; the Postgres path adds the
    // resource-aware RLS policy the access-control matrix declares.
    let hidden = kernel.message_unreadable_channels_for("local_tenant", "local_user");
    let matching: Vec<_> = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| mdx_core::is_message_thread_message_receipt_kind(&receipt.kind))
        .filter(|receipt| !hidden.contains(payload_value(receipt, "channel_id")))
        .filter(|receipt| {
            window.channel_id.is_empty()
                || payload_value(receipt, "channel_id") == window.channel_id
        })
        .filter(|receipt| {
            window.thread_id.is_empty() || payload_value(receipt, "thread_id") == window.thread_id
        })
        .filter(|receipt| {
            window.q.is_empty()
                || payload_value(receipt, "body")
                    .to_lowercase()
                    .contains(&window.q)
        })
        .filter(|receipt| {
            window.before_sequence == 0
                || payload_value(receipt, "sequence")
                    .parse::<usize>()
                    .map(|sequence| sequence < window.before_sequence)
                    .unwrap_or(false)
        })
        .collect();
    let total_matching = matching.len();
    let skip = if window.limit > 0 {
        total_matching.saturating_sub(window.limit)
    } else {
        0
    };
    let has_older = skip > 0;
    let messages = matching
        .into_iter()
        .skip(skip)
        .map(|receipt| {
            format!(
                r#"{{"thread_id":{},"channel_id":{},"message_id":{},"message_receipt_id":{},"message_receipt_kind":{},"policy_decision_id":{},"actor_id":{},"actor_type":{},"actor_display_name":{},"actor_role":{},"content_type":{},"body":{},"content_payload":{},"source_receipt_id":{},"source_receipt_kind":{},"trusted_context_citation":{},"trusted_context_source_id":{},"trusted_context_receipt_id":{},"trusted_context_projection_route":{},"outbox_topic":{},"sequence":{},"created_at":{},"llm_allowed_on_hot_path":false,"realtime_fanout_allowed":false,"production_write_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "thread_id")),
                json_string_literal(payload_value(receipt, "channel_id")),
                json_string_literal(payload_value(receipt, "message_id")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(&receipt.kind),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(receipt.actor_id.as_str()),
                json_string_literal(payload_value(receipt, "actor_type")),
                json_string_literal(payload_value(receipt, "actor_display_name")),
                json_string_literal(payload_value(receipt, "actor_role")),
                json_string_literal(payload_value(receipt, "content_type")),
                json_string_literal(payload_value(receipt, "body")),
                json_string_literal(payload_value(receipt, "content_payload")),
                json_string_literal(payload_value(receipt, "source_receipt_id")),
                json_string_literal(payload_value(receipt, "source_receipt_kind")),
                payload_value(receipt, "trusted_context_citation") == "true",
                json_string_literal(payload_value(receipt, "trusted_context_source_id")),
                json_string_literal(payload_value(receipt, "trusted_context_receipt_id")),
                json_string_literal(payload_value(receipt, "trusted_context_projection_route")),
                json_string_literal(payload_value(receipt, "outbox_topic")),
                payload_value(receipt, "sequence"),
                json_string_literal(&receipt.receipt_timestamp)
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":"mdx-message-thread-message-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/messages/thread-messages.json","message_count":{},"window":{{"channel_id":{},"before_sequence":{},"limit":{},"returned":{},"has_older":{}}},"llm_allowed_on_hot_path":false,"realtime_fanout_allowed":false,"live_substrate_required":false,"production_write_allowed":false,"messages":[{}]}}"#,
        total_matching,
        json_string_literal(&window.channel_id),
        window.before_sequence,
        window.limit,
        messages.len(),
        has_older,
        messages.join(",")
    ))
}

fn source_receipt_id(kernel: &mut MdxKernel) -> Result<String, String> {
    if let Some(receipt_id) = kernel
        .ledger()
        .entries()
        .first()
        .map(|receipt| receipt.receipt_id.clone())
    {
        return Ok(receipt_id);
    }
    kernel.run_evals_runner_agent()?;
    kernel
        .ledger()
        .entries()
        .first()
        .map(|receipt| receipt.receipt_id.clone())
        .ok_or_else(|| "local Message write route missing source receipt".to_string())
}

fn payload_value<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn json_string_field(body: &str, key: &str) -> Option<String> {
    let after_key = body.split(&format!("\"{key}\"")).nth(1)?;
    let after_colon = after_key.split_once(':')?.1.trim_start();
    let value = after_colon.strip_prefix('"')?;
    let mut out = String::new();
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            out.push(match character {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return Some(out);
        } else {
            out.push(character);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::{
        PagesContextSourceTrustDecision, TwinArtifactContextRequest, TwinArtifactDeletionRequest,
    };

    #[test]
    fn message_thread_message_route_cites_only_active_trusted_context() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let (source_id, trust_receipt_id) = {
            let mut writable = kernel.write().expect("kernel");
            let source = writable
                .save_twin_artifact_context_local(TwinArtifactContextRequest {
                    tenant_id: "local_tenant",
                    actor_id: "human:local_user",
                    session_id: "message_route_context_source_session",
                    artifact_name: "message-route-context.md",
                    mime_type: "text/markdown",
                    artifact_text: "Message route citations should use active trusted Context.",
                    privacy_class: "user_private_sensitive",
                    company_context: "Pages says Message citations must be receipt backed.",
                    company_context_source: "Pages:Message route citation standard",
                })
                .expect("source");
            let trust = writable
                .save_pages_context_source_trust_decision_local(PagesContextSourceTrustDecision {
                    tenant_id: "local_tenant",
                    actor_id: "human:local_user",
                    trust_decision_id: "message_route_context_trust",
                    source_id: &source.artifact_id,
                    decision_outcome: "trusted_for_ai",
                    decision_note: "Approved for Message route citation.",
                    source_route: "/pages/context-sources/trust-decisions.json",
                })
                .expect("trust");
            (source.artifact_id, trust.trust_decision_receipt_id)
        };

        let response = route_response(
            "POST",
            "/messages/thread-messages.json",
            &format!(
                r#"{{"message_id":"message_context_route","body":"Sharing trusted Context.","source_receipt_id":"{trust_receipt_id}"}}"#
            ),
            &kernel,
        )
        .expect("route")
        .expect("response");
        assert_eq!(response.status, "200 OK");
        for expected in [
            "\"trusted_context_citation\":true",
            "\"trusted_context_projection_route\":\"/pages/context-sources/projection.json\"",
            &format!(r#""trusted_context_source_id":"{source_id}""#),
            &format!(r#""trusted_context_receipt_id":"{trust_receipt_id}""#),
        ] {
            assert!(
                response.body.contains(expected),
                "{expected}: {}",
                response.body
            );
        }

        {
            let mut writable = kernel.write().expect("kernel");
            writable
                .delete_twin_artifact_local(TwinArtifactDeletionRequest {
                    tenant_id: "local_tenant",
                    actor_id: "human:local_user",
                    artifact_id: &source_id,
                    reason: "Remove trusted Context before another Message route citation.",
                    local_blob_bytes_deleted: false,
                })
                .expect("delete source");
        }
        let error = route_response(
            "POST",
            "/messages/thread-messages.json",
            &format!(
                r#"{{"message_id":"message_context_route_deleted","body":"Should fail closed.","source_receipt_id":"{trust_receipt_id}"}}"#
            ),
            &kernel,
        )
        .expect("route")
        .expect_err("deleted source should fail closed");
        assert!(error.contains("trusted Context source receipt"));
    }
}
