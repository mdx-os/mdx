use mdx_core::json_string_literal;
use serde_json::Value;

const LOCAL_TENANT: &str = "tenant_local";
const LOCAL_ACTOR: &str = "local_user";
const DEFAULT_MAX_LOCAL_MEMORY_COUNT: usize = 10_000;
const DEFAULT_QUEUE_DEPTH_LIMIT: usize = 50_000;
const DEFAULT_CONTENT_TOKEN_LIMIT: usize = 32_000;

#[derive(Default)]
pub struct CtxMemoryAdmissionRuntime {
    admissions: Vec<CtxMemoryAdmission>,
    next_admission: usize,
}

pub struct CtxMemoryAdmissionResult {
    pub body: String,
    pub events: Vec<CtxMemoryAdmissionEvent>,
}

pub struct CtxMemoryAdmissionEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub subject_id: String,
}

#[derive(Clone)]
struct CtxMemoryAdmission {
    sequence: usize,
    admission_id: String,
    tenant_id: String,
    actor_id: String,
    memory_id: String,
    tier: String,
    content_tokens: usize,
    content_token_limit: usize,
    importance: usize,
    requested_write_count: usize,
    active_memory_count: usize,
    pending_queue_depth: usize,
    max_local_memory_count: usize,
    queue_depth_limit: usize,
    vector_index_ready: bool,
    cache_ready: bool,
    admission_decision: String,
    terminal_state: String,
    admitted_to_local_write_path: bool,
    admitted_to_queue: bool,
    rejected: bool,
    rejection_reason: String,
}

struct MemoryAdmissionRequest {
    tenant_id: String,
    actor_id: String,
    memory_id: String,
    tier: String,
    content_tokens: usize,
    content_token_limit: usize,
    importance: usize,
    requested_write_count: usize,
    active_memory_count: usize,
    pending_queue_depth: usize,
    max_local_memory_count: usize,
    queue_depth_limit: usize,
    vector_index_ready: bool,
    cache_ready: bool,
    provider_connected_memory_requested: bool,
    network_allowed: bool,
    secret_inheritance_allowed: bool,
    production_write_authority: bool,
}

impl CtxMemoryAdmissionRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn status(&self) -> &'static str {
        "LIVE-LOCAL-CTX-MEMORY-ADMISSION-FLOOR"
    }

    pub fn submit_json(&mut self, body: &str) -> Result<CtxMemoryAdmissionResult, String> {
        let request = parse_memory_admission_request(body)?;
        self.next_admission += 1;
        let security_rejected = request.provider_connected_memory_requested
            || request.network_allowed
            || request.secret_inheritance_allowed
            || request.production_write_authority;
        let content_too_large = request.content_tokens > request.content_token_limit;
        let memory_over_limit = request
            .active_memory_count
            .saturating_add(request.requested_write_count)
            > request.max_local_memory_count;
        let queue_full = request.pending_queue_depth >= request.queue_depth_limit;
        let (
            admission_decision,
            terminal_state,
            admitted_to_local_write_path,
            admitted_to_queue,
            rejected,
            rejection_reason,
        ) = if security_rejected {
            (
                "rejected_security_boundary",
                "CTX_MEMORY_ADMISSION_REJECTED_SECURITY_BOUNDARY",
                false,
                false,
                true,
                "provider_network_secret_or_production_write_authority_requested",
            )
        } else if content_too_large {
            (
                "queued_content_backpressure",
                "CTX_MEMORY_ADMISSION_QUEUED_BACKPRESSURE",
                false,
                true,
                false,
                "content_token_limit_exceeded",
            )
        } else if queue_full {
            (
                "rejected_queue_full",
                "CTX_MEMORY_ADMISSION_REJECTED_QUEUE_FULL",
                false,
                false,
                true,
                "queue_depth_limit_reached",
            )
        } else if memory_over_limit {
            (
                "queued_backpressure",
                "CTX_MEMORY_ADMISSION_QUEUED_BACKPRESSURE",
                false,
                true,
                false,
                "local_memory_capacity_pressure",
            )
        } else {
            (
                "accepted_local_without_write",
                "CTX_MEMORY_ADMISSION_RECORDED_ACCEPTED_LOCAL",
                true,
                false,
                false,
                "none",
            )
        };
        let admission = CtxMemoryAdmission {
            sequence: self.next_admission,
            admission_id: format!("ctx_memory_admission_{:06}", self.next_admission),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            memory_id: request.memory_id,
            tier: request.tier,
            content_tokens: request.content_tokens,
            content_token_limit: request.content_token_limit,
            importance: request.importance,
            requested_write_count: request.requested_write_count,
            active_memory_count: request.active_memory_count,
            pending_queue_depth: request.pending_queue_depth,
            max_local_memory_count: request.max_local_memory_count,
            queue_depth_limit: request.queue_depth_limit,
            vector_index_ready: request.vector_index_ready,
            cache_ready: request.cache_ready,
            admission_decision: admission_decision.to_string(),
            terminal_state: terminal_state.to_string(),
            admitted_to_local_write_path,
            admitted_to_queue,
            rejected,
            rejection_reason: rejection_reason.to_string(),
        };
        let events = admission_events(&admission);
        let body = render_admission_response_json(&admission);
        self.admissions.push(admission);
        Ok(CtxMemoryAdmissionResult { body, events })
    }

    pub fn admissions_json(&self) -> String {
        let accepted_count = self
            .admissions
            .iter()
            .filter(|admission| admission.admitted_to_local_write_path)
            .count();
        let queued_count = self
            .admissions
            .iter()
            .filter(|admission| admission.admitted_to_queue)
            .count();
        let rejected_count = self
            .admissions
            .iter()
            .filter(|admission| admission.rejected)
            .count();
        format!(
            r#"{{"name":"mdx-ctx-memory-admissions","status":"LIVE-LOCAL-CTX-MEMORY-ADMISSION-FLOOR","runtime":"mdx-ctx-engine","route":"/ctx/memory-admissions.json","submit_route":"/v1/ctx/memory-admissions","admission_count":{},"accepted_count":{},"queued_count":{},"rejected_count":{},"max_local_memory_count":{},"queue_depth_limit":{},"content_token_limit":{},"hot_path_budget_ms":20,"fairness_policy":"tenant_actor_weighted_memory_queue","backpressure_policy":"admit_until_local_budget_then_queue_and_reject_overflow","memory_write_performed":false,"vector_write_performed":false,"cache_write_performed":false,"provider_connected_memory_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"provider_calls_allowed":false,"production_writes_allowed":false,"admissions":[{}]}}"#,
            self.admissions.len(),
            accepted_count,
            queued_count,
            rejected_count,
            self.admissions
                .last()
                .map(|admission| admission.max_local_memory_count)
                .unwrap_or(DEFAULT_MAX_LOCAL_MEMORY_COUNT),
            self.admissions
                .last()
                .map(|admission| admission.queue_depth_limit)
                .unwrap_or(DEFAULT_QUEUE_DEPTH_LIMIT),
            self.admissions
                .last()
                .map(|admission| admission.content_token_limit)
                .unwrap_or(DEFAULT_CONTENT_TOKEN_LIMIT),
            self.admissions
                .iter()
                .map(render_admission_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

fn parse_memory_admission_request(body: &str) -> Result<MemoryAdmissionRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid CTX memory admission json: {error}"))?;
    Ok(MemoryAdmissionRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        memory_id: string_value(&value, "memory_id", "ctx_memory_admission_candidate_001"),
        tier: normalize_tier(&string_value(&value, "tier", "episodic")),
        content_tokens: usize_value(&value, "content_tokens", 512),
        content_token_limit: usize_value(
            &value,
            "content_token_limit",
            DEFAULT_CONTENT_TOKEN_LIMIT,
        ),
        importance: usize_value(&value, "importance", 80).min(100),
        requested_write_count: usize_value(&value, "requested_write_count", 1).max(1),
        active_memory_count: usize_value(&value, "active_memory_count", 0),
        pending_queue_depth: usize_value(&value, "pending_queue_depth", 0),
        max_local_memory_count: usize_value(
            &value,
            "max_local_memory_count",
            DEFAULT_MAX_LOCAL_MEMORY_COUNT,
        ),
        queue_depth_limit: usize_value(&value, "queue_depth_limit", DEFAULT_QUEUE_DEPTH_LIMIT),
        vector_index_ready: bool_value(&value, "vector_index_ready", false),
        cache_ready: bool_value(&value, "cache_ready", false),
        provider_connected_memory_requested: bool_value(
            &value,
            "provider_connected_memory_requested",
            false,
        ),
        network_allowed: bool_value(&value, "network_allowed", false),
        secret_inheritance_allowed: bool_value(&value, "secret_inheritance_allowed", false),
        production_write_authority: bool_value(&value, "production_write_authority", false),
    })
}

fn admission_events(admission: &CtxMemoryAdmission) -> Vec<CtxMemoryAdmissionEvent> {
    let mut events = vec![admission_event(admission, "memory_admission_recorded")];
    if admission.admitted_to_local_write_path {
        events.push(admission_event(
            admission,
            "memory_admission_accepted_local",
        ));
    }
    if admission.admitted_to_queue {
        events.push(admission_event(
            admission,
            "memory_admission_queued_backpressure",
        ));
    }
    if admission.rejected {
        events.push(admission_event(admission, "memory_admission_rejected"));
    }
    events.push(admission_event(
        admission,
        "memory_admission_authority_remained_blocked",
    ));
    events
}

fn admission_event(admission: &CtxMemoryAdmission, event_type: &str) -> CtxMemoryAdmissionEvent {
    CtxMemoryAdmissionEvent {
        event_type: event_type.to_string(),
        tenant_id: admission.tenant_id.clone(),
        subject_id: admission.admission_id.clone(),
    }
}

fn render_admission_response_json(admission: &CtxMemoryAdmission) -> String {
    format!(
        r#"{{"name":"mdx-ctx-memory-admission","status":"LIVE-LOCAL-CTX-MEMORY-ADMISSION-FLOOR","runtime":"mdx-ctx-engine","admission":{},"admission_id":{},"tenant_id":{},"actor_id":{},"memory_id":{},"tier":{},"terminal_state":{},"admission_decision":{},"content_tokens":{},"content_token_limit":{},"importance":{},"requested_write_count":{},"active_memory_count":{},"pending_queue_depth":{},"max_local_memory_count":{},"queue_depth_limit":{},"vector_index_ready":{},"cache_ready":{},"admitted_to_local_write_path":{},"admitted_to_queue":{},"rejected":{},"rejection_reason":{},"memory_write_performed":false,"durable_write_performed":false,"vector_write_performed":false,"cache_write_performed":false,"provider_connected_memory_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
        render_admission_json(admission),
        json_string_literal(&admission.admission_id),
        json_string_literal(&admission.tenant_id),
        json_string_literal(&admission.actor_id),
        json_string_literal(&admission.memory_id),
        json_string_literal(&admission.tier),
        json_string_literal(&admission.terminal_state),
        json_string_literal(&admission.admission_decision),
        admission.content_tokens,
        admission.content_token_limit,
        admission.importance,
        admission.requested_write_count,
        admission.active_memory_count,
        admission.pending_queue_depth,
        admission.max_local_memory_count,
        admission.queue_depth_limit,
        admission.vector_index_ready,
        admission.cache_ready,
        admission.admitted_to_local_write_path,
        admission.admitted_to_queue,
        admission.rejected,
        json_string_literal(&admission.rejection_reason)
    )
}

fn render_admission_json(admission: &CtxMemoryAdmission) -> String {
    format!(
        r#"{{"sequence":{},"admission_id":{},"tenant_id":{},"actor_id":{},"memory_id":{},"tier":{},"content_tokens":{},"content_token_limit":{},"importance":{},"requested_write_count":{},"active_memory_count":{},"pending_queue_depth":{},"max_local_memory_count":{},"queue_depth_limit":{},"vector_index_ready":{},"cache_ready":{},"admission_decision":{},"terminal_state":{},"admitted_to_local_write_path":{},"admitted_to_queue":{},"rejected":{},"rejection_reason":{},"memory_write_performed":false,"durable_write_performed":false,"vector_write_performed":false,"cache_write_performed":false,"provider_connected_memory_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
        admission.sequence,
        json_string_literal(&admission.admission_id),
        json_string_literal(&admission.tenant_id),
        json_string_literal(&admission.actor_id),
        json_string_literal(&admission.memory_id),
        json_string_literal(&admission.tier),
        admission.content_tokens,
        admission.content_token_limit,
        admission.importance,
        admission.requested_write_count,
        admission.active_memory_count,
        admission.pending_queue_depth,
        admission.max_local_memory_count,
        admission.queue_depth_limit,
        admission.vector_index_ready,
        admission.cache_ready,
        json_string_literal(&admission.admission_decision),
        json_string_literal(&admission.terminal_state),
        admission.admitted_to_local_write_path,
        admission.admitted_to_queue,
        admission.rejected,
        json_string_literal(&admission.rejection_reason)
    )
}

fn string_value(value: &Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn bool_value(value: &Value, key: &str, default: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn usize_value(value: &Value, key: &str, default: usize) -> usize {
    value
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default)
}

fn normalize_tier(tier: &str) -> String {
    match tier {
        "working" | "semantic" | "procedural" => tier.to_string(),
        _ => "episodic".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_admission_records_accept_queue_and_security_reject() {
        let mut runtime = CtxMemoryAdmissionRuntime::new();
        let accepted = runtime
            .submit_json(
                r#"{"active_memory_count":42,"vector_index_ready":true,"cache_ready":true}"#,
            )
            .expect("accepted");
        assert!(
            accepted
                .body
                .contains("CTX_MEMORY_ADMISSION_RECORDED_ACCEPTED_LOCAL")
        );
        assert!(
            accepted
                .body
                .contains("\"admitted_to_local_write_path\":true")
        );
        assert!(accepted.body.contains("\"memory_write_performed\":false"));
        let queued = runtime
            .submit_json(r#"{"active_memory_count":10000,"max_local_memory_count":10000}"#)
            .expect("queued");
        assert!(
            queued
                .body
                .contains("CTX_MEMORY_ADMISSION_QUEUED_BACKPRESSURE")
        );
        assert!(queued.body.contains("\"admitted_to_queue\":true"));
        let rejected = runtime
            .submit_json(r#"{"provider_connected_memory_requested":true}"#)
            .expect("rejected");
        assert!(
            rejected
                .body
                .contains("CTX_MEMORY_ADMISSION_REJECTED_SECURITY_BOUNDARY")
        );
        assert!(rejected.body.contains("\"provider_calls_allowed\":false"));
        let projection = runtime.admissions_json();
        assert!(projection.contains("\"admission_count\":3"));
        assert!(projection.contains("\"accepted_count\":1"));
        assert!(projection.contains("\"queued_count\":1"));
        assert!(projection.contains("\"rejected_count\":1"));
    }
}
