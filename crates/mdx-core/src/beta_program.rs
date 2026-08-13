// Beta program operating contract.
//
// Enrollment is the auditable authority: every participant enters the beta by a
// beta.enrollment.recorded receipt. The dashboard and telemetry routes then read
// a tenant-scoped latest-active projection instead of scanning arbitrary client
// claims. Source-B telemetry is deliberately narrower than product events: it
// records safe activation, navigation, abandonment, and client-side timing
// signals the ledger cannot observe on its own. Rejections are receipt-bound too,
// but never persist the unsafe payload.

use crate::{
    ActionKind, ActorId, CorrelationIds, GovernedWriteIdentity, LoopId, LoopRun, MdxKernel,
    StorageProvider, TenantId, TraceId, WorkflowId, payload,
};
use std::collections::{BTreeMap, BTreeSet};

pub const BETA_ENROLLMENT_ROUTE: &str = "/beta/enrollments.json";
pub const BETA_ENROLLMENT_PROJECTION_ROUTE: &str = "/beta/enrollments/projection.json";
pub const BETA_WAITLIST_ROUTE: &str = "/beta/waitlist.json";
pub const BETA_WAITLIST_PROJECTION_ROUTE: &str = "/beta/waitlist/projection.json";
pub const BETA_TELEMETRY_ROUTE: &str = "/beta/telemetry-events.json";
pub const BETA_TELEMETRY_PROJECTION_ROUTE: &str = "/beta/telemetry-events/projection.json";

pub const BETA_WAITLIST_RECEIPT_KIND: &str = "beta.waitlist.requested";
pub const BETA_ENROLLMENT_RECEIPT_KIND: &str = "beta.enrollment.recorded";
pub const BETA_COHORT_HALT_RECEIPT_KIND: &str = "beta.cohort_halt.recorded";
pub const BETA_TELEMETRY_RECEIPT_KIND: &str = "beta.telemetry.recorded";
pub const BETA_TELEMETRY_REJECTION_RECEIPT_KIND: &str = "beta.telemetry.rejected";

pub const BETA_ENROLLMENT_STATUSES: &[&str] = &["active", "inactive", "revoked"];
pub const BETA_RISK_TIERS: &[&str] = &["trusted", "standard", "watch", "restricted"];
pub const BETA_TELEMETRY_EVENT_KINDS: &[&str] = &[
    "activation_step",
    "surface_visit",
    "session_start",
    "session_end",
    "flow_abandoned",
    "action_abandoned",
    "feedback_opened",
    "interaction_timing",
];
pub const BETA_OPERATIONAL_TELEMETRY_KINDS: &[&str] = &["interaction_timing"];
pub const BETA_TELEMETRY_REJECTION_REASONS: &[&str] = &[
    "invalid_json",
    "unknown_field",
    "unknown_event_kind",
    "missing_field",
    "unsafe_value",
    "invalid_route",
    "missing_enrollment",
    "product_analytics_opt_out",
    "invalid_value",
];

const SAFE_TEXT_MAX_CHARS: usize = 160;
const SAFE_TIMESTAMP_MAX_CHARS: usize = 64;

const TELEMETRY_SAFE_TEXT_FIELDS: &[&str] = &[
    "type",
    "route",
    "ts",
    "step",
    "surface",
    "flow",
    "last_step",
    "action",
    "reason_category",
    "interaction_kind",
    "completed",
    "duration_ms",
    "budget_ms",
    "value",
    "app_version",
];

const FORBIDDEN_VALUE_MARKERS: &[&str] = &[
    "secret",
    "password",
    "passwd",
    "api_key",
    "apikey",
    "api key",
    "bearer ",
    "authorization:",
    "private key",
    "-----begin",
    "client_secret",
    "access_token",
    "refresh_token",
    "credential",
    "token=",
    "token:",
    "system prompt",
    "prompt:",
    "model output",
    "output:",
    "completion:",
    "assistant:",
    "ignore previous instructions",
    "message body",
    "page body",
    "message_body",
    "page_body",
    "draft_text",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BetaEnrollment<'a> {
    pub participant_id: &'a str,
    pub cohort_id: &'a str,
    pub role: &'a str,
    pub use_case: &'a str,
    pub expected_first_task: &'a str,
    pub support_owner: &'a str,
    pub risk_tier: &'a str,
    pub status: &'a str,
    pub product_analytics_consent: bool,
    pub follow_up_contact_consent: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafeBetaEnrollment {
    pub participant_id: String,
    pub cohort_id: String,
    pub role: String,
    pub use_case: String,
    pub expected_first_task: String,
    pub support_owner: String,
    pub risk_tier: String,
    pub status: String,
    pub product_analytics_consent: bool,
    pub follow_up_contact_consent: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BetaWaitlistRequest<'a> {
    pub applicant_id: &'a str,
    pub email_hash: &'a str,
    pub role: &'a str,
    pub repo_ecosystem: &'a str,
    pub build_goal: &'a str,
    pub requested_cohort: &'a str,
    pub referral_source: &'a str,
    pub follow_up_contact_consent: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafeBetaWaitlistRequest {
    pub applicant_id: String,
    pub email_hash: String,
    pub role: String,
    pub repo_ecosystem: String,
    pub build_goal: String,
    pub requested_cohort: String,
    pub referral_source: String,
    pub follow_up_contact_consent: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BetaProgramError {
    Missing(&'static str),
    UnknownStatus(String),
    UnknownRiskTier(String),
    UnknownTelemetryKind(String),
    InvalidRoute(&'static str),
    InvalidValue { field: String, reason: &'static str },
    UnsafeValue { field: String, marker: String },
    MissingEnrollment,
    ProductAnalyticsOptOut,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BetaWaitlistReport {
    pub status: &'static str,
    pub waitlist_receipt_id: String,
    pub policy_decision_id: String,
    pub applicant_id: String,
    pub email_hash: String,
    pub role: String,
    pub repo_ecosystem: String,
    pub build_goal: String,
    pub requested_cohort: String,
    pub referral_source: String,
    pub follow_up_contact_consent: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BetaEnrollmentReport {
    pub status: &'static str,
    pub enrollment_receipt_id: String,
    pub policy_decision_id: String,
    pub auth_session_status: String,
    pub participant_id: String,
    pub participant_actor_id: String,
    pub cohort_id: String,
    pub enrollment_status: String,
    pub product_analytics_consent: bool,
    pub follow_up_contact_consent: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BetaCohortHalt<'a> {
    pub cohort_id: &'a str,
    pub reason: &'a str,
    pub active_count: usize,
    pub cap: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BetaCohortHaltReport {
    pub status: &'static str,
    pub cohort_halt_receipt_id: String,
    pub policy_decision_id: String,
    pub auth_session_status: String,
    pub cohort_id: String,
    pub reason: String,
    pub active_count: usize,
    pub cap: usize,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BetaWaitlistSnapshot {
    pub receipt_id: String,
    pub receipt_timestamp: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub applicant_id: String,
    pub email_hash: String,
    pub role: String,
    pub repo_ecosystem: String,
    pub build_goal: String,
    pub requested_cohort: String,
    pub referral_source: String,
    pub follow_up_contact_consent: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BetaWaitlistProjection {
    pub records: Vec<BetaWaitlistSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BetaEnrollmentSnapshot {
    pub receipt_id: String,
    pub receipt_timestamp: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub participant_id: String,
    pub cohort_id: String,
    pub role: String,
    pub use_case: String,
    pub expected_first_task: String,
    pub support_owner: String,
    pub risk_tier: String,
    pub status: String,
    pub product_analytics_consent: bool,
    pub follow_up_contact_consent: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BetaEnrollmentProjection {
    pub records: Vec<BetaEnrollmentSnapshot>,
    pub latest_active: Vec<BetaEnrollmentSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BetaTelemetryEvent {
    pub event_kind: String,
    pub route: String,
    pub occurred_at: String,
    pub fields: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafeBetaTelemetryEvent {
    pub event_kind: String,
    pub route: String,
    pub occurred_at: String,
    pub fields: BTreeMap<String, String>,
    pub operational_security_telemetry: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BetaTelemetryReport {
    pub status: &'static str,
    pub telemetry_receipt_id: String,
    pub policy_decision_id: String,
    pub auth_session_status: String,
    pub event_kind: String,
    pub cohort_id: String,
    pub participant_actor_id: String,
    pub product_analytics_consent_observed: bool,
    pub operational_security_telemetry: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BetaTelemetryRejectionReport {
    pub status: &'static str,
    pub rejection_receipt_id: String,
    pub policy_decision_id: String,
    pub auth_session_status: String,
    pub attempted_event_kind: String,
    pub rejection_reason: String,
    pub route: String,
    pub cohort_id: String,
    pub participant_actor_id: String,
    pub unsafe_payload_persisted: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BetaParticipantJourney {
    pub tenant_id: String,
    pub participant_actor_id: String,
    pub participant_id: String,
    pub cohort_id: String,
    pub role: String,
    pub expected_first_task: String,
    pub support_owner: String,
    pub product_analytics_consent: bool,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub event_count: usize,
    pub session_start_count: usize,
    pub session_end_count: usize,
    pub distinct_session_day_count: usize,
    pub activation_steps: Vec<String>,
    pub surface_visit_counts: BTreeMap<String, usize>,
    pub feedback_opened_count: usize,
    pub first_value_observed: bool,
    pub return_session_observed: bool,
    pub last_app_version: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BetaParticipantJourneyProjection {
    pub journeys: Vec<BetaParticipantJourney>,
}

fn token_like(word: &str) -> bool {
    let trimmed =
        word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '.');
    if trimmed.len() < 12 {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    const SECRET_PREFIXES: &[&str] = &["sk-", "xai-", "ghp_", "gho_", "github_pat_", "akia"];
    if SECRET_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        return true;
    }
    lower.starts_with("eyj") && trimmed.matches('.').count() >= 2
}

fn value_forbidden_marker(value: &str) -> Option<String> {
    let lower = value.to_ascii_lowercase();
    if let Some(marker) = FORBIDDEN_VALUE_MARKERS
        .iter()
        .find(|marker| lower.contains(**marker))
    {
        return Some((*marker).to_string());
    }
    for word in value.split(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '`') {
        if token_like(word) {
            return Some("token_like_string".to_string());
        }
    }
    None
}

pub fn beta_route_path_only_reason(route: &str) -> Option<&'static str> {
    if route.contains("://") {
        return Some("scheme");
    }
    if route.contains('?') {
        return Some("query_string");
    }
    if route.contains('#') {
        return Some("fragment");
    }
    if route.chars().any(|c| c.is_whitespace()) {
        return Some("whitespace");
    }
    if !route.starts_with('/') {
        return Some("not_a_path");
    }
    None
}

fn clean_safe_text(
    field: &'static str,
    value: &str,
    max: usize,
) -> Result<String, BetaProgramError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(BetaProgramError::Missing(field));
    }
    if trimmed.chars().count() > max {
        return Err(BetaProgramError::InvalidValue {
            field: field.to_string(),
            reason: "too_long",
        });
    }
    if let Some(marker) = value_forbidden_marker(trimmed) {
        return Err(BetaProgramError::UnsafeValue {
            field: field.to_string(),
            marker,
        });
    }
    Ok(trimmed.to_string())
}

fn bool_string(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

pub fn build_safe_beta_enrollment(
    enrollment: &BetaEnrollment<'_>,
) -> Result<SafeBetaEnrollment, BetaProgramError> {
    let participant_id = clean_safe_text(
        "participant_id",
        enrollment.participant_id,
        SAFE_TEXT_MAX_CHARS,
    )?;
    let cohort_id = clean_safe_text("cohort_id", enrollment.cohort_id, SAFE_TEXT_MAX_CHARS)?;
    let role = clean_safe_text("role", enrollment.role, SAFE_TEXT_MAX_CHARS)?;
    let use_case = clean_safe_text("use_case", enrollment.use_case, SAFE_TEXT_MAX_CHARS)?;
    let expected_first_task = clean_safe_text(
        "expected_first_task",
        enrollment.expected_first_task,
        SAFE_TEXT_MAX_CHARS,
    )?;
    let support_owner = clean_safe_text(
        "support_owner",
        enrollment.support_owner,
        SAFE_TEXT_MAX_CHARS,
    )?;
    let risk_tier = clean_safe_text("risk_tier", enrollment.risk_tier, SAFE_TEXT_MAX_CHARS)?;
    if !BETA_RISK_TIERS.contains(&risk_tier.as_str()) {
        return Err(BetaProgramError::UnknownRiskTier(risk_tier));
    }
    let status = if enrollment.status.trim().is_empty() {
        "active".to_string()
    } else {
        clean_safe_text("status", enrollment.status, SAFE_TEXT_MAX_CHARS)?
    };
    if !BETA_ENROLLMENT_STATUSES.contains(&status.as_str()) {
        return Err(BetaProgramError::UnknownStatus(status));
    }
    Ok(SafeBetaEnrollment {
        participant_id,
        cohort_id,
        role,
        use_case,
        expected_first_task,
        support_owner,
        risk_tier,
        status,
        product_analytics_consent: enrollment.product_analytics_consent,
        follow_up_contact_consent: enrollment.follow_up_contact_consent,
    })
}

pub fn build_safe_beta_waitlist_request(
    request: &BetaWaitlistRequest<'_>,
) -> Result<SafeBetaWaitlistRequest, BetaProgramError> {
    Ok(SafeBetaWaitlistRequest {
        applicant_id: clean_safe_text("applicant_id", request.applicant_id, SAFE_TEXT_MAX_CHARS)?,
        email_hash: clean_safe_text("email_hash", request.email_hash, SAFE_TEXT_MAX_CHARS)?,
        role: clean_safe_text("role", request.role, SAFE_TEXT_MAX_CHARS)?,
        repo_ecosystem: clean_safe_text(
            "repo_ecosystem",
            request.repo_ecosystem,
            SAFE_TEXT_MAX_CHARS,
        )?,
        build_goal: clean_safe_text("build_goal", request.build_goal, SAFE_TEXT_MAX_CHARS)?,
        requested_cohort: clean_safe_text(
            "requested_cohort",
            request.requested_cohort,
            SAFE_TEXT_MAX_CHARS,
        )?,
        referral_source: clean_safe_text(
            "referral_source",
            request.referral_source,
            SAFE_TEXT_MAX_CHARS,
        )?,
        follow_up_contact_consent: request.follow_up_contact_consent,
    })
}

pub fn build_safe_beta_telemetry_event(
    event: &BetaTelemetryEvent,
) -> Result<SafeBetaTelemetryEvent, BetaProgramError> {
    let event_kind = clean_safe_text("type", &event.event_kind, SAFE_TEXT_MAX_CHARS)?;
    if !BETA_TELEMETRY_EVENT_KINDS.contains(&event_kind.as_str()) {
        return Err(BetaProgramError::UnknownTelemetryKind(event_kind));
    }
    if let Some(reason) = beta_route_path_only_reason(&event.route) {
        return Err(BetaProgramError::InvalidRoute(reason));
    }
    let route = clean_safe_text("route", &event.route, SAFE_TEXT_MAX_CHARS)?;
    let occurred_at = clean_safe_text("ts", &event.occurred_at, SAFE_TIMESTAMP_MAX_CHARS)?;
    let mut fields = BTreeMap::new();
    for (key, value) in &event.fields {
        if !TELEMETRY_SAFE_TEXT_FIELDS.contains(&key.as_str()) {
            return Err(BetaProgramError::InvalidValue {
                field: "field".to_string(),
                reason: "unknown_field",
            });
        }
        if matches!(key.as_str(), "type" | "route" | "ts") {
            continue;
        }
        fields.insert(
            key.clone(),
            clean_safe_text("field_value", value, SAFE_TEXT_MAX_CHARS)?,
        );
    }
    Ok(SafeBetaTelemetryEvent {
        operational_security_telemetry: BETA_OPERATIONAL_TELEMETRY_KINDS
            .contains(&event_kind.as_str()),
        event_kind,
        route,
        occurred_at,
        fields,
    })
}

fn auth_session_status(identity: &GovernedWriteIdentity) -> &'static str {
    if identity.identity_source == "local_demo_fixture" {
        "ACCEPTED_LOCAL_STUB"
    } else {
        "VERIFIED_TRUSTED_SESSION"
    }
}

fn correlation_for(
    kernel: &mut MdxKernel<impl StorageProvider>,
    tenant_id: &str,
    actor_id: &str,
    loop_id: &str,
) -> CorrelationIds {
    CorrelationIds {
        tenant_id: TenantId::new(tenant_id),
        trace_id: TraceId::new(kernel.ids.next("trace")),
        actor_id: ActorId::new(actor_id),
        loop_id: LoopId::new(loop_id),
        workflow_id: WorkflowId::new(kernel.ids.next("workflow")),
    }
}

fn start_run<S: StorageProvider>(
    kernel: &mut MdxKernel<S>,
    correlation: &CorrelationIds,
) -> String {
    let run_id = kernel.ids.next("run");
    kernel.storage.push_loop_run(LoopRun {
        run_id: run_id.clone(),
        loop_id: correlation.loop_id.clone(),
        agent_id: correlation.actor_id.clone(),
        workflow_id: correlation.workflow_id.clone(),
        status: "RUNNING".to_string(),
    });
    run_id
}

fn finish_run<S: StorageProvider>(kernel: &mut MdxKernel<S>, run_id: &str, status: &str) {
    if let Some(run) = kernel
        .storage
        .loop_runs_mut()
        .iter_mut()
        .find(|run| run.run_id == run_id)
    {
        run.status = status.to_string();
    }
}

fn payload_bool(value: bool) -> &'static str {
    bool_string(value)
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn record_beta_waitlist_request_with_identity(
        &mut self,
        request: &BetaWaitlistRequest<'_>,
        identity: &GovernedWriteIdentity,
        tenant_id: &str,
        actor_id: &str,
    ) -> Result<BetaWaitlistReport, BetaProgramError> {
        let safe = build_safe_beta_waitlist_request(request)?;
        let correlation = correlation_for(self, tenant_id, actor_id, "beta_waitlist");
        let run_id = start_run(self, &correlation);
        let decision = self.decide_with_receipt(&correlation, ActionKind::RecordBetaEnrollment);

        let receipt = self.transition_receipt(
            &run_id,
            "RECORD_BETA_WAITLIST_REQUEST",
            &correlation,
            &decision,
            BETA_WAITLIST_RECEIPT_KIND,
            payload(&[
                ("applicant_id", safe.applicant_id.as_str()),
                ("email_hash", safe.email_hash.as_str()),
                ("role", safe.role.as_str()),
                ("repo_ecosystem", safe.repo_ecosystem.as_str()),
                ("build_goal", safe.build_goal.as_str()),
                ("requested_cohort", safe.requested_cohort.as_str()),
                ("referral_source", safe.referral_source.as_str()),
                (
                    "follow_up_contact_consent",
                    payload_bool(safe.follow_up_contact_consent),
                ),
                ("identity_source", identity.identity_source.as_str()),
                ("identity_actor_kind", identity.actor_kind.as_str()),
                (
                    "identity_subject_actor_id",
                    identity.subject_actor_id.as_str(),
                ),
                ("identity_delegation_id", identity.delegation_id.as_str()),
                ("source_route", BETA_WAITLIST_ROUTE),
                ("projection_route", BETA_WAITLIST_PROJECTION_ROUTE),
                ("terminal_state", "BETA_WAITLIST_REQUEST_RECORDED"),
                ("production_write_allowed", "false"),
            ]),
        );
        finish_run(self, &run_id, "BETA_WAITLIST_REQUEST_RECORDED");

        Ok(BetaWaitlistReport {
            status: "BETA_WAITLIST_REQUEST_RECORDED",
            waitlist_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            applicant_id: safe.applicant_id,
            email_hash: safe.email_hash,
            role: safe.role,
            repo_ecosystem: safe.repo_ecosystem,
            build_goal: safe.build_goal,
            requested_cohort: safe.requested_cohort,
            referral_source: safe.referral_source,
            follow_up_contact_consent: safe.follow_up_contact_consent,
            production_write_allowed: false,
        })
    }

    pub fn beta_waitlist_projection(&self) -> BetaWaitlistProjection {
        let records = self
            .storage
            .ledger()
            .query()
            .by_kind(BETA_WAITLIST_RECEIPT_KIND)
            .into_iter()
            .map(|receipt| BetaWaitlistSnapshot {
                receipt_id: receipt.receipt_id.clone(),
                receipt_timestamp: receipt.receipt_timestamp.clone(),
                tenant_id: receipt.tenant_id.as_str().to_string(),
                actor_id: receipt.actor_id.as_str().to_string(),
                applicant_id: receipt
                    .payload
                    .get("applicant_id")
                    .cloned()
                    .unwrap_or_default(),
                email_hash: receipt
                    .payload
                    .get("email_hash")
                    .cloned()
                    .unwrap_or_default(),
                role: receipt.payload.get("role").cloned().unwrap_or_default(),
                repo_ecosystem: receipt
                    .payload
                    .get("repo_ecosystem")
                    .cloned()
                    .unwrap_or_default(),
                build_goal: receipt
                    .payload
                    .get("build_goal")
                    .cloned()
                    .unwrap_or_default(),
                requested_cohort: receipt
                    .payload
                    .get("requested_cohort")
                    .cloned()
                    .unwrap_or_default(),
                referral_source: receipt
                    .payload
                    .get("referral_source")
                    .cloned()
                    .unwrap_or_default(),
                follow_up_contact_consent: receipt
                    .payload
                    .get("follow_up_contact_consent")
                    .map(String::as_str)
                    == Some("true"),
            })
            .collect();
        BetaWaitlistProjection { records }
    }

    pub fn record_beta_enrollment_with_identity(
        &mut self,
        enrollment: &BetaEnrollment<'_>,
        identity: &GovernedWriteIdentity,
        tenant_id: &str,
        actor_id: &str,
    ) -> Result<BetaEnrollmentReport, BetaProgramError> {
        let safe = build_safe_beta_enrollment(enrollment)?;
        let auth_status = auth_session_status(identity);
        let correlation = correlation_for(self, tenant_id, actor_id, "beta_enrollment");
        let run_id = start_run(self, &correlation);
        let decision = self.decide_with_receipt(&correlation, ActionKind::RecordBetaEnrollment);

        let receipt = self.transition_receipt(
            &run_id,
            "RECORD_BETA_ENROLLMENT",
            &correlation,
            &decision,
            BETA_ENROLLMENT_RECEIPT_KIND,
            payload(&[
                ("participant_id", safe.participant_id.as_str()),
                ("participant_actor_id", actor_id),
                ("cohort_id", safe.cohort_id.as_str()),
                ("role", safe.role.as_str()),
                ("use_case", safe.use_case.as_str()),
                ("expected_first_task", safe.expected_first_task.as_str()),
                ("support_owner", safe.support_owner.as_str()),
                ("risk_tier", safe.risk_tier.as_str()),
                ("status", safe.status.as_str()),
                (
                    "product_analytics_consent",
                    payload_bool(safe.product_analytics_consent),
                ),
                (
                    "follow_up_contact_consent",
                    payload_bool(safe.follow_up_contact_consent),
                ),
                ("auth_session_status", auth_status),
                ("identity_source", identity.identity_source.as_str()),
                ("identity_actor_kind", identity.actor_kind.as_str()),
                (
                    "identity_subject_actor_id",
                    identity.subject_actor_id.as_str(),
                ),
                ("identity_delegation_id", identity.delegation_id.as_str()),
                ("source_route", BETA_ENROLLMENT_ROUTE),
                ("projection_route", BETA_ENROLLMENT_PROJECTION_ROUTE),
                ("terminal_state", "BETA_ENROLLMENT_RECORDED"),
                ("production_write_allowed", "false"),
            ]),
        );
        finish_run(self, &run_id, "BETA_ENROLLMENT_RECORDED");

        Ok(BetaEnrollmentReport {
            status: "BETA_ENROLLMENT_RECORDED",
            enrollment_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            auth_session_status: auth_status.to_string(),
            participant_id: safe.participant_id,
            participant_actor_id: actor_id.to_string(),
            cohort_id: safe.cohort_id,
            enrollment_status: safe.status,
            product_analytics_consent: safe.product_analytics_consent,
            follow_up_contact_consent: safe.follow_up_contact_consent,
            production_write_allowed: false,
        })
    }

    pub fn record_beta_cohort_halt_with_identity(
        &mut self,
        halt: &BetaCohortHalt<'_>,
        identity: &GovernedWriteIdentity,
        tenant_id: &str,
        actor_id: &str,
    ) -> Result<BetaCohortHaltReport, BetaProgramError> {
        let cohort_id = clean_safe_text("cohort_id", halt.cohort_id, SAFE_TEXT_MAX_CHARS)?;
        let reason = clean_safe_text("reason", halt.reason, SAFE_TEXT_MAX_CHARS)?;
        let active_count_text = halt.active_count.to_string();
        let cap_text = halt.cap.to_string();
        let auth_status = auth_session_status(identity);
        let correlation = correlation_for(self, tenant_id, actor_id, "beta_cohort_halt");
        let run_id = start_run(self, &correlation);
        let decision = self.decide_with_receipt(&correlation, ActionKind::RecordBetaEnrollment);

        let receipt = self.transition_receipt(
            &run_id,
            "RECORD_BETA_COHORT_HALT",
            &correlation,
            &decision,
            BETA_COHORT_HALT_RECEIPT_KIND,
            payload(&[
                ("cohort_id", cohort_id.as_str()),
                ("reason", reason.as_str()),
                ("active_count", active_count_text.as_str()),
                ("cap", cap_text.as_str()),
                ("auth_session_status", auth_status),
                ("identity_source", identity.identity_source.as_str()),
                ("identity_actor_kind", identity.actor_kind.as_str()),
                (
                    "identity_subject_actor_id",
                    identity.subject_actor_id.as_str(),
                ),
                ("identity_delegation_id", identity.delegation_id.as_str()),
                ("source_route", BETA_ENROLLMENT_ROUTE),
                ("projection_route", BETA_ENROLLMENT_PROJECTION_ROUTE),
                ("terminal_state", "BETA_COHORT_HALT_RECORDED"),
                ("production_write_allowed", "false"),
            ]),
        );
        finish_run(self, &run_id, "BETA_COHORT_HALT_RECORDED");

        Ok(BetaCohortHaltReport {
            status: "BETA_COHORT_HALT_RECORDED",
            cohort_halt_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            auth_session_status: auth_status.to_string(),
            cohort_id,
            reason,
            active_count: halt.active_count,
            cap: halt.cap,
            production_write_allowed: false,
        })
    }

    pub fn beta_enrollment_projection(&self) -> BetaEnrollmentProjection {
        let records: Vec<BetaEnrollmentSnapshot> = self
            .storage
            .ledger()
            .query()
            .by_kind(BETA_ENROLLMENT_RECEIPT_KIND)
            .into_iter()
            .map(|receipt| BetaEnrollmentSnapshot {
                receipt_id: receipt.receipt_id.clone(),
                receipt_timestamp: receipt.receipt_timestamp.clone(),
                tenant_id: receipt.tenant_id.as_str().to_string(),
                actor_id: receipt
                    .payload
                    .get("participant_actor_id")
                    .cloned()
                    .unwrap_or_else(|| receipt.actor_id.as_str().to_string()),
                participant_id: receipt
                    .payload
                    .get("participant_id")
                    .cloned()
                    .unwrap_or_default(),
                cohort_id: receipt
                    .payload
                    .get("cohort_id")
                    .cloned()
                    .unwrap_or_default(),
                role: receipt.payload.get("role").cloned().unwrap_or_default(),
                use_case: receipt.payload.get("use_case").cloned().unwrap_or_default(),
                expected_first_task: receipt
                    .payload
                    .get("expected_first_task")
                    .cloned()
                    .unwrap_or_default(),
                support_owner: receipt
                    .payload
                    .get("support_owner")
                    .cloned()
                    .unwrap_or_default(),
                risk_tier: receipt
                    .payload
                    .get("risk_tier")
                    .cloned()
                    .unwrap_or_default(),
                status: receipt.payload.get("status").cloned().unwrap_or_default(),
                product_analytics_consent: receipt
                    .payload
                    .get("product_analytics_consent")
                    .map(|value| value == "true")
                    .unwrap_or(false),
                follow_up_contact_consent: receipt
                    .payload
                    .get("follow_up_contact_consent")
                    .map(|value| value == "true")
                    .unwrap_or(false),
            })
            .collect();

        let mut latest_by_actor: BTreeMap<(String, String), BetaEnrollmentSnapshot> =
            BTreeMap::new();
        for record in &records {
            latest_by_actor.insert(
                (record.tenant_id.clone(), record.actor_id.clone()),
                record.clone(),
            );
        }
        let latest_active = latest_by_actor
            .into_values()
            .filter(|record| record.status == "active")
            .collect();
        BetaEnrollmentProjection {
            records,
            latest_active,
        }
    }

    pub fn latest_active_beta_enrollment(
        &self,
        tenant_id: &str,
        actor_id: &str,
    ) -> Option<BetaEnrollmentSnapshot> {
        self.beta_enrollment_projection()
            .latest_active
            .into_iter()
            .find(|record| record.tenant_id == tenant_id && record.actor_id == actor_id)
    }

    pub fn record_beta_telemetry_with_identity(
        &mut self,
        event: &BetaTelemetryEvent,
        identity: &GovernedWriteIdentity,
        tenant_id: &str,
        actor_id: &str,
    ) -> Result<BetaTelemetryReport, BetaProgramError> {
        let safe = build_safe_beta_telemetry_event(event)?;
        let enrollment = self.latest_active_beta_enrollment(tenant_id, actor_id);
        if enrollment.is_none() && !safe.operational_security_telemetry {
            return Err(BetaProgramError::MissingEnrollment);
        }
        let product_analytics_consent = enrollment
            .as_ref()
            .map(|enrollment| enrollment.product_analytics_consent)
            .unwrap_or(false);
        if !safe.operational_security_telemetry && !product_analytics_consent {
            return Err(BetaProgramError::ProductAnalyticsOptOut);
        }
        let cohort_id = enrollment
            .as_ref()
            .map(|enrollment| enrollment.cohort_id.clone())
            .unwrap_or_else(|| "unenrolled".to_string());
        let participant_actor_id = enrollment
            .as_ref()
            .map(|enrollment| enrollment.actor_id.clone())
            .unwrap_or_else(|| actor_id.to_string());
        let auth_status = auth_session_status(identity);
        let correlation = correlation_for(self, tenant_id, actor_id, "beta_telemetry");
        let run_id = start_run(self, &correlation);
        let decision = self.decide_with_receipt(&correlation, ActionKind::RecordBetaTelemetry);

        let mut receipt_payload = payload(&[
            ("event_kind", safe.event_kind.as_str()),
            ("route", safe.route.as_str()),
            ("occurred_at", safe.occurred_at.as_str()),
            ("cohort_id", cohort_id.as_str()),
            ("participant_actor_id", participant_actor_id.as_str()),
            (
                "product_analytics_consent_observed",
                payload_bool(product_analytics_consent),
            ),
            (
                "operational_security_telemetry",
                payload_bool(safe.operational_security_telemetry),
            ),
            ("auth_session_status", auth_status),
            ("identity_source", identity.identity_source.as_str()),
            ("identity_actor_kind", identity.actor_kind.as_str()),
            (
                "identity_subject_actor_id",
                identity.subject_actor_id.as_str(),
            ),
            ("identity_delegation_id", identity.delegation_id.as_str()),
            ("source_route", BETA_TELEMETRY_ROUTE),
            ("projection_route", BETA_TELEMETRY_PROJECTION_ROUTE),
            ("terminal_state", "BETA_TELEMETRY_RECORDED"),
            ("production_write_allowed", "false"),
        ]);
        for (key, value) in &safe.fields {
            receipt_payload.insert(format!("field.{key}"), value.clone());
        }

        let receipt = self.transition_receipt(
            &run_id,
            "RECORD_BETA_TELEMETRY",
            &correlation,
            &decision,
            BETA_TELEMETRY_RECEIPT_KIND,
            receipt_payload,
        );
        finish_run(self, &run_id, "BETA_TELEMETRY_RECORDED");

        Ok(BetaTelemetryReport {
            status: "BETA_TELEMETRY_RECORDED",
            telemetry_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            auth_session_status: auth_status.to_string(),
            event_kind: safe.event_kind,
            cohort_id,
            participant_actor_id,
            product_analytics_consent_observed: product_analytics_consent,
            operational_security_telemetry: safe.operational_security_telemetry,
            production_write_allowed: false,
        })
    }

    pub fn record_beta_telemetry_rejection_with_identity(
        &mut self,
        attempted_event_kind: &str,
        rejection_reason: &str,
        route: &str,
        identity: &GovernedWriteIdentity,
        tenant_id: &str,
        actor_id: &str,
    ) -> BetaTelemetryRejectionReport {
        let attempted_event_kind = safe_rejection_value(attempted_event_kind, "unknown");
        let rejection_reason = if BETA_TELEMETRY_REJECTION_REASONS.contains(&rejection_reason) {
            rejection_reason.to_string()
        } else {
            "invalid_value".to_string()
        };
        let route = if beta_route_path_only_reason(route).is_none()
            && value_forbidden_marker(route).is_none()
        {
            route.to_string()
        } else {
            String::new()
        };
        let enrollment = self.latest_active_beta_enrollment(tenant_id, actor_id);
        let cohort_id = enrollment
            .as_ref()
            .map(|enrollment| enrollment.cohort_id.clone())
            .unwrap_or_else(|| "unenrolled".to_string());
        let participant_actor_id = enrollment
            .as_ref()
            .map(|enrollment| enrollment.actor_id.clone())
            .unwrap_or_else(|| actor_id.to_string());
        let auth_status = auth_session_status(identity);
        let correlation = correlation_for(self, tenant_id, actor_id, "beta_telemetry_rejection");
        let run_id = start_run(self, &correlation);
        let decision = self.decide_with_receipt(&correlation, ActionKind::RejectBetaTelemetry);
        let receipt = self.transition_receipt(
            &run_id,
            "REJECT_BETA_TELEMETRY",
            &correlation,
            &decision,
            BETA_TELEMETRY_REJECTION_RECEIPT_KIND,
            payload(&[
                ("attempted_event_kind", attempted_event_kind.as_str()),
                ("rejection_reason", rejection_reason.as_str()),
                ("route", route.as_str()),
                ("cohort_id", cohort_id.as_str()),
                ("participant_actor_id", participant_actor_id.as_str()),
                ("auth_session_status", auth_status),
                ("identity_source", identity.identity_source.as_str()),
                ("identity_actor_kind", identity.actor_kind.as_str()),
                (
                    "identity_subject_actor_id",
                    identity.subject_actor_id.as_str(),
                ),
                ("identity_delegation_id", identity.delegation_id.as_str()),
                ("source_route", BETA_TELEMETRY_ROUTE),
                ("projection_route", BETA_TELEMETRY_PROJECTION_ROUTE),
                ("terminal_state", "BETA_TELEMETRY_REJECTED"),
                ("unsafe_payload_persisted", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        finish_run(self, &run_id, "BETA_TELEMETRY_REJECTED");

        BetaTelemetryRejectionReport {
            status: "BETA_TELEMETRY_REJECTED",
            rejection_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            auth_session_status: auth_status.to_string(),
            attempted_event_kind,
            rejection_reason,
            route,
            cohort_id,
            participant_actor_id,
            unsafe_payload_persisted: false,
            production_write_allowed: false,
        }
    }

    pub fn beta_telemetry_counts(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for receipt in self
            .storage
            .ledger()
            .query()
            .by_kind(BETA_TELEMETRY_RECEIPT_KIND)
        {
            let key = receipt
                .payload
                .get("event_kind")
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
            *counts.entry(key).or_insert(0) += 1;
        }
        counts
    }

    pub fn beta_telemetry_rejection_counts(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for receipt in self
            .storage
            .ledger()
            .query()
            .by_kind(BETA_TELEMETRY_REJECTION_RECEIPT_KIND)
        {
            let key = receipt
                .payload
                .get("rejection_reason")
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
            *counts.entry(key).or_insert(0) += 1;
        }
        counts
    }

    pub fn beta_telemetry_cohort_counts(&self) -> BTreeMap<String, usize> {
        let mut cohorts = BTreeSet::new();
        for record in self.beta_enrollment_projection().latest_active {
            cohorts.insert(record.cohort_id);
        }
        let mut counts = BTreeMap::new();
        for cohort in cohorts {
            counts.insert(cohort, 0);
        }
        for receipt in self
            .storage
            .ledger()
            .query()
            .by_kind(BETA_TELEMETRY_RECEIPT_KIND)
        {
            let key = receipt
                .payload
                .get("cohort_id")
                .cloned()
                .unwrap_or_else(|| "unenrolled".to_string());
            *counts.entry(key).or_insert(0) += 1;
        }
        counts
    }

    /// Receipt-backed activation and return state for each active participant.
    /// The projection contains only allowlisted telemetry fields and enrollment
    /// metadata. It never exposes raw client payloads or content.
    pub fn beta_participant_journey_projection(&self) -> BetaParticipantJourneyProjection {
        let mut journeys: BTreeMap<(String, String), BetaParticipantJourney> = self
            .beta_enrollment_projection()
            .latest_active
            .into_iter()
            .map(|enrollment| {
                let key = (enrollment.tenant_id.clone(), enrollment.actor_id.clone());
                let journey = BetaParticipantJourney {
                    tenant_id: enrollment.tenant_id,
                    participant_actor_id: enrollment.actor_id,
                    participant_id: enrollment.participant_id,
                    cohort_id: enrollment.cohort_id,
                    role: enrollment.role,
                    expected_first_task: enrollment.expected_first_task,
                    support_owner: enrollment.support_owner,
                    product_analytics_consent: enrollment.product_analytics_consent,
                    first_seen_at: String::new(),
                    last_seen_at: String::new(),
                    event_count: 0,
                    session_start_count: 0,
                    session_end_count: 0,
                    distinct_session_day_count: 0,
                    activation_steps: Vec::new(),
                    surface_visit_counts: BTreeMap::new(),
                    feedback_opened_count: 0,
                    first_value_observed: false,
                    return_session_observed: false,
                    last_app_version: String::new(),
                };
                (key, journey)
            })
            .collect();
        let mut session_days: BTreeMap<(String, String), BTreeSet<String>> = journeys
            .keys()
            .cloned()
            .map(|key| (key, BTreeSet::new()))
            .collect();

        for receipt in self
            .storage
            .ledger()
            .query()
            .by_kind(BETA_TELEMETRY_RECEIPT_KIND)
        {
            let actor_id = receipt
                .payload
                .get("participant_actor_id")
                .cloned()
                .unwrap_or_else(|| receipt.actor_id.as_str().to_string());
            let key = (receipt.tenant_id.as_str().to_string(), actor_id);
            let Some(journey) = journeys.get_mut(&key) else {
                continue;
            };
            let occurred_at = receipt
                .payload
                .get("occurred_at")
                .filter(|value| !value.is_empty())
                .cloned()
                .unwrap_or_else(|| receipt.receipt_timestamp.clone());
            if journey.first_seen_at.is_empty() || occurred_at < journey.first_seen_at {
                journey.first_seen_at = occurred_at.clone();
            }
            if journey.last_seen_at.is_empty() || occurred_at > journey.last_seen_at {
                journey.last_seen_at = occurred_at.clone();
            }
            journey.event_count += 1;

            match receipt.payload.get("event_kind").map(String::as_str) {
                Some("session_start") => {
                    journey.session_start_count += 1;
                    if let Some(day) = occurred_at.get(0..10)
                        && let Some(days) = session_days.get_mut(&key)
                    {
                        days.insert(day.to_string());
                        journey.distinct_session_day_count = days.len();
                    }
                }
                Some("session_end") => journey.session_end_count += 1,
                Some("activation_step") => {
                    let completed = receipt
                        .payload
                        .get("field.completed")
                        .map(|value| value == "true")
                        .unwrap_or(false);
                    let step = receipt
                        .payload
                        .get("field.step")
                        .cloned()
                        .unwrap_or_default();
                    if completed && !step.is_empty() && !journey.activation_steps.contains(&step) {
                        journey.activation_steps.push(step);
                    }
                }
                Some("surface_visit") => {
                    if let Some(surface) = receipt.payload.get("field.surface") {
                        *journey
                            .surface_visit_counts
                            .entry(surface.clone())
                            .or_insert(0) += 1;
                    }
                }
                Some("feedback_opened") => journey.feedback_opened_count += 1,
                _ => {}
            }
            if let Some(app_version) = receipt.payload.get("field.app_version") {
                // Native Mac builds use a numeric semantic version label. Web
                // releases use a named label and must not overwrite the last
                // Mac build observed for update/support operations.
                if app_version
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_ascii_digit())
                {
                    journey.last_app_version = app_version.clone();
                }
            }
            // A page reload is not a return. Require session starts on two
            // distinct UTC dates so this gate stays conservative and honest.
            journey.return_session_observed = journey.distinct_session_day_count >= 2;
        }

        let forge_first_value_owners: BTreeSet<(String, String)> = self
            .storage
            .ledger()
            .query()
            .by_kind("forge.run.event")
            .into_iter()
            .filter(|receipt| {
                receipt
                    .payload
                    .get("event")
                    .is_some_and(|event| event == "run_started" || event == "run_finished")
            })
            .filter_map(|receipt| {
                receipt
                    .payload
                    .get("owner_user_id")
                    .map(|owner| (receipt.tenant_id.as_str().to_string(), owner.clone()))
            })
            .collect();
        for (key, journey) in &mut journeys {
            journey.first_value_observed = journey
                .activation_steps
                .iter()
                .any(|step| step == "first_result_seen")
                && forge_first_value_owners.contains(key);
        }

        BetaParticipantJourneyProjection {
            journeys: journeys.into_values().collect(),
        }
    }
}

fn safe_rejection_value(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > SAFE_TEXT_MAX_CHARS
        || value_forbidden_marker(trimmed).is_some()
    {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryStorage;

    fn enrollment<'a>() -> BetaEnrollment<'a> {
        BetaEnrollment {
            participant_id: "engineer_1",
            cohort_id: "cohort_a",
            role: "backend",
            use_case: "forge",
            expected_first_task: "request_forge_recipe",
            support_owner: "beta_lead",
            risk_tier: "trusted",
            status: "active",
            product_analytics_consent: true,
            follow_up_contact_consent: true,
        }
    }

    #[test]
    fn enrollment_records_and_projects_latest_active() {
        let mut kernel = MdxKernel::<InMemoryStorage>::default();
        let identity = GovernedWriteIdentity::local_demo("engineer_1");
        let report = kernel
            .record_beta_enrollment_with_identity(
                &enrollment(),
                &identity,
                "tenant_local",
                "engineer_1",
            )
            .expect("enrollment should record");
        assert_eq!(report.status, "BETA_ENROLLMENT_RECORDED");
        let projection = kernel.beta_enrollment_projection();
        assert_eq!(projection.records.len(), 1);
        assert_eq!(projection.latest_active.len(), 1);
        assert_eq!(projection.latest_active[0].cohort_id, "cohort_a");
    }

    #[test]
    fn latest_active_wins_over_inactive_update() {
        let mut kernel = MdxKernel::<InMemoryStorage>::default();
        let identity = GovernedWriteIdentity::local_demo("engineer_1");
        kernel
            .record_beta_enrollment_with_identity(
                &enrollment(),
                &identity,
                "tenant_local",
                "engineer_1",
            )
            .unwrap();
        let inactive = BetaEnrollment {
            status: "inactive",
            ..enrollment()
        };
        kernel
            .record_beta_enrollment_with_identity(
                &inactive,
                &identity,
                "tenant_local",
                "engineer_1",
            )
            .unwrap();
        assert!(
            kernel
                .latest_active_beta_enrollment("tenant_local", "engineer_1")
                .is_none()
        );
    }

    #[test]
    fn product_analytics_telemetry_requires_enrollment_and_consent() {
        let mut kernel = MdxKernel::<InMemoryStorage>::default();
        let identity = GovernedWriteIdentity::local_demo("engineer_1");
        let event = BetaTelemetryEvent {
            event_kind: "activation_step".to_string(),
            route: "/forge".to_string(),
            occurred_at: "2026-06-21T00:00:00Z".to_string(),
            fields: BTreeMap::from([("step".to_string(), "first_forge_request".to_string())]),
        };
        assert_eq!(
            kernel.record_beta_telemetry_with_identity(
                &event,
                &identity,
                "tenant_local",
                "engineer_1",
            ),
            Err(BetaProgramError::MissingEnrollment)
        );
        kernel
            .record_beta_enrollment_with_identity(
                &enrollment(),
                &identity,
                "tenant_local",
                "engineer_1",
            )
            .unwrap();
        let report = kernel
            .record_beta_telemetry_with_identity(&event, &identity, "tenant_local", "engineer_1")
            .unwrap();
        assert_eq!(report.status, "BETA_TELEMETRY_RECORDED");
        assert_eq!(report.cohort_id, "cohort_a");
    }

    #[test]
    fn participant_journey_projects_activation_return_and_release_without_content() {
        let mut kernel = MdxKernel::<InMemoryStorage>::default();
        let identity = GovernedWriteIdentity::local_demo("engineer_1");
        kernel
            .record_beta_enrollment_with_identity(
                &enrollment(),
                &identity,
                "tenant_local",
                "engineer_1",
            )
            .unwrap();

        for event in [
            BetaTelemetryEvent {
                event_kind: "session_start".to_string(),
                route: "/home".to_string(),
                occurred_at: "2026-06-21T00:00:00Z".to_string(),
                fields: BTreeMap::from([("app_version".to_string(), "0.9.2".to_string())]),
            },
            BetaTelemetryEvent {
                event_kind: "activation_step".to_string(),
                route: "/forge".to_string(),
                occurred_at: "2026-06-21T00:02:00Z".to_string(),
                fields: BTreeMap::from([
                    ("step".to_string(), "first_result_seen".to_string()),
                    ("completed".to_string(), "true".to_string()),
                    ("app_version".to_string(), "0.9.2".to_string()),
                ]),
            },
            BetaTelemetryEvent {
                event_kind: "session_start".to_string(),
                route: "/home".to_string(),
                occurred_at: "2026-06-22T00:00:00Z".to_string(),
                fields: BTreeMap::from([("app_version".to_string(), "0.9.2".to_string())]),
            },
        ] {
            kernel
                .record_beta_telemetry_with_identity(
                    &event,
                    &identity,
                    "tenant_local",
                    "engineer_1",
                )
                .unwrap();
        }
        assert!(
            !kernel.beta_participant_journey_projection().journeys[0].first_value_observed,
            "a client milestone without a Forge receipt is not first value"
        );
        kernel
            .record_forge_run_event_with_identity(
                crate::ForgeRunEvent {
                    tenant_id: "tenant_local",
                    actor_id: "engineer_1",
                    run_id: "forge_run_1",
                    event: "run_started",
                    work_item_id: "first_value",
                    detail: "start",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
            )
            .unwrap();

        let projection = kernel.beta_participant_journey_projection();
        assert_eq!(projection.journeys.len(), 1);
        let journey = &projection.journeys[0];
        assert_eq!(journey.session_start_count, 2);
        assert_eq!(journey.distinct_session_day_count, 2);
        assert!(journey.first_value_observed);
        assert!(journey.return_session_observed);
        assert_eq!(journey.activation_steps, ["first_result_seen"]);
        assert_eq!(journey.last_app_version, "0.9.2");
        assert_eq!(journey.first_seen_at, "2026-06-21T00:00:00Z");
        assert_eq!(journey.last_seen_at, "2026-06-22T00:00:00Z");
    }

    #[test]
    fn operational_telemetry_can_record_without_product_consent() {
        let mut kernel = MdxKernel::<InMemoryStorage>::default();
        let identity = GovernedWriteIdentity::local_demo("engineer_1");
        let event = BetaTelemetryEvent {
            event_kind: "interaction_timing".to_string(),
            route: "/forge".to_string(),
            occurred_at: "2026-06-21T00:00:00Z".to_string(),
            fields: BTreeMap::from([("interaction_kind".to_string(), "approve".to_string())]),
        };
        let report = kernel
            .record_beta_telemetry_with_identity(&event, &identity, "tenant_local", "engineer_1")
            .unwrap();
        assert_eq!(report.cohort_id, "unenrolled");
        assert!(report.operational_security_telemetry);
    }

    #[test]
    fn rejection_records_only_safe_shape() {
        let mut kernel = MdxKernel::<InMemoryStorage>::default();
        let identity = GovernedWriteIdentity::local_demo("engineer_1");
        let report = kernel.record_beta_telemetry_rejection_with_identity(
            "activation_step",
            "unsafe_value",
            "/forge/sk-test-secret",
            &identity,
            "tenant_local",
            "engineer_1",
        );
        assert_eq!(report.status, "BETA_TELEMETRY_REJECTED");
        assert_eq!(report.route, "");
        let receipt = kernel
            .storage
            .ledger()
            .query()
            .by_kind(BETA_TELEMETRY_REJECTION_RECEIPT_KIND)
            .pop()
            .unwrap();
        assert_eq!(
            receipt.payload.get("unsafe_payload_persisted").unwrap(),
            "false"
        );
        assert!(
            !receipt
                .payload
                .values()
                .any(|value| value.contains("sk-test"))
        );
    }
}
