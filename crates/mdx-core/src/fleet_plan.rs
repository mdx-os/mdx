//! The fleet plan: one spec decomposed into parallel build streams. A
//! fleet run is Forge at scale - a planner proposes N streams, a human
//! ratifies the decomposition, and only then does any build spend start.
//!
//! The plan is the make-or-break artifact, so the kernel enforces its
//! laws at the door rather than hoping downstream:
//! - write scopes are DISJOINT across streams - within any scope, writes
//!   stay single-threaded (the conflict law);
//! - shared registration files belong to the integration phase alone
//!   (integration-owned paths) - streams declare, never edit them;
//! - the seams (interface contracts) are decided once, here, and ride
//!   into every stream's context verbatim;
//! - ratification is a human verb with a reason, and it mints the
//!   streams as real work items on the work plane.
//!
//! The kernel never calls a model and never parses JSON (mdx-core stays
//! zero-dependency): the server's planner parses its model's output into
//! typed streams; this rail validates them, records them as flat payload
//! fields it can read back itself, and refuses what would conflict.

use crate::strategy_board::BoardWrite;
use crate::work_plane::WorkItemShape;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider};
use std::collections::BTreeMap;

const MAX_SPEC_CHARS: usize = 8_000;
const MAX_OBJECTIVE_CHARS: usize = 1_500;
const MAX_CONTRACT_CHARS: usize = 1_500;
const MAX_FIELD_CHARS: usize = 500;
const MAX_REPO_CONTEXT_CHARS: usize = 2_000;
// A fleet can declare many parallel streams - serious, meaty work across as
// many teams as it needs. Sixty-four keeps the advertised 48-wide operating
// point inside the governed plan contract with room for a few integration
// seams. The conductor still runs them in bounded waves
// (MDX_FLEET_MAX_CONCURRENT, default 4) so scale never overruns the machine;
// this is the plan-size ceiling, not the concurrency one.
const MAX_STREAMS: usize = 64;
const MAX_SCOPE_PATHS: usize = 24;
/// Work-plane description cap (mirrors work_plane MAX_LONG_CHARS).
const MAX_WORK_DESCRIPTION_CHARS: usize = 2_000;

/// One validated stream of a fleet plan - the shape the fan-out later
/// builds a run packet from. Lists ride as newline-joined payload fields.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FleetStream {
    pub stream_id: String,
    pub objective: String,
    pub write_scope: Vec<String>,
    pub interface_contract: String,
    pub depends_on: Vec<String>,
    pub checks: Vec<String>,
    pub max_turns: u32,
    /// The model slot the planner cast for this stream - its choice of which
    /// coder should do this specific piece, by difficulty and cost. Empty is
    /// the default builder; a name (e.g. "OPUS") selects a configured slot.
    /// An unknown slot degrades safely to the default at run time.
    pub builder_slot: String,
    /// The data-sensitivity tier of the stream's work, as the planner
    /// classified it: "open", "internal", or "sensitive". A "sensitive"
    /// stream touches regulated, customer, secret, or confidential data and
    /// may only run on a sovereign coder; the conductor enforces this and
    /// refuses it on a frontier coder, so sensitive data never leaves. Empty
    /// is treated as "internal" (the safe default).
    pub data_sensitivity: String,
}

#[derive(Clone, Copy, Debug)]
pub struct FleetPlanDraft<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    /// Empty mints a new fleet id; a known id re-plans (latest draft wins).
    pub fleet_id: &'a str,
    /// The spec the plan decomposes - the human's what and why.
    pub spec: &'a str,
    /// The operator's goal, kept separate from planner instructions.
    pub goal: &'a str,
    /// The checks requested by the operator before planner augmentation.
    pub checks: &'a [String],
    /// Paths only the integration phase may edit (registration files,
    /// generated manifests). Disjoint from all stream scopes.
    pub integration_owned_paths: &'a [String],
    /// Commands the integrated branch must pass.
    pub full_suite_checks: &'a [String],
    /// The planner's argument for this stream count (scale effort to
    /// complexity - one stream is a valid answer).
    pub justification: &'a str,
    /// Operator-requested execution width. The planner may use fewer streams
    /// when the seams are not real, but must never exceed this ceiling.
    /// Zero means "not explicitly requested" and records the stream count.
    pub requested_width: u32,
    /// Which model drafted it, for the record. Empty for a manual plan.
    pub planner_model: &'a str,
    /// Optional bet this fleet serves; rides into the minted work items.
    pub bet_id: &'a str,
    /// The connected repo this plan was shaped against. Empty means MDx itself.
    pub repo_id: &'a str,
    /// Deterministic repo intelligence captured at planning time so fleet runs,
    /// review, and later evals do not rediscover the repo from zero.
    pub repo_primary_language: &'a str,
    pub language_pack_id: &'a str,
    pub repo_profile_suggested_checks: &'a str,
    pub repo_profile_artifact_patterns: &'a str,
    pub repo_profile_semantic_intelligence: &'a str,
    pub repo_profile_semantic_tool_readiness: &'a str,
    pub repo_profile_toolchain_readiness: &'a str,
    pub repo_profile_proof_plan_status: &'a str,
    pub repo_profile_proof_plan_summary: &'a str,
    pub repo_profile_source: &'a str,
    /// Wide fleets need a second senior read of the decomposition before
    /// execution. The server records that scrutiny here; the run route gates
    /// high-width starts on a ready verdict.
    pub wide_plan_review_required: bool,
    pub wide_plan_review_status: &'a str,
    pub wide_plan_review_reviewer_model: &'a str,
    pub wide_plan_review_verdict: &'a str,
    pub wide_plan_review_confidence: &'a str,
    pub wide_plan_review_concerns: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FleetPlanReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub fleet_id: String,
    pub stream_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FleetRatifyReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub fleet_id: String,
    /// One minted work item per stream, in stream order.
    pub work_item_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FleetPlanError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    BadStreams(String),
    ScopeConflict(String),
    ReviewNotReady(String),
    UnknownFleet(String),
    AlreadyRatified(String),
}

impl FleetPlanError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("a fleet plan needs {field}"),
            Self::TooLong(field, len, max) => {
                format!("the {field} is {len} characters; the limit is {max}")
            }
            Self::BadStreams(reason) => format!("the streams do not hold together: {reason}"),
            Self::ScopeConflict(reason) => {
                format!("two streams would write the same files - fix the split: {reason}")
            }
            Self::ReviewNotReady(reason) => {
                format!("the wide fleet review is not ready yet - {reason}")
            }
            Self::UnknownFleet(id) => format!("no plan drafted for {id} - draft one first"),
            Self::AlreadyRatified(id) => {
                format!("the plan for {id} is already ratified - start a new fleet to re-plan")
            }
        }
    }
}

/// Two scope entries conflict when equal or when one contains the other
/// (a path is a prefix of the other at a path boundary).
fn paths_overlap(a: &str, b: &str) -> bool {
    let a = a.trim_end_matches('/');
    let b = b.trim_end_matches('/');
    a == b || a.starts_with(&format!("{b}/")) || b.starts_with(&format!("{a}/"))
}

/// Validate a decomposition: well-formed streams, real dependencies, and
/// the conflict law - every scope disjoint from every other stream's and
/// from the integration-owned paths.
pub fn validate_fleet_streams(
    streams: &[FleetStream],
    integration_owned: &[String],
) -> Result<(), FleetPlanError> {
    if streams.is_empty() {
        return Err(FleetPlanError::BadStreams("no streams".to_string()));
    }
    if streams.len() > MAX_STREAMS {
        return Err(FleetPlanError::BadStreams(format!(
            "{} streams; the limit is {MAX_STREAMS}",
            streams.len()
        )));
    }
    for (index, stream) in streams.iter().enumerate() {
        let id = stream.stream_id.trim();
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(FleetPlanError::BadStreams(format!(
                "every stream needs a simple stream_id; got {:?}",
                stream.stream_id
            )));
        }
        if stream.objective.trim().is_empty() {
            return Err(FleetPlanError::BadStreams(format!(
                "stream {id} has no objective"
            )));
        }
        if stream.objective.chars().count() > MAX_OBJECTIVE_CHARS {
            return Err(FleetPlanError::TooLong(
                "stream objective",
                stream.objective.chars().count(),
                MAX_OBJECTIVE_CHARS,
            ));
        }
        if stream.interface_contract.chars().count() > MAX_CONTRACT_CHARS {
            return Err(FleetPlanError::TooLong(
                "interface contract",
                stream.interface_contract.chars().count(),
                MAX_CONTRACT_CHARS,
            ));
        }
        for check in &stream.checks {
            if check.contains('\n') {
                return Err(FleetPlanError::BadStreams(format!(
                    "stream {id} has a multi-line check - allowed commands must be single lines (the run matches them exactly)"
                )));
            }
        }
        if stream.write_scope.is_empty() {
            return Err(FleetPlanError::BadStreams(format!(
                "stream {id} declares no write_scope"
            )));
        }
        if stream.write_scope.len() > MAX_SCOPE_PATHS {
            return Err(FleetPlanError::BadStreams(format!(
                "stream {id} declares {} scope paths; the limit is {MAX_SCOPE_PATHS}",
                stream.write_scope.len()
            )));
        }
        if streams
            .iter()
            .skip(index + 1)
            .any(|other| other.stream_id.trim() == id)
        {
            return Err(FleetPlanError::BadStreams(format!(
                "duplicate stream_id {id}"
            )));
        }
        for dep in &stream.depends_on {
            if !streams
                .iter()
                .any(|other| other.stream_id.trim() == dep.trim())
            {
                return Err(FleetPlanError::BadStreams(format!(
                    "stream {id} depends on unknown stream {dep}"
                )));
            }
        }
        for path in &stream.write_scope {
            for other in streams.iter().skip(index + 1) {
                for other_path in &other.write_scope {
                    if paths_overlap(path, other_path) {
                        return Err(FleetPlanError::ScopeConflict(format!(
                            "{id} and {} both cover {path}",
                            other.stream_id
                        )));
                    }
                }
            }
            for owned in integration_owned {
                if paths_overlap(path, owned) {
                    return Err(FleetPlanError::ScopeConflict(format!(
                        "{id} writes {path}, which belongs to integration ({owned})"
                    )));
                }
            }
        }
    }
    Ok(())
}

/// The order integration merges stream branches: dependencies first
/// (topological by depends_on, declaration order as the tie-break). A
/// dependency cycle falls back to declaration order rather than hanging;
/// the merge is of disjoint scopes either way, so order only decides
/// which seam an integration conflict would surface at first.
pub fn fleet_integration_order(streams: &[FleetStream]) -> Vec<String> {
    let mut placed: Vec<String> = Vec::new();
    let mut remaining: Vec<&FleetStream> = streams.iter().collect();
    while !remaining.is_empty() {
        let ready = remaining.iter().position(|stream| {
            stream
                .depends_on
                .iter()
                .all(|dep| placed.iter().any(|done| done == dep.trim()))
        });
        match ready {
            Some(index) => placed.push(remaining.remove(index).stream_id.trim().to_string()),
            // A cycle: take the first remaining as declared and move on.
            None => placed.push(remaining.remove(0).stream_id.trim().to_string()),
        }
    }
    placed
}

/// Decode the streams a plan receipt recorded. The inverse of the flat
/// encoding in record_fleet_plan_draft; the fan-out (F2) reads plans
/// through this, so the encoding has exactly one writer and one reader.
pub fn fleet_plan_streams(plan: &BTreeMap<String, String>) -> Vec<FleetStream> {
    let count: usize = plan
        .get("stream_count")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    (0..count)
        .map(|index| {
            let field = |name: &str| {
                plan.get(&format!("stream_{index}_{name}"))
                    .cloned()
                    .unwrap_or_default()
            };
            let list = |name: &str| -> Vec<String> {
                field(name)
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
                    .collect()
            };
            FleetStream {
                stream_id: field("id"),
                objective: field("objective"),
                write_scope: list("write_scope"),
                interface_contract: field("contract"),
                depends_on: list("depends_on"),
                checks: list("checks"),
                max_turns: field("max_turns").parse().unwrap_or(0),
                builder_slot: field("builder_slot"),
                data_sensitivity: field("data_sensitivity"),
            }
        })
        .collect()
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn record_fleet_plan_draft(
        &mut self,
        request: FleetPlanDraft<'_>,
        streams: &[FleetStream],
    ) -> Result<FleetPlanReport, FleetPlanError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.record_fleet_plan_draft_with_identity(request, streams, &identity)
    }

    pub fn record_fleet_plan_draft_with_identity(
        &mut self,
        request: FleetPlanDraft<'_>,
        streams: &[FleetStream],
        identity: &GovernedWriteIdentity,
    ) -> Result<FleetPlanReport, FleetPlanError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("a spec", request.spec),
            ("a goal", request.goal),
            ("its justification", request.justification),
        ] {
            if value.trim().is_empty() {
                return Err(FleetPlanError::Missing(field));
            }
        }
        if request.spec.chars().count() > MAX_SPEC_CHARS {
            return Err(FleetPlanError::TooLong(
                "spec",
                request.spec.chars().count(),
                MAX_SPEC_CHARS,
            ));
        }
        for (field, value) in [
            ("justification", request.justification),
            ("planner_model", request.planner_model),
            ("bet_id", request.bet_id),
            ("repo_id", request.repo_id),
            ("repo_primary_language", request.repo_primary_language),
            ("language_pack_id", request.language_pack_id),
            (
                "repo_profile_proof_plan_status",
                request.repo_profile_proof_plan_status,
            ),
            ("repo_profile_source", request.repo_profile_source),
            ("wide_plan_review_status", request.wide_plan_review_status),
            (
                "wide_plan_review_reviewer_model",
                request.wide_plan_review_reviewer_model,
            ),
            ("wide_plan_review_verdict", request.wide_plan_review_verdict),
            (
                "wide_plan_review_confidence",
                request.wide_plan_review_confidence,
            ),
        ] {
            if value.chars().count() > MAX_FIELD_CHARS {
                return Err(FleetPlanError::TooLong(
                    field,
                    value.chars().count(),
                    MAX_FIELD_CHARS,
                ));
            }
        }
        for (field, value) in [
            (
                "repo_profile_suggested_checks",
                request.repo_profile_suggested_checks,
            ),
            (
                "repo_profile_artifact_patterns",
                request.repo_profile_artifact_patterns,
            ),
            (
                "repo_profile_semantic_intelligence",
                request.repo_profile_semantic_intelligence,
            ),
            (
                "repo_profile_semantic_tool_readiness",
                request.repo_profile_semantic_tool_readiness,
            ),
            (
                "repo_profile_toolchain_readiness",
                request.repo_profile_toolchain_readiness,
            ),
            (
                "repo_profile_proof_plan_summary",
                request.repo_profile_proof_plan_summary,
            ),
            (
                "wide_plan_review_concerns",
                request.wide_plan_review_concerns,
            ),
        ] {
            if value.chars().count() > MAX_REPO_CONTEXT_CHARS {
                return Err(FleetPlanError::TooLong(
                    field,
                    value.chars().count(),
                    MAX_REPO_CONTEXT_CHARS,
                ));
            }
        }
        validate_fleet_streams(streams, request.integration_owned_paths)?;
        if request.requested_width > 0 && streams.len() > request.requested_width as usize {
            return Err(FleetPlanError::BadStreams(format!(
                "{} streams exceeds requested_width={}",
                streams.len(),
                request.requested_width
            )));
        }
        let requested_width = if request.requested_width == 0 {
            streams.len()
        } else {
            request.requested_width as usize
        };

        let fleet_id = if request.fleet_id.trim().is_empty() {
            self.ids.next("fleet")
        } else {
            request.fleet_id.trim().to_string()
        };
        let mut fields: BTreeMap<String, String> = BTreeMap::new();
        fields.insert("fleet_id".to_string(), fleet_id.clone());
        fields.insert("spec".to_string(), request.spec.to_string());
        fields.insert("goal".to_string(), request.goal.to_string());
        fields.insert("checks".to_string(), request.checks.join("\n"));
        fields.insert(
            "integration_owned_paths".to_string(),
            request.integration_owned_paths.join("\n"),
        );
        fields.insert(
            "full_suite_checks".to_string(),
            request.full_suite_checks.join("\n"),
        );
        fields.insert(
            "justification".to_string(),
            request.justification.to_string(),
        );
        fields.insert(
            "planner_model".to_string(),
            request.planner_model.to_string(),
        );
        fields.insert("bet_id".to_string(), request.bet_id.to_string());
        fields.insert("repo_id".to_string(), request.repo_id.to_string());
        fields.insert(
            "repo_primary_language".to_string(),
            request.repo_primary_language.to_string(),
        );
        fields.insert(
            "language_pack_id".to_string(),
            request.language_pack_id.to_string(),
        );
        fields.insert(
            "repo_profile_suggested_checks".to_string(),
            request.repo_profile_suggested_checks.to_string(),
        );
        fields.insert(
            "repo_profile_artifact_patterns".to_string(),
            request.repo_profile_artifact_patterns.to_string(),
        );
        fields.insert(
            "repo_profile_semantic_intelligence".to_string(),
            request.repo_profile_semantic_intelligence.to_string(),
        );
        fields.insert(
            "repo_profile_semantic_tool_readiness".to_string(),
            request.repo_profile_semantic_tool_readiness.to_string(),
        );
        fields.insert(
            "repo_profile_toolchain_readiness".to_string(),
            request.repo_profile_toolchain_readiness.to_string(),
        );
        fields.insert(
            "repo_profile_proof_plan_status".to_string(),
            request.repo_profile_proof_plan_status.to_string(),
        );
        fields.insert(
            "repo_profile_proof_plan_summary".to_string(),
            request.repo_profile_proof_plan_summary.to_string(),
        );
        fields.insert(
            "repo_profile_source".to_string(),
            request.repo_profile_source.to_string(),
        );
        fields.insert(
            "wide_plan_review_required".to_string(),
            request.wide_plan_review_required.to_string(),
        );
        fields.insert(
            "wide_plan_review_status".to_string(),
            request.wide_plan_review_status.to_string(),
        );
        fields.insert(
            "wide_plan_review_reviewer_model".to_string(),
            request.wide_plan_review_reviewer_model.to_string(),
        );
        fields.insert(
            "wide_plan_review_verdict".to_string(),
            request.wide_plan_review_verdict.to_string(),
        );
        fields.insert(
            "wide_plan_review_confidence".to_string(),
            request.wide_plan_review_confidence.to_string(),
        );
        fields.insert(
            "wide_plan_review_concerns".to_string(),
            request.wide_plan_review_concerns.to_string(),
        );
        fields.insert("stream_count".to_string(), streams.len().to_string());
        fields.insert("requested_width".to_string(), requested_width.to_string());
        for (index, stream) in streams.iter().enumerate() {
            let mut put = |name: &str, value: String| {
                fields.insert(format!("stream_{index}_{name}"), value);
            };
            put("id", stream.stream_id.trim().to_string());
            put("objective", stream.objective.clone());
            put("write_scope", stream.write_scope.join("\n"));
            put("contract", stream.interface_contract.clone());
            put("depends_on", stream.depends_on.join("\n"));
            put("checks", stream.checks.join("\n"));
            put("max_turns", stream.max_turns.to_string());
            put("builder_slot", stream.builder_slot.trim().to_string());
            put(
                "data_sensitivity",
                stream.data_sensitivity.trim().to_string(),
            );
        }
        fields.insert(
            "identity_source".to_string(),
            identity.identity_source.clone(),
        );
        fields.insert("authority_opened".to_string(), "none".to_string());
        fields.insert("production_write_allowed".to_string(), "false".to_string());

        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "fleet_plan",
                action: ActionKind::RecordFleetPlan,
                transition: "RECORD_FLEET_PLAN_DRAFT",
                kind: "fleet.plan.drafted",
            },
            fields,
        );
        Ok(FleetPlanReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            fleet_id,
            stream_count: streams.len(),
        })
    }

    /// The latest drafted plan for a fleet, as its raw payload fields.
    /// None when nothing was drafted.
    pub fn latest_fleet_plan(&self, fleet_id: &str) -> Option<BTreeMap<String, String>> {
        let fleet_id = fleet_id.trim();
        if fleet_id.is_empty() {
            return None;
        }
        let mut latest = None;
        for receipt in self.ledger().query().by_kind("fleet.plan.drafted").iter() {
            if receipt.payload.get("fleet_id").map(String::as_str) == Some(fleet_id) {
                latest = Some(receipt.payload.clone());
            }
        }
        latest
    }

    pub fn fleet_plan_ratified(&self, fleet_id: &str) -> bool {
        self.ledger()
            .query()
            .by_kind("fleet.plan.ratified")
            .iter()
            .any(|receipt| {
                receipt.payload.get("fleet_id").map(String::as_str) == Some(fleet_id.trim())
            })
    }

    /// The human yes on a decomposition. Re-validates the conflict law
    /// against the latest draft, records the ratification, and mints one
    /// work item per stream so the streams are real work on the plane.
    pub fn ratify_fleet_plan_with_identity(
        &mut self,
        tenant_id: &str,
        actor_id: &str,
        fleet_id: &str,
        reason: &str,
        identity: &GovernedWriteIdentity,
    ) -> Result<FleetRatifyReport, FleetPlanError> {
        for (field, value) in [
            ("tenant_id", tenant_id),
            ("actor_id", actor_id),
            ("the fleet to ratify", fleet_id),
            ("your reason", reason),
        ] {
            if value.trim().is_empty() {
                return Err(FleetPlanError::Missing(field));
            }
        }
        let fleet_id = fleet_id.trim();
        let Some(plan) = self.latest_fleet_plan(fleet_id) else {
            return Err(FleetPlanError::UnknownFleet(fleet_id.to_string()));
        };
        if self.fleet_plan_ratified(fleet_id) {
            return Err(FleetPlanError::AlreadyRatified(fleet_id.to_string()));
        }
        if plan.get("wide_plan_review_required").map(String::as_str) == Some("true") {
            let status = plan
                .get("wide_plan_review_status")
                .map(String::as_str)
                .unwrap_or("");
            let verdict = plan
                .get("wide_plan_review_verdict")
                .map(String::as_str)
                .unwrap_or("");
            let review_ready = status == "recorded"
                && (verdict.eq_ignore_ascii_case("ready")
                    || verdict.eq_ignore_ascii_case("approve")
                    || verdict.eq_ignore_ascii_case("approved"));
            if !review_ready {
                return Err(FleetPlanError::ReviewNotReady(
                    "wait for advisor review, connect a reviewer, or re-plan with a narrower fleet"
                        .to_string(),
                ));
            }
        }
        let streams = fleet_plan_streams(&plan);
        let integration_owned: Vec<String> = plan
            .get("integration_owned_paths")
            .map(|value| {
                value
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        validate_fleet_streams(&streams, &integration_owned)?;
        let bet_id = plan.get("bet_id").cloned().unwrap_or_default();

        let mut fields: BTreeMap<String, String> = BTreeMap::new();
        fields.insert("fleet_id".to_string(), fleet_id.to_string());
        fields.insert("reason".to_string(), reason.to_string());
        fields.insert("stream_count".to_string(), streams.len().to_string());
        fields.insert(
            "identity_source".to_string(),
            identity.identity_source.clone(),
        );
        fields.insert("authority_opened".to_string(), "none".to_string());
        fields.insert("production_write_allowed".to_string(), "false".to_string());
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id,
                actor_id,
                loop_id: "fleet_plan",
                action: ActionKind::RatifyFleetPlan,
                transition: "RATIFY_FLEET_PLAN",
                kind: "fleet.plan.ratified",
            },
            fields,
        );

        let mut work_item_ids = Vec::new();
        for stream in &streams {
            let work_item_id = format!("work_item_{fleet_id}_{}", stream.stream_id);
            let title: String = stream.objective.chars().take(140).collect();
            let description: String = if stream.interface_contract.is_empty() {
                stream.objective.clone()
            } else {
                format!(
                    "{}\n\nInterface contract:\n{}",
                    stream.objective, stream.interface_contract
                )
            }
            .chars()
            .take(MAX_WORK_DESCRIPTION_CHARS)
            .collect();
            let minted = self.save_work_item_local_with_identity(
                WorkItemShape {
                    tenant_id,
                    actor_id,
                    work_item_id: &work_item_id,
                    title: &title,
                    description: &description,
                    bet_id: &bet_id,
                    owner_id: "",
                    approval: "full_signoff",
                    page_ref: "",
                    build_ref: fleet_id,
                },
                identity,
            );
            if minted.is_ok() {
                work_item_ids.push(work_item_id);
            }
        }
        Ok(FleetRatifyReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            fleet_id: fleet_id.to_string(),
            work_item_ids,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(id: &str, objective: &str, scope: &[&str]) -> FleetStream {
        FleetStream {
            stream_id: id.to_string(),
            objective: objective.to_string(),
            write_scope: scope.iter().map(|s| s.to_string()).collect(),
            interface_contract: String::new(),
            depends_on: Vec::new(),
            checks: Vec::new(),
            max_turns: 30,
            builder_slot: String::new(),
            data_sensitivity: String::new(),
        }
    }

    fn owned() -> Vec<String> {
        vec![
            "crates/mdx-core/src/lib.rs".to_string(),
            "crates/mdx-server/src/main.rs".to_string(),
            "generated/".to_string(),
        ]
    }

    fn draft<'a>(fleet_id: &'a str, integration_owned: &'a [String]) -> FleetPlanDraft<'a> {
        FleetPlanDraft {
            tenant_id: "t",
            actor_id: "human:founder",
            fleet_id,
            spec: "Build the foo surface end to end.",
            goal: "Build the foo surface end to end.",
            checks: &[],
            integration_owned_paths: integration_owned,
            full_suite_checks: &[],
            justification: "Two naturally disjoint layers; one stream would serialize them.",
            requested_width: 0,
            planner_model: "test_planner",
            bet_id: "",
            repo_id: "",
            repo_primary_language: "",
            language_pack_id: "",
            repo_profile_suggested_checks: "",
            repo_profile_artifact_patterns: "",
            repo_profile_semantic_intelligence: "",
            repo_profile_semantic_tool_readiness: "",
            repo_profile_toolchain_readiness: "",
            repo_profile_proof_plan_status: "",
            repo_profile_proof_plan_summary: "",
            repo_profile_source: "",
            wide_plan_review_required: false,
            wide_plan_review_status: "not_required",
            wide_plan_review_reviewer_model: "",
            wide_plan_review_verdict: "not_required",
            wide_plan_review_confidence: "none",
            wide_plan_review_concerns: "",
        }
    }

    #[test]
    fn a_valid_plan_drafts_round_trips_and_ratification_mints_work_items() {
        let mut kernel = MdxKernel::boot_local();
        let integration_owned = owned();
        let mut kernel_stream = stream(
            "s1_kernel",
            "Add the rail in mdx-core",
            &["crates/mdx-core/src/foo.rs"],
        );
        kernel_stream.interface_contract = "pub fn foo() -> Foo".to_string();
        kernel_stream.checks = vec!["cargo test -p mdx-core foo".to_string()];
        let mut route_stream = stream(
            "s2_route",
            "Serve the rail over HTTP",
            &["crates/mdx-server/src/foo_route.rs"],
        );
        route_stream.depends_on = vec!["s1_kernel".to_string()];
        let streams = vec![kernel_stream, route_stream];

        let report = kernel
            .record_fleet_plan_draft(draft("", &integration_owned), &streams)
            .expect("draft");
        assert_eq!(report.stream_count, 2);
        assert!(report.fleet_id.starts_with("fleet"));

        // The flat encoding round-trips exactly - one writer, one reader.
        let plan = kernel.latest_fleet_plan(&report.fleet_id).expect("plan");
        assert_eq!(
            plan.get("goal").map(String::as_str),
            Some("Build the foo surface end to end.")
        );
        assert_eq!(plan.get("checks").map(String::as_str), Some(""));
        let decoded = fleet_plan_streams(&plan);
        assert_eq!(decoded, streams);
        assert!(!kernel.fleet_plan_ratified(&report.fleet_id));

        let identity = GovernedWriteIdentity::local_demo("human:founder");
        let ratified = kernel
            .ratify_fleet_plan_with_identity(
                "t",
                "human:founder",
                &report.fleet_id,
                "Seams are right; go.",
                &identity,
            )
            .expect("ratify");
        assert_eq!(ratified.work_item_ids.len(), 2);
        assert!(ratified.work_item_ids[0].contains("s1_kernel"));
        assert!(kernel.fleet_plan_ratified(&report.fleet_id));

        // A second ratification is refused - one yes per plan.
        let again = kernel.ratify_fleet_plan_with_identity(
            "t",
            "human:founder",
            &report.fleet_id,
            "again",
            &identity,
        );
        assert!(matches!(again, Err(FleetPlanError::AlreadyRatified(_))));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn wide_plan_ratification_waits_for_recorded_review() {
        let mut kernel = MdxKernel::boot_local();
        let integration_owned = owned();
        let streams = vec![stream(
            "s1_kernel",
            "Add the rail in mdx-core",
            &["crates/mdx-core/src/foo.rs"],
        )];
        let mut draft = draft("", &integration_owned);
        draft.requested_width = 8;
        draft.wide_plan_review_required = true;
        draft.wide_plan_review_status = "review_unavailable";
        draft.wide_plan_review_reviewer_model = "gpt-5";
        draft.wide_plan_review_verdict = "not_recorded";
        let report = kernel
            .record_fleet_plan_draft(draft, &streams)
            .expect("draft");
        let identity = GovernedWriteIdentity::local_demo("human:founder");

        let refused = kernel.ratify_fleet_plan_with_identity(
            "t",
            "human:founder",
            &report.fleet_id,
            "Seams look good.",
            &identity,
        );

        assert!(matches!(refused, Err(FleetPlanError::ReviewNotReady(_))));
        assert!(!kernel.fleet_plan_ratified(&report.fleet_id));
    }

    #[test]
    fn a_planners_per_stream_model_casting_round_trips() {
        let mut kernel = MdxKernel::boot_local();
        let integration_owned = owned();
        // The planner cast the hard stream to OPUS and left the easy one on
        // the default coder - the casting must survive the flat encoding so
        // the conductor runs each stream under the model the plan chose.
        let mut hard = stream(
            "s1_hard",
            "Cross-cutting change across the kernel",
            &["crates/mdx-core/src/foo.rs"],
        );
        hard.builder_slot = "OPUS".to_string();
        hard.data_sensitivity = "sensitive".to_string();
        let easy = stream(
            "s2_easy",
            "A self-contained leaf module",
            &["crates/mdx-core/src/bar.rs"],
        );
        let streams = vec![hard, easy];
        let report = kernel
            .record_fleet_plan_draft(draft("", &integration_owned), &streams)
            .expect("draft");
        let plan = kernel.latest_fleet_plan(&report.fleet_id).expect("plan");
        let decoded = fleet_plan_streams(&plan);
        assert_eq!(decoded[0].builder_slot, "OPUS");
        assert_eq!(decoded[0].data_sensitivity, "sensitive");
        assert_eq!(decoded[1].builder_slot, "");
        assert_eq!(decoded, streams);
    }

    #[test]
    fn the_conflict_law_refuses_overlap_containment_and_integration_owned_paths() {
        let integration_owned = owned();
        // Exact overlap.
        let refused = validate_fleet_streams(
            &[
                stream("a", "x", &["crates/mdx-core/src/foo.rs"]),
                stream("b", "y", &["crates/mdx-core/src/foo.rs"]),
            ],
            &integration_owned,
        );
        assert!(matches!(refused, Err(FleetPlanError::ScopeConflict(_))));

        // Containment: a directory swallowing a neighbor's file.
        let refused = validate_fleet_streams(
            &[
                stream("a", "x", &["crates/mdx-core/src/"]),
                stream("b", "y", &["crates/mdx-core/src/foo.rs"]),
            ],
            &integration_owned,
        );
        assert!(matches!(refused, Err(FleetPlanError::ScopeConflict(_))));

        // A stream may not claim integration-owned paths.
        let refused = validate_fleet_streams(
            &[stream("a", "x", &["crates/mdx-core/src/lib.rs"])],
            &integration_owned,
        );
        assert!(matches!(refused, Err(FleetPlanError::ScopeConflict(_))));
        let refused = validate_fleet_streams(
            &[stream("a", "x", &["generated/routes/foo.json"])],
            &integration_owned,
        );
        assert!(matches!(refused, Err(FleetPlanError::ScopeConflict(_))));

        // Sibling files sharing a name prefix are NOT a conflict.
        let fine = validate_fleet_streams(
            &[
                stream("a", "x", &["crates/mdx-core/src/foo.rs"]),
                stream("b", "y", &["crates/mdx-core/src/foo_extra.rs"]),
            ],
            &integration_owned,
        );
        assert!(fine.is_ok());
    }

    #[test]
    fn the_plan_contract_accepts_the_advertised_48_wide_fleet() {
        let streams = (1..=48)
            .map(|index| {
                stream(
                    &format!("lane_{index:02}"),
                    "Create one isolated fleet proof",
                    &[&format!("docs/lane-{index:02}.md")],
                )
            })
            .collect::<Vec<_>>();

        assert!(validate_fleet_streams(&streams, &[]).is_ok());

        let over_limit = (1..=65)
            .map(|index| {
                stream(
                    &format!("lane_{index:02}"),
                    "Create one isolated fleet proof",
                    &[&format!("docs/overflow-{index:02}.md")],
                )
            })
            .collect::<Vec<_>>();
        let error = validate_fleet_streams(&over_limit, &[]).expect_err("65 exceeds ceiling");
        assert!(error.message().contains("65 streams; the limit is 64"));
    }

    #[test]
    fn integration_order_puts_dependencies_first_and_survives_cycles() {
        let mut a = stream("a", "x", &["pa"]);
        a.depends_on = vec!["c".to_string()];
        let b = stream("b", "y", &["pb"]);
        let c = stream("c", "z", &["pc"]);
        // a depends on c: c must merge before a; b keeps declaration order.
        assert_eq!(
            fleet_integration_order(&[a.clone(), b.clone(), c.clone()]),
            vec!["b", "c", "a"]
        );
        // A cycle does not hang - declaration order breaks it.
        let mut x = stream("x", "x", &["px"]);
        x.depends_on = vec!["y".to_string()];
        let mut y = stream("y", "y", &["py"]);
        y.depends_on = vec!["x".to_string()];
        assert_eq!(fleet_integration_order(&[x, y]), vec!["x", "y"]);
    }

    #[test]
    fn malformed_streams_and_unknown_fleets_are_refused_with_reasons() {
        let mut kernel = MdxKernel::boot_local();
        let integration_owned = owned();
        for (label, bad) in [
            ("empty", Vec::new()),
            ("no objective", vec![stream("a", " ", &["p"])]),
            ("no scope", vec![stream("a", "x", &[])]),
            ("bad id", vec![stream("has spaces", "x", &["p"])]),
            (
                "duplicate ids",
                vec![stream("a", "x", &["p"]), stream("a", "y", &["q"])],
            ),
            ("ghost dependency", {
                let mut s = stream("a", "x", &["p"]);
                s.depends_on = vec!["ghost".to_string()];
                vec![s]
            }),
        ] {
            let refused = validate_fleet_streams(&bad, &integration_owned);
            assert!(
                matches!(refused, Err(FleetPlanError::BadStreams(_))),
                "expected BadStreams for {label}"
            );
        }
        let identity = GovernedWriteIdentity::local_demo("human:founder");
        let missing = kernel.ratify_fleet_plan_with_identity(
            "t",
            "human:founder",
            "fleet_ghost",
            "why",
            &identity,
        );
        assert!(matches!(missing, Err(FleetPlanError::UnknownFleet(_))));
    }
}
