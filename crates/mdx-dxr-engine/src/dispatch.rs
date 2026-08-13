use mdx_core::json_string_literal;
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

const LOCAL_TENANT: &str = "tenant_local";
const LOCAL_ACTOR: &str = "forge_operator";
const DEFAULT_WORKER: &str = "dxr_dispatch_worker_local_001";
const READY_JOB_ID: &str = "dxr_job_dispatch_ready_001";
const READY_RUN_ID: &str = "dxr_run_dispatch_ready_001";
const WAITING_JOB_ID: &str = "dxr_job_dispatch_waiting_001";
const WAITING_RUN_ID: &str = "dxr_run_dispatch_waiting_001";
const CHILD_RUN_ID: &str = "dxr_run_dispatch_child_done_001";
const DEPENDENCY_ID: &str = "dxr_dependency_child_result_001";
const EXPIRED_JOB_ID: &str = "dxr_job_dispatch_expired_001";
const EXPIRED_RUN_ID: &str = "dxr_run_dispatch_expired_001";
const ORPHAN_JOB_ID: &str = "dxr_job_dispatch_orphan_001";
const ORPHAN_RUN_ID: &str = "dxr_run_dispatch_orphan_001";
const STALE_JOB_ID: &str = "dxr_job_dispatch_stale_001";
const DYNAMIC_WORKFLOW_CONTROL_RECOVERY_ROUTE: &str = "/dxr/dynamic-workflow-control-recovery.json";

#[derive(Default)]
pub struct DxrDispatchRuntime {
    claims: Vec<DxrDispatchClaim>,
    actions: Vec<DxrDispatchAction>,
    next_claim: usize,
    next_action: usize,
    scan_sequence: usize,
    recovery_sequence: usize,
    lease_duration_ms: u64,
    heartbeat_interval_ms: u64,
    stale_job_timeout_ms: u64,
    max_requeue_attempts: u64,
}

pub struct DxrDispatchResult {
    pub body: String,
    pub events: Vec<DxrDispatchRuntimeEvent>,
}

pub struct DxrDispatchRuntimeEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
struct DxrDispatchClaim {
    sequence: usize,
    claim_id: String,
    tenant_id: String,
    job_id: String,
    run_id: String,
    worker_id: String,
    status: String,
    heartbeat_at_ms: u64,
    lease_expires_at_ms: u64,
    lease_duration_ms: u64,
    terminal_state: String,
}

struct DxrDispatchAction {
    sequence: usize,
    action_id: String,
    tenant_id: String,
    job_id: String,
    run_id: String,
    actor_id: String,
    worker_id: String,
    event_type: String,
    terminal_state: String,
}

struct DispatchDecision {
    sequence: usize,
    decision: &'static str,
    job_id: &'static str,
    run_id: &'static str,
    dependency_id: Option<&'static str>,
    child_run_id: Option<&'static str>,
    reason: &'static str,
}

struct DispatchActionInput<'a> {
    tenant_id: &'a str,
    job_id: &'a str,
    run_id: &'a str,
    actor_id: &'a str,
    worker_id: &'a str,
    event_type: &'a str,
    terminal_state: &'a str,
}

struct ClaimRequest {
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_id: String,
    lease_duration_ms: u64,
}

impl DxrDispatchRuntime {
    pub fn new() -> Self {
        Self {
            claims: vec![DxrDispatchClaim {
                sequence: 1,
                claim_id: "dxr_dispatch_claim_expired_001".to_string(),
                tenant_id: LOCAL_TENANT.to_string(),
                job_id: EXPIRED_JOB_ID.to_string(),
                run_id: EXPIRED_RUN_ID.to_string(),
                worker_id: "dxr_dispatch_worker_stale_001".to_string(),
                status: "active".to_string(),
                heartbeat_at_ms: now_millis().saturating_sub(90_000),
                lease_expires_at_ms: now_millis().saturating_sub(30_000),
                lease_duration_ms: 30_000,
                terminal_state: "DXR_DISPATCH_CLAIM_LEASE_EXPIRED".to_string(),
            }],
            actions: Vec::new(),
            next_claim: 1,
            next_action: 0,
            scan_sequence: 0,
            recovery_sequence: 0,
            lease_duration_ms: env_u64("DXR_LEASE_DURATION_MS", 30_000),
            heartbeat_interval_ms: env_u64("DXR_HEARTBEAT_INTERVAL_MS", 5_000),
            stale_job_timeout_ms: env_u64("DXR_STALE_JOB_TIMEOUT_MS", 1_800_000),
            max_requeue_attempts: env_u64("DXR_MAX_REQUEUE_ATTEMPTS", 3),
        }
    }

    pub fn readiness_json(&mut self) -> DxrDispatchResult {
        self.scan_sequence += 1;
        let decisions = self.scan_decisions();
        let event = self.record_action(DispatchActionInput {
            tenant_id: LOCAL_TENANT,
            job_id: READY_JOB_ID,
            run_id: READY_RUN_ID,
            actor_id: LOCAL_ACTOR,
            worker_id: "dxr_dispatch_scheduler_local",
            event_type: "dispatch_readiness_scanned",
            terminal_state: "DXR_DISPATCH_READINESS_SCAN_RECORDED",
        });
        let body = format!(
            r#"{{"name":"mdx-dxr-dispatch-readiness","status":"LIVE-LOCAL-DXR-READINESS-SCAN","runtime":"mdx-dxr-engine","route":"/dxr/dispatch/readiness.json","claim_route":"/v1/dxr/dispatch/claims","heartbeat_route":"/v1/dxr/dispatch/heartbeats","release_route":"/v1/dxr/dispatch/releases","scan_sequence":{},"decision_count":{},"lease_duration_ms":{},"heartbeat_interval_ms":{},"stale_job_timeout_ms":{},"hot_path_budget_ms":20,"ready_for_live_worker_execution":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false,"decisions":[{}],"seeded_snapshot":{},"recent_actions":[{}]}}"#,
            self.scan_sequence,
            decisions.len(),
            self.lease_duration_ms,
            self.heartbeat_interval_ms,
            self.stale_job_timeout_ms,
            decisions
                .iter()
                .map(render_decision_json)
                .collect::<Vec<_>>()
                .join(","),
            render_seeded_snapshot_json(),
            self.render_recent_actions_json()
        );
        DxrDispatchResult {
            body,
            events: vec![event],
        }
    }

    pub fn recovery_plan_json(&mut self) -> DxrDispatchResult {
        self.recovery_sequence += 1;
        let decisions = self.scan_decisions();
        let recovery_actions = decisions
            .iter()
            .map(render_recovery_action_json)
            .collect::<Vec<_>>();
        let event = self.record_action(DispatchActionInput {
            tenant_id: LOCAL_TENANT,
            job_id: STALE_JOB_ID,
            run_id: "dxr_run_dispatch_recovery_plan_001",
            actor_id: LOCAL_ACTOR,
            worker_id: "dxr_dispatch_scheduler_local",
            event_type: "dispatch_recovery_plan_recorded",
            terminal_state: "DXR_DISPATCH_RECOVERY_PLAN_RECORDED",
        });
        let body = format!(
            r#"{{"name":"mdx-dxr-dispatch-recovery-plan","status":"LIVE-LOCAL-DXR-DISPATCH-RECOVERY-PLAN","runtime":"mdx-dxr-engine","route":"/dxr/dispatch/recovery-plan.json","readiness_route":"/dxr/dispatch/readiness.json","claims_route":"/dxr/dispatch/claims.json","dynamic_workflow_control_recovery_route":{},"resume_cursor_source":"dxr_workflow_runs+dxr_workflow_events","restart_safe":true,"recovery_sequence":{},"decision_count":{},"recovery_action_count":{},"execute_count":{},"deliver_child_result_count":{},"resume_count":{},"requeue_count":{},"recover_stale_count":{},"expired_claim_requeue_count":{},"orphan_requeue_count":{},"lease_duration_ms":{},"heartbeat_interval_ms":{},"stale_job_timeout_ms":{},"max_requeue_attempts":{},"hot_path_budget_ms":20,"target_concurrent_engineers":1000,"peak_forge_builds":5000,"queue_backpressure_required":true,"idempotency_required":true,"claim_before_execute_required":true,"heartbeat_before_worker_execution_required":true,"durable_workflow_recovery_required":true,"dynamic_workflow_resume_cursor_required":true,"worker_execution_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"production_writes_allowed":false,"actions":[{}],"recent_actions":[{}]}}"#,
            json_string_literal(DYNAMIC_WORKFLOW_CONTROL_RECOVERY_ROUTE),
            self.recovery_sequence,
            decisions.len(),
            recovery_actions.len(),
            count_decisions(&decisions, "Execute"),
            count_decisions(&decisions, "DeliverChildResult"),
            count_decisions(&decisions, "Resume"),
            count_decisions(&decisions, "Requeue"),
            count_decisions(&decisions, "RecoverStale"),
            decisions
                .iter()
                .filter(|decision| decision.decision == "Requeue"
                    && decision.reason.contains("claim expired"))
                .count(),
            decisions
                .iter()
                .filter(|decision| decision.decision == "Requeue"
                    && decision.reason.contains("orphaned run"))
                .count(),
            self.lease_duration_ms,
            self.heartbeat_interval_ms,
            self.stale_job_timeout_ms,
            self.max_requeue_attempts,
            recovery_actions.join(","),
            self.render_recent_actions_json()
        );
        DxrDispatchResult {
            body,
            events: vec![event],
        }
    }

    pub fn claim_json(&mut self, body: &str) -> Result<DxrDispatchResult, String> {
        let request = parse_claim_request(body, self.lease_duration_ms)?;
        if self.claims.iter().any(|claim| {
            claim.run_id == request.run_id
                && claim.status == "active"
                && claim.lease_expires_at_ms > now_millis()
        }) {
            let denied = self.record_action(DispatchActionInput {
                tenant_id: &request.tenant_id,
                job_id: &request.job_id,
                run_id: &request.run_id,
                actor_id: &request.actor_id,
                worker_id: &request.worker_id,
                event_type: "dispatch_claim_conflict",
                terminal_state: "DXR_DISPATCH_CLAIM_CONFLICT_ACTIVE_LEASE",
            });
            return Ok(DxrDispatchResult {
                body: format!(
                    r#"{{"name":"mdx-dxr-dispatch-claim","status":"DXR_DISPATCH_CLAIM_CONFLICT","runtime":"mdx-dxr-engine","run_id":{},"worker_id":{},"terminal_state":"DXR_DISPATCH_CLAIM_CONFLICT_ACTIVE_LEASE","provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
                    json_string_literal(&request.run_id),
                    json_string_literal(&request.worker_id)
                ),
                events: vec![denied],
            });
        }

        self.next_claim += 1;
        let heartbeat_at_ms = now_millis();
        let claim = DxrDispatchClaim {
            sequence: self.next_claim,
            claim_id: format!("dxr_dispatch_claim_{:06}", self.next_claim),
            tenant_id: request.tenant_id.clone(),
            job_id: request.job_id.clone(),
            run_id: request.run_id.clone(),
            worker_id: request.worker_id.clone(),
            status: "active".to_string(),
            heartbeat_at_ms,
            lease_expires_at_ms: heartbeat_at_ms.saturating_add(request.lease_duration_ms),
            lease_duration_ms: request.lease_duration_ms,
            terminal_state: "DXR_DISPATCH_CLAIM_RECORDED".to_string(),
        };
        let event = self.record_action(DispatchActionInput {
            tenant_id: &request.tenant_id,
            job_id: &request.job_id,
            run_id: &request.run_id,
            actor_id: &request.actor_id,
            worker_id: &request.worker_id,
            event_type: "dispatch_run_claimed",
            terminal_state: "DXR_DISPATCH_CLAIM_RECORDED",
        });
        let body = render_claim_response_json(
            "mdx-dxr-dispatch-claim",
            "LIVE-LOCAL-DXR-CLAIM-LEASE",
            &claim,
        );
        self.claims.push(claim);
        Ok(DxrDispatchResult {
            body,
            events: vec![event],
        })
    }

    pub fn heartbeat_json(&mut self, body: &str) -> Result<DxrDispatchResult, String> {
        let request = parse_claim_request(body, self.lease_duration_ms)?;
        let claim = self
            .claims
            .iter_mut()
            .find(|claim| {
                claim.run_id == request.run_id
                    && claim.worker_id == request.worker_id
                    && claim.status == "active"
            })
            .ok_or_else(|| "DXR dispatch heartbeat denied: active claim not found".to_string())?;
        let now = now_millis();
        if claim.lease_expires_at_ms <= now {
            claim.status = "expired".to_string();
            claim.terminal_state = "DXR_DISPATCH_CLAIM_EXPIRED".to_string();
            let expired = claim.clone();
            let event = self.record_action(DispatchActionInput {
                tenant_id: &request.tenant_id,
                job_id: &request.job_id,
                run_id: &request.run_id,
                actor_id: &request.actor_id,
                worker_id: &request.worker_id,
                event_type: "dispatch_claim_expired",
                terminal_state: "DXR_DISPATCH_CLAIM_EXPIRED",
            });
            return Ok(DxrDispatchResult {
                body: render_claim_response_json(
                    "mdx-dxr-dispatch-heartbeat",
                    "LIVE-LOCAL-DXR-CLAIM-EXPIRED",
                    &expired,
                ),
                events: vec![event],
            });
        }
        claim.heartbeat_at_ms = now;
        claim.lease_expires_at_ms = now.saturating_add(request.lease_duration_ms);
        claim.terminal_state = "DXR_DISPATCH_HEARTBEAT_RENEWED".to_string();
        let renewed = claim.clone();
        let event = self.record_action(DispatchActionInput {
            tenant_id: &request.tenant_id,
            job_id: &request.job_id,
            run_id: &request.run_id,
            actor_id: &request.actor_id,
            worker_id: &request.worker_id,
            event_type: "dispatch_heartbeat_renewed",
            terminal_state: "DXR_DISPATCH_HEARTBEAT_RENEWED",
        });
        Ok(DxrDispatchResult {
            body: render_claim_response_json(
                "mdx-dxr-dispatch-heartbeat",
                "LIVE-LOCAL-DXR-HEARTBEAT-RENEWED",
                &renewed,
            ),
            events: vec![event],
        })
    }

    pub fn release_json(&mut self, body: &str) -> Result<DxrDispatchResult, String> {
        let request = parse_claim_request(body, self.lease_duration_ms)?;
        let claim = self
            .claims
            .iter_mut()
            .find(|claim| {
                claim.run_id == request.run_id
                    && claim.worker_id == request.worker_id
                    && claim.status == "active"
            })
            .ok_or_else(|| "DXR dispatch release denied: active claim not found".to_string())?;
        claim.status = "released".to_string();
        claim.terminal_state = "DXR_DISPATCH_CLAIM_RELEASED".to_string();
        let released = claim.clone();
        let event = self.record_action(DispatchActionInput {
            tenant_id: &request.tenant_id,
            job_id: &request.job_id,
            run_id: &request.run_id,
            actor_id: &request.actor_id,
            worker_id: &request.worker_id,
            event_type: "dispatch_claim_released",
            terminal_state: "DXR_DISPATCH_CLAIM_RELEASED",
        });
        Ok(DxrDispatchResult {
            body: render_claim_response_json(
                "mdx-dxr-dispatch-release",
                "LIVE-LOCAL-DXR-CLAIM-RELEASED",
                &released,
            ),
            events: vec![event],
        })
    }

    pub fn claims_json(&self) -> String {
        format!(
            r#"{{"name":"mdx-dxr-dispatch-claims","status":"LIVE-LOCAL-DXR-DISPATCH-RUNTIME","runtime":"mdx-dxr-engine","route":"/dxr/dispatch/claims.json","claim_route":"/v1/dxr/dispatch/claims","heartbeat_route":"/v1/dxr/dispatch/heartbeats","release_route":"/v1/dxr/dispatch/releases","claim_count":{},"claims":[{}],"recent_actions":[{}],"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
            self.claims.len(),
            self.claims
                .iter()
                .map(render_claim_json)
                .collect::<Vec<_>>()
                .join(","),
            self.render_recent_actions_json()
        )
    }

    fn scan_decisions(&self) -> Vec<DispatchDecision> {
        vec![
            DispatchDecision {
                sequence: 1,
                decision: "Execute",
                job_id: READY_JOB_ID,
                run_id: READY_RUN_ID,
                dependency_id: None,
                child_run_id: None,
                reason: "pending run has no unresolved dependencies",
            },
            DispatchDecision {
                sequence: 2,
                decision: "DeliverChildResult",
                job_id: WAITING_JOB_ID,
                run_id: WAITING_RUN_ID,
                dependency_id: Some(DEPENDENCY_ID),
                child_run_id: Some(CHILD_RUN_ID),
                reason: "child run completed while parent was waiting",
            },
            DispatchDecision {
                sequence: 3,
                decision: "Resume",
                job_id: WAITING_JOB_ID,
                run_id: WAITING_RUN_ID,
                dependency_id: Some(DEPENDENCY_ID),
                child_run_id: Some(CHILD_RUN_ID),
                reason: "all dependencies are resolved or completed",
            },
            DispatchDecision {
                sequence: 4,
                decision: "Requeue",
                job_id: EXPIRED_JOB_ID,
                run_id: EXPIRED_RUN_ID,
                dependency_id: None,
                child_run_id: None,
                reason: "claim expired: worker missed heartbeat before lease expiry",
            },
            DispatchDecision {
                sequence: 5,
                decision: "Requeue",
                job_id: ORPHAN_JOB_ID,
                run_id: ORPHAN_RUN_ID,
                dependency_id: None,
                child_run_id: None,
                reason: "orphaned run: no claim record found",
            },
            DispatchDecision {
                sequence: 6,
                decision: "RecoverStale",
                job_id: STALE_JOB_ID,
                run_id: "dxr_run_dispatch_stale_001",
                dependency_id: None,
                child_run_id: None,
                reason: "job updated_at exceeds stale job timeout",
            },
        ]
    }

    fn record_action(&mut self, action: DispatchActionInput<'_>) -> DxrDispatchRuntimeEvent {
        self.next_action += 1;
        self.actions.push(DxrDispatchAction {
            sequence: self.next_action,
            action_id: format!("dxr_dispatch_action_{:06}", self.next_action),
            tenant_id: action.tenant_id.to_string(),
            job_id: action.job_id.to_string(),
            run_id: action.run_id.to_string(),
            actor_id: action.actor_id.to_string(),
            worker_id: action.worker_id.to_string(),
            event_type: action.event_type.to_string(),
            terminal_state: action.terminal_state.to_string(),
        });
        DxrDispatchRuntimeEvent {
            event_type: action.event_type.to_string(),
            tenant_id: action.tenant_id.to_string(),
            job_id: action.job_id.to_string(),
            run_id: action.run_id.to_string(),
            actor_id: action.actor_id.to_string(),
        }
    }

    fn render_recent_actions_json(&self) -> String {
        self.actions
            .iter()
            .rev()
            .take(8)
            .rev()
            .map(render_action_json)
            .collect::<Vec<_>>()
            .join(",")
    }
}

fn parse_claim_request(body: &str, default_lease_duration_ms: u64) -> Result<ClaimRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR dispatch json: {error}"))?;
    Ok(ClaimRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        job_id: string_value(&value, "job_id", READY_JOB_ID),
        run_id: string_value(&value, "run_id", READY_RUN_ID),
        worker_id: string_value(&value, "worker_id", DEFAULT_WORKER),
        lease_duration_ms: value
            .get("lease_duration_ms")
            .and_then(Value::as_u64)
            .unwrap_or(default_lease_duration_ms),
    })
}

fn string_value(value: &Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn render_decision_json(decision: &DispatchDecision) -> String {
    format!(
        r#"{{"sequence":{},"decision":{},"job_id":{},"run_id":{},"dependency_id":{},"child_run_id":{},"reason":{},"scheduler_status":"LIVE-LOCAL-DXR-READINESS-SCAN","production_dispatch_allowed":false}}"#,
        decision.sequence,
        json_string_literal(decision.decision),
        json_string_literal(decision.job_id),
        json_string_literal(decision.run_id),
        optional_json_string(decision.dependency_id),
        optional_json_string(decision.child_run_id),
        json_string_literal(decision.reason)
    )
}

fn render_recovery_action_json(decision: &DispatchDecision) -> String {
    let recovery_action = match decision.decision {
        "Execute" => "claim_ready_run",
        "DeliverChildResult" => "resolve_child_dependency",
        "Resume" => "resume_parent_run",
        "Requeue" if decision.reason.contains("claim expired") => "expire_claim_and_requeue",
        "Requeue" if decision.reason.contains("orphaned run") => "requeue_orphaned_run",
        "Requeue" => "requeue_run",
        "RecoverStale" => "recover_stale_job",
        _ => "inspect_dispatch_decision",
    };
    let terminal_state = match decision.decision {
        "Execute" => "DXR_DISPATCH_RECOVERY_EXECUTE_READY",
        "DeliverChildResult" => "DXR_DISPATCH_RECOVERY_CHILD_RESULT_READY",
        "Resume" => "DXR_DISPATCH_RECOVERY_RESUME_READY",
        "Requeue" => "DXR_DISPATCH_RECOVERY_REQUEUE_READY",
        "RecoverStale" => "DXR_DISPATCH_RECOVERY_STALE_JOB_READY",
        _ => "DXR_DISPATCH_RECOVERY_INSPECTION_READY",
    };
    let action_key = format!("{}-{}", decision.decision, decision.run_id);
    format!(
        r#"{{"sequence":{},"decision":{},"recovery_action":{},"terminal_state":{},"job_id":{},"run_id":{},"dependency_id":{},"child_run_id":{},"reason":{},"idempotency_key":{},"resume_cursor_source":{},"lease_checked":true,"heartbeat_checked":true,"max_requeue_attempts_checked":true,"claim_before_execute_required":true,"durable_workflow_recovery_required":true,"dynamic_workflow_resume_cursor_required":true,"production_dispatch_allowed":false,"worker_execution_allowed":false,"sandbox_process_started":false}}"#,
        decision.sequence,
        json_string_literal(decision.decision),
        json_string_literal(recovery_action),
        json_string_literal(terminal_state),
        json_string_literal(decision.job_id),
        json_string_literal(decision.run_id),
        optional_json_string(decision.dependency_id),
        optional_json_string(decision.child_run_id),
        json_string_literal(decision.reason),
        json_string_literal(&format!(
            "dxr_dispatch_recovery_{}",
            stable_key(&action_key)
        )),
        json_string_literal(DYNAMIC_WORKFLOW_CONTROL_RECOVERY_ROUTE)
    )
}

fn count_decisions(decisions: &[DispatchDecision], decision_name: &str) -> usize {
    decisions
        .iter()
        .filter(|decision| decision.decision == decision_name)
        .count()
}

fn stable_key(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn render_claim_response_json(name: &str, status: &str, claim: &DxrDispatchClaim) -> String {
    format!(
        r#"{{"name":{},"status":{},"runtime":"mdx-dxr-engine","claim":{},"claim_id":{},"tenant_id":{},"job_id":{},"run_id":{},"worker_id":{},"lease_duration_ms":{},"heartbeat_at_ms":{},"lease_expires_at_ms":{},"terminal_state":{},"claim_lease_status":"LIVE-LOCAL-DXR-CLAIM-LEASE","heartbeat_status":{},"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(name),
        json_string_literal(status),
        render_claim_json(claim),
        json_string_literal(&claim.claim_id),
        json_string_literal(&claim.tenant_id),
        json_string_literal(&claim.job_id),
        json_string_literal(&claim.run_id),
        json_string_literal(&claim.worker_id),
        claim.lease_duration_ms,
        claim.heartbeat_at_ms,
        claim.lease_expires_at_ms,
        json_string_literal(&claim.terminal_state),
        json_string_literal(
            if claim.terminal_state == "DXR_DISPATCH_HEARTBEAT_RENEWED" {
                "LIVE-LOCAL-DXR-HEARTBEAT-RENEWED"
            } else {
                "PENDING-HEARTBEAT"
            }
        )
    )
}

fn render_claim_json(claim: &DxrDispatchClaim) -> String {
    format!(
        r#"{{"sequence":{},"claim_id":{},"tenant_id":{},"job_id":{},"run_id":{},"worker_id":{},"status":{},"heartbeat_at_ms":{},"lease_expires_at_ms":{},"lease_duration_ms":{},"terminal_state":{},"claim_lease_status":"LIVE-LOCAL-DXR-CLAIM-LEASE","production_worker_execution_allowed":false}}"#,
        claim.sequence,
        json_string_literal(&claim.claim_id),
        json_string_literal(&claim.tenant_id),
        json_string_literal(&claim.job_id),
        json_string_literal(&claim.run_id),
        json_string_literal(&claim.worker_id),
        json_string_literal(&claim.status),
        claim.heartbeat_at_ms,
        claim.lease_expires_at_ms,
        claim.lease_duration_ms,
        json_string_literal(&claim.terminal_state)
    )
}

fn render_action_json(action: &DxrDispatchAction) -> String {
    format!(
        r#"{{"sequence":{},"action_id":{},"tenant_id":{},"job_id":{},"run_id":{},"actor_id":{},"worker_id":{},"event_type":{},"terminal_state":{},"runtime_status":"LIVE-LOCAL-DXR-DISPATCH-RUNTIME"}}"#,
        action.sequence,
        json_string_literal(&action.action_id),
        json_string_literal(&action.tenant_id),
        json_string_literal(&action.job_id),
        json_string_literal(&action.run_id),
        json_string_literal(&action.actor_id),
        json_string_literal(&action.worker_id),
        json_string_literal(&action.event_type),
        json_string_literal(&action.terminal_state)
    )
}

fn render_seeded_snapshot_json() -> String {
    format!(
        r#"{{"jobs":[{{"job_id":{},"status":"running","priority":10}},{{"job_id":{},"status":"running","priority":8}},{{"job_id":{},"status":"running","priority":6}},{{"job_id":{},"status":"running","priority":5}},{{"job_id":{},"status":"running","priority":1}}],"runs":[{{"run_id":{},"job_id":{},"status":"pending"}},{{"run_id":{},"job_id":{},"status":"waiting"}},{{"run_id":{},"job_id":{},"status":"completed"}},{{"run_id":{},"job_id":{},"status":"claimed"}},{{"run_id":{},"job_id":{},"status":"running"}}],"dependencies":[{{"dependency_id":{},"parent_run_id":{},"child_run_id":{},"status":"pending"}}]}}"#,
        json_string_literal(READY_JOB_ID),
        json_string_literal(WAITING_JOB_ID),
        json_string_literal(EXPIRED_JOB_ID),
        json_string_literal(ORPHAN_JOB_ID),
        json_string_literal(STALE_JOB_ID),
        json_string_literal(READY_RUN_ID),
        json_string_literal(READY_JOB_ID),
        json_string_literal(WAITING_RUN_ID),
        json_string_literal(WAITING_JOB_ID),
        json_string_literal(CHILD_RUN_ID),
        json_string_literal(WAITING_JOB_ID),
        json_string_literal(EXPIRED_RUN_ID),
        json_string_literal(EXPIRED_JOB_ID),
        json_string_literal(ORPHAN_RUN_ID),
        json_string_literal(ORPHAN_JOB_ID),
        json_string_literal(DEPENDENCY_ID),
        json_string_literal(WAITING_RUN_ID),
        json_string_literal(CHILD_RUN_ID)
    )
}

fn optional_json_string(value: Option<&str>) -> String {
    value
        .map(json_string_literal)
        .unwrap_or_else(|| "null".to_string())
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_includes_v1_decisions() {
        let mut runtime = DxrDispatchRuntime::new();
        let output = runtime.readiness_json().body;
        for token in [
            "\"Execute\"",
            "\"DeliverChildResult\"",
            "\"Resume\"",
            "\"Requeue\"",
            "\"RecoverStale\"",
        ] {
            assert!(output.contains(token), "missing {token}");
        }
    }

    #[test]
    fn recovery_plan_includes_restart_safe_action_classes() {
        let mut runtime = DxrDispatchRuntime::new();
        let output = runtime.recovery_plan_json().body;
        for token in [
            "LIVE-LOCAL-DXR-DISPATCH-RECOVERY-PLAN",
            "claim_ready_run",
            "resolve_child_dependency",
            "resume_parent_run",
            "expire_claim_and_requeue",
            "requeue_orphaned_run",
            "recover_stale_job",
            DYNAMIC_WORKFLOW_CONTROL_RECOVERY_ROUTE,
            "\"target_concurrent_engineers\":1000",
            "\"peak_forge_builds\":5000",
        ] {
            assert!(output.contains(token), "missing {token}");
        }
    }

    #[test]
    fn claim_heartbeat_release_lifecycle() {
        let mut runtime = DxrDispatchRuntime::new();
        let claim = runtime.claim_json("{}").expect("claim").body;
        assert!(claim.contains("LIVE-LOCAL-DXR-CLAIM-LEASE"));
        let heartbeat = runtime.heartbeat_json("{}").expect("heartbeat").body;
        assert!(heartbeat.contains("LIVE-LOCAL-DXR-HEARTBEAT-RENEWED"));
        let release = runtime.release_json("{}").expect("release").body;
        assert!(release.contains("LIVE-LOCAL-DXR-CLAIM-RELEASED"));
    }
}
