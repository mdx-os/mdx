//! The Marketplace skill library: procedural memory distilled from verified
//! Forge trajectories and promoted, under governance, into a reusable
//! capability. This is the third promotion target beside the advisory-text
//! learning loop and the surface memory store, and it is the highest-yield
//! compounding rail in the literature (Voyager's skill library, AWM and
//! ReasoningBank). What makes MDx's version different from every research
//! demo is that each step of the promotion carries a hash-chained receipt:
//!
//!   verified successful run  ->  marketplace.skill.drafted  ->  marketplace.skill.ratified
//!
//! A DRAFT may only be minted from a run the ledger already proved green (a
//! forge.run.event run_finished with RUN_FINISHED_DONE that also carries a
//! check_passed event), and its procedure is assembled deterministically from
//! that run's own trajectory events - never invented. A RATIFICATION must be
//! signed by an actor DISTINCT from the drafter and must cite the draft, so a
//! ratified distilled skill is provably traceable back to a real run. No
//! promotion here opens execution authority, secret access, inherited agent
//! permissions, adaptation, or a production write: a distilled skill is an
//! advisory procedure, cited like a lesson.

use crate::learning_memory_promotion::LearningMemoryApplicability;
use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

pub const MARKETPLACE_SKILL_DRAFTED_KIND: &str = "marketplace.skill.drafted";
pub const MARKETPLACE_SKILL_RATIFIED_KIND: &str = "marketplace.skill.ratified";

/// Where the stored procedure text came from. Deterministic assembly from the
/// run's own trajectory is the guaranteed floor; a model pass may sharpen the
/// prose, and the receipt says which one the reader holds. Mirrors the
/// forge-outcome lesson_source claim guard.
pub const MARKETPLACE_SKILL_PROCEDURE_SOURCES: &[&str] = &["deterministic", "model_distilled"];

/// The sensitivity classes a distilled skill can carry. A sensitive skill is
/// boundary-locked: reuse refuses to inject it into a run whose applicability
/// does not explicitly name it, so sensitive trajectory content never rides
/// into an unrelated run.
pub const MARKETPLACE_SKILL_SENSITIVITIES: &[&str] = &["normal", "sensitive"];

const MAX_TEXT_CHARS: usize = 2000;
const MAX_PROCEDURE_STEPS: usize = 40;

/// A DRAFT candidate distilled from one verified Forge run.
#[derive(Clone, Copy, Debug)]
pub struct MarketplaceSkillDraft<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    /// The run_finished forge.run.event receipt that proved this run green.
    pub source_run_receipt_id: &'a str,
    pub skill_id: &'a str,
    pub skill_name: &'a str,
    pub summary: &'a str,
    pub helps_with: &'a str,
    /// Optional model-sharpened procedure prose. Empty means the kernel stores
    /// the deterministic assembly verbatim.
    pub model_procedure: &'a str,
    /// Empty or "deterministic" stores the deterministic floor; "model_distilled"
    /// stores the sharpened prose under the claim guard.
    pub procedure_source: &'a str,
    /// Empty means "normal".
    pub sensitivity: &'a str,
}

/// The distinct-actor promotion of a DRAFT into a ratified distilled capability.
#[derive(Clone, Copy, Debug)]
pub struct MarketplaceSkillRatification<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub skill_id: &'a str,
    /// The marketplace.skill.drafted receipt this ratification promotes.
    pub draft_receipt_id: &'a str,
    /// "ratify" or "decline".
    pub decision: &'a str,
    pub note: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketplaceSkillDraftReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub skill_id: String,
    pub skill_state: String,
    pub source_run_id: String,
    pub procedure_source: String,
    pub procedure_step_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketplaceSkillRatificationReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub skill_id: String,
    pub capability_state: String,
    pub drafted_by: String,
    pub ratified_by: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarketplaceSkillError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    UnknownProcedureSource(String),
    UnknownSensitivity(String),
    UnknownSourceRun(String),
    SourceRunNotGreen(String),
    SourceRunNoProof(String),
    ModelProcedureRejected(String),
    UnknownDraft(String),
    WrongDraftKind(String, String),
    SelfRatification(String),
    UnknownDecision(String),
}

impl MarketplaceSkillError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("marketplace skill missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("the {field} is {len} characters; the limit is {max}")
            }
            Self::UnknownProcedureSource(other) => format!(
                "marketplace skill procedure_source must be one of {}; got {other}",
                MARKETPLACE_SKILL_PROCEDURE_SOURCES.join(", ")
            ),
            Self::UnknownSensitivity(other) => format!(
                "marketplace skill sensitivity must be one of {}; got {other}",
                MARKETPLACE_SKILL_SENSITIVITIES.join(", ")
            ),
            Self::UnknownSourceRun(receipt_id) => {
                format!("marketplace skill source run receipt {receipt_id} is unknown")
            }
            Self::SourceRunNotGreen(receipt_id) => format!(
                "marketplace skill source run receipt {receipt_id} is not a proven-green run_finished event"
            ),
            Self::SourceRunNoProof(run_id) => format!(
                "marketplace skill source run {run_id} has no check_passed event, so it is not verified"
            ),
            Self::ModelProcedureRejected(reason) => {
                format!("marketplace skill model procedure rejected: {reason}")
            }
            Self::UnknownDraft(receipt_id) => {
                format!("marketplace skill draft receipt {receipt_id} is unknown")
            }
            Self::WrongDraftKind(receipt_id, kind) => format!(
                "marketplace skill draft receipt {receipt_id} has kind {kind}; expected {MARKETPLACE_SKILL_DRAFTED_KIND}"
            ),
            Self::SelfRatification(actor) => format!(
                "marketplace skill ratification requires a distinct actor: {actor} drafted this skill and cannot ratify it"
            ),
            Self::UnknownDecision(other) => {
                format!(
                    "marketplace skill ratification decision must be ratify or decline, not {other}"
                )
            }
        }
    }
}

/// One distilled skill folded from the ledger for reuse and for the surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DistilledSkillView {
    pub skill_id: String,
    pub skill_name: String,
    pub summary: String,
    pub helps_with: String,
    pub procedure: String,
    pub procedure_source: String,
    pub procedure_step_count: usize,
    pub sensitivity: String,
    pub boundary_locked: bool,
    pub applicability_work_tiers: String,
    pub applicability_language_packs: String,
    /// "drafted", "ratified", or "declined".
    pub state: String,
    pub source_run_receipt_id: String,
    pub source_run_id: String,
    pub source_check_passed_receipt_id: String,
    pub draft_receipt_id: String,
    pub ratification_receipt_id: String,
    pub drafted_by: String,
    pub ratified_by: String,
}

impl<S: StorageProvider> MdxKernel<S> {
    /// Draft a distilled skill from a verified successful Forge run. The source
    /// run receipt must be a run_finished forge.run.event stamped
    /// RUN_FINISHED_DONE whose run also emitted at least one check_passed
    /// event; anything else refuses. The procedure is assembled deterministically
    /// from that run's own trajectory - the ledger evidence, never invented.
    pub fn draft_marketplace_skill(
        &mut self,
        request: MarketplaceSkillDraft<'_>,
    ) -> Result<MarketplaceSkillDraftReport, MarketplaceSkillError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.draft_marketplace_skill_with_applicability(
            request,
            LearningMemoryApplicability::default(),
            &identity,
        )
    }

    pub fn draft_marketplace_skill_with_applicability(
        &mut self,
        request: MarketplaceSkillDraft<'_>,
        applicability: LearningMemoryApplicability<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<MarketplaceSkillDraftReport, MarketplaceSkillError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("source_run_receipt_id", request.source_run_receipt_id),
            ("skill_id", request.skill_id),
            ("skill_name", request.skill_name),
            ("summary", request.summary),
            ("helps_with", request.helps_with),
        ] {
            if value.trim().is_empty() {
                return Err(MarketplaceSkillError::Missing(field));
            }
        }
        let procedure_source = match request.procedure_source.trim() {
            "" => "deterministic",
            other => other,
        };
        if !MARKETPLACE_SKILL_PROCEDURE_SOURCES.contains(&procedure_source) {
            return Err(MarketplaceSkillError::UnknownProcedureSource(
                procedure_source.to_string(),
            ));
        }
        let sensitivity = match request.sensitivity.trim() {
            "" => "normal",
            other => other,
        };
        if !MARKETPLACE_SKILL_SENSITIVITIES.contains(&sensitivity) {
            return Err(MarketplaceSkillError::UnknownSensitivity(
                sensitivity.to_string(),
            ));
        }
        for (field, value) in [
            ("skill_name", request.skill_name),
            ("summary", request.summary),
            ("helps_with", request.helps_with),
            ("applicability_work_tiers", applicability.work_tiers),
            ("applicability_language_packs", applicability.language_packs),
            ("applicability_notes", applicability.notes),
        ] {
            let len = value.chars().count();
            if len > MAX_TEXT_CHARS {
                return Err(MarketplaceSkillError::TooLong(field, len, MAX_TEXT_CHARS));
            }
        }

        // Verify the source run is a proven-green run_finished event.
        let source_run_receipt_id = request.source_run_receipt_id.trim();
        let source = match self.ledger().query().by_id(source_run_receipt_id) {
            None => {
                return Err(MarketplaceSkillError::UnknownSourceRun(
                    source_run_receipt_id.to_string(),
                ));
            }
            Some(receipt) => receipt,
        };
        let run_finished = source.kind == "forge.run.event"
            && source.payload.get("event").map(String::as_str) == Some("run_finished")
            && source
                .payload
                .get("detail")
                .map(|detail| detail.contains("status=RUN_FINISHED_DONE"))
                .unwrap_or(false);
        if !run_finished {
            return Err(MarketplaceSkillError::SourceRunNotGreen(
                source_run_receipt_id.to_string(),
            ));
        }
        let source_run_id = source.payload.get("run_id").cloned().unwrap_or_default();

        // The run must have proved a real check: at least one check_passed
        // trajectory event for the same run. This is the green in proven-green.
        let source_check_passed_receipt_id = self
            .ledger()
            .query()
            .by_kind("forge.run.event")
            .into_iter()
            .find(|receipt| {
                receipt.payload.get("run_id").map(String::as_str) == Some(source_run_id.as_str())
                    && receipt.payload.get("event").map(String::as_str) == Some("check_passed")
            })
            .map(|receipt| receipt.receipt_id.clone());
        let source_check_passed_receipt_id = match source_check_passed_receipt_id {
            Some(id) => id,
            None => {
                return Err(MarketplaceSkillError::SourceRunNoProof(source_run_id));
            }
        };

        // Deterministic procedure assembly from the run's own trajectory.
        let deterministic_steps = self.distill_run_trajectory_steps(&source_run_id);
        let procedure_step_count = deterministic_steps.len();
        let deterministic_procedure = number_steps(&deterministic_steps);

        // The stored procedure: the deterministic floor, unless a caller passes
        // a model-sharpened variant that clears the claim guard. The guard
        // never fires without a real deterministic floor, so a run with no
        // trajectory can never mint a model-authored skill.
        let (procedure, procedure_source) = if procedure_source == "model_distilled" {
            let candidate = request.model_procedure.trim();
            if let Err(reason) = model_procedure_claims_safe(candidate, procedure_step_count) {
                return Err(MarketplaceSkillError::ModelProcedureRejected(reason));
            }
            (candidate.to_string(), "model_distilled")
        } else {
            (deterministic_procedure.clone(), "deterministic")
        };
        if procedure.chars().count() > MAX_TEXT_CHARS {
            return Err(MarketplaceSkillError::TooLong(
                "procedure",
                procedure.chars().count(),
                MAX_TEXT_CHARS,
            ));
        }

        let skill_id = request.skill_id.trim();
        let boundary_locked = if sensitivity == "sensitive" {
            "true"
        } else {
            "false"
        };
        let evidence_refs = format!(
            "source_run_receipt={source_run_receipt_id}, check_passed_receipt={source_check_passed_receipt_id}"
        );
        let step_count_text = procedure_step_count.to_string();
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "marketplace_skill",
                action: ActionKind::DraftMarketplaceSkill,
                transition: "DRAFT_MARKETPLACE_SKILL",
                kind: MARKETPLACE_SKILL_DRAFTED_KIND,
            },
            payload(&[
                ("skill_id", skill_id),
                ("skill_name", request.skill_name.trim()),
                ("summary", request.summary.trim()),
                ("helps_with", request.helps_with.trim()),
                ("source_run_receipt_id", source_run_receipt_id),
                ("source_run_id", source_run_id.as_str()),
                (
                    "source_check_passed_receipt_id",
                    source_check_passed_receipt_id.as_str(),
                ),
                ("procedure", procedure.as_str()),
                ("procedure_deterministic", deterministic_procedure.as_str()),
                ("procedure_source", procedure_source),
                ("procedure_step_count", step_count_text.as_str()),
                ("sensitivity", sensitivity),
                ("boundary_locked", boundary_locked),
                ("applicability_work_tiers", applicability.work_tiers.trim()),
                (
                    "applicability_language_packs",
                    applicability.language_packs.trim(),
                ),
                ("applicability_notes", applicability.notes.trim()),
                ("evidence_refs", evidence_refs.as_str()),
                ("origin", "distilled_skill"),
                ("skill_state", "drafted"),
                ("identity_source", &identity.identity_source),
                ("capability_execution_allowed", "false"),
                ("secret_access_allowed", "false"),
                ("inherited_agent_permissions_allowed", "false"),
                ("adaptation_allowed", "false"),
                ("runtime_behavior_change_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(MarketplaceSkillDraftReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            skill_id: skill_id.to_string(),
            skill_state: "drafted".to_string(),
            source_run_id,
            procedure_source: procedure_source.to_string(),
            procedure_step_count,
        })
    }

    /// Promote a DRAFT into a ratified distilled capability. The ratifier must
    /// be a DISTINCT actor from the drafter and must cite the draft receipt.
    /// This is the Voyager skill-library-add, governed and receipted: the
    /// ratification receipt carries the full chain source_run -> draft ->
    /// ratification, and the capability registry folds it in as a
    /// distilled_skill origin at read time.
    pub fn ratify_marketplace_skill(
        &mut self,
        request: MarketplaceSkillRatification<'_>,
    ) -> Result<MarketplaceSkillRatificationReport, MarketplaceSkillError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.ratify_marketplace_skill_with_identity(request, &identity)
    }

    pub fn ratify_marketplace_skill_with_identity(
        &mut self,
        request: MarketplaceSkillRatification<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<MarketplaceSkillRatificationReport, MarketplaceSkillError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("skill_id", request.skill_id),
            ("draft_receipt_id", request.draft_receipt_id),
            ("decision", request.decision),
        ] {
            if value.trim().is_empty() {
                return Err(MarketplaceSkillError::Missing(field));
            }
        }
        if request.note.chars().count() > MAX_TEXT_CHARS {
            return Err(MarketplaceSkillError::TooLong(
                "note",
                request.note.chars().count(),
                MAX_TEXT_CHARS,
            ));
        }
        let decision = request.decision.trim();
        if decision != "ratify" && decision != "decline" {
            return Err(MarketplaceSkillError::UnknownDecision(decision.to_string()));
        }
        let draft_receipt_id = request.draft_receipt_id.trim();
        let draft = match self.ledger().query().by_id(draft_receipt_id) {
            None => {
                return Err(MarketplaceSkillError::UnknownDraft(
                    draft_receipt_id.to_string(),
                ));
            }
            Some(receipt) if receipt.kind != MARKETPLACE_SKILL_DRAFTED_KIND => {
                return Err(MarketplaceSkillError::WrongDraftKind(
                    draft_receipt_id.to_string(),
                    receipt.kind.clone(),
                ));
            }
            Some(receipt) => receipt,
        };
        let drafted_by = draft.actor_id.as_str().to_string();
        if drafted_by == request.actor_id.trim() {
            return Err(MarketplaceSkillError::SelfRatification(drafted_by));
        }
        // Carry the source-run chain forward from the draft onto the
        // ratification so a ratified skill never floats free of its evidence.
        let field = |key: &str| draft.payload.get(key).cloned().unwrap_or_default();
        let capability_state = if decision == "ratify" {
            "ratified"
        } else {
            "declined"
        };
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "marketplace_skill",
                action: ActionKind::RatifyMarketplaceSkill,
                transition: "RATIFY_MARKETPLACE_SKILL",
                kind: MARKETPLACE_SKILL_RATIFIED_KIND,
            },
            payload(&[
                ("skill_id", request.skill_id.trim()),
                ("skill_name", field("skill_name").as_str()),
                ("summary", field("summary").as_str()),
                ("helps_with", field("helps_with").as_str()),
                ("draft_receipt_id", draft_receipt_id),
                (
                    "source_run_receipt_id",
                    field("source_run_receipt_id").as_str(),
                ),
                ("source_run_id", field("source_run_id").as_str()),
                (
                    "source_check_passed_receipt_id",
                    field("source_check_passed_receipt_id").as_str(),
                ),
                ("procedure", field("procedure").as_str()),
                ("procedure_source", field("procedure_source").as_str()),
                (
                    "procedure_step_count",
                    field("procedure_step_count").as_str(),
                ),
                ("sensitivity", field("sensitivity").as_str()),
                ("boundary_locked", field("boundary_locked").as_str()),
                (
                    "applicability_work_tiers",
                    field("applicability_work_tiers").as_str(),
                ),
                (
                    "applicability_language_packs",
                    field("applicability_language_packs").as_str(),
                ),
                ("applicability_notes", field("applicability_notes").as_str()),
                ("evidence_refs", field("evidence_refs").as_str()),
                ("origin", "distilled_skill"),
                ("ratification_decision", decision),
                ("capability_state", capability_state),
                ("drafted_by", drafted_by.as_str()),
                ("ratified_by", request.actor_id.trim()),
                ("note", request.note.trim()),
                ("distinct_actor_required", "true"),
                ("identity_source", &identity.identity_source),
                ("capability_execution_allowed", "false"),
                ("secret_access_allowed", "false"),
                ("inherited_agent_permissions_allowed", "false"),
                ("adaptation_allowed", "false"),
                ("runtime_behavior_change_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(MarketplaceSkillRatificationReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            skill_id: request.skill_id.trim().to_string(),
            capability_state: capability_state.to_string(),
            drafted_by,
            ratified_by: request.actor_id.trim().to_string(),
        })
    }

    /// Ordered procedure steps derived from one run's own trajectory events:
    /// the tool executions and passed checks in the order the ledger recorded
    /// them, deduplicated where a step repeats back to back and capped so a
    /// long run yields a bounded procedure. Deterministic - the same run
    /// always distills to the same steps.
    pub fn distill_run_trajectory_steps(&self, run_id: &str) -> Vec<String> {
        let mut steps: Vec<String> = Vec::new();
        for receipt in self.ledger().query().by_kind("forge.run.event") {
            if receipt.payload.get("run_id").map(String::as_str) != Some(run_id) {
                continue;
            }
            let event = receipt
                .payload
                .get("event")
                .map(String::as_str)
                .unwrap_or("");
            let detail = receipt
                .payload
                .get("detail")
                .map(String::as_str)
                .unwrap_or("")
                .trim();
            let step = match event {
                "tool_executed" => {
                    let compact = compact_step_detail(detail);
                    if compact.is_empty() {
                        continue;
                    }
                    compact
                }
                "check_passed" => {
                    let compact = compact_step_detail(detail);
                    if compact.is_empty() {
                        "run the selected proof to green".to_string()
                    } else {
                        format!("prove it green: {compact}")
                    }
                }
                _ => continue,
            };
            if steps.last().map(String::as_str) == Some(step.as_str()) {
                continue;
            }
            steps.push(step);
            if steps.len() >= MAX_PROCEDURE_STEPS {
                break;
            }
        }
        steps
    }

    /// The distilled skills folded from the ledger, drafts first then their
    /// ratifications, newest ratification winning the state for a skill id.
    /// The surface and the Forge reuse path both read this fold.
    pub fn distilled_skill_views(&self) -> Vec<DistilledSkillView> {
        use std::collections::BTreeMap;
        let mut views: BTreeMap<String, DistilledSkillView> = BTreeMap::new();
        for receipt in self
            .ledger()
            .query()
            .by_kind(MARKETPLACE_SKILL_DRAFTED_KIND)
        {
            let field = |key: &str| receipt.payload.get(key).cloned().unwrap_or_default();
            let skill_id = field("skill_id");
            if skill_id.is_empty() {
                continue;
            }
            views.insert(
                skill_id.clone(),
                DistilledSkillView {
                    skill_id,
                    skill_name: field("skill_name"),
                    summary: field("summary"),
                    helps_with: field("helps_with"),
                    procedure: field("procedure"),
                    procedure_source: field("procedure_source"),
                    procedure_step_count: field("procedure_step_count").parse().unwrap_or(0),
                    sensitivity: field("sensitivity"),
                    boundary_locked: field("boundary_locked") == "true",
                    applicability_work_tiers: field("applicability_work_tiers"),
                    applicability_language_packs: field("applicability_language_packs"),
                    state: "drafted".to_string(),
                    source_run_receipt_id: field("source_run_receipt_id"),
                    source_run_id: field("source_run_id"),
                    source_check_passed_receipt_id: field("source_check_passed_receipt_id"),
                    draft_receipt_id: receipt.receipt_id.clone(),
                    ratification_receipt_id: String::new(),
                    drafted_by: receipt.actor_id.as_str().to_string(),
                    ratified_by: String::new(),
                },
            );
        }
        for receipt in self
            .ledger()
            .query()
            .by_kind(MARKETPLACE_SKILL_RATIFIED_KIND)
        {
            let field = |key: &str| receipt.payload.get(key).cloned().unwrap_or_default();
            let skill_id = field("skill_id");
            if skill_id.is_empty() {
                continue;
            }
            let entry = views
                .entry(skill_id.clone())
                .or_insert_with(|| DistilledSkillView {
                    skill_id: skill_id.clone(),
                    skill_name: field("skill_name"),
                    summary: field("summary"),
                    helps_with: field("helps_with"),
                    procedure: field("procedure"),
                    procedure_source: field("procedure_source"),
                    procedure_step_count: field("procedure_step_count").parse().unwrap_or(0),
                    sensitivity: field("sensitivity"),
                    boundary_locked: field("boundary_locked") == "true",
                    applicability_work_tiers: field("applicability_work_tiers"),
                    applicability_language_packs: field("applicability_language_packs"),
                    state: "drafted".to_string(),
                    source_run_receipt_id: field("source_run_receipt_id"),
                    source_run_id: field("source_run_id"),
                    source_check_passed_receipt_id: field("source_check_passed_receipt_id"),
                    draft_receipt_id: field("draft_receipt_id"),
                    ratification_receipt_id: String::new(),
                    drafted_by: field("drafted_by"),
                    ratified_by: String::new(),
                });
            entry.state = if field("capability_state") == "ratified" {
                "ratified".to_string()
            } else {
                "declined".to_string()
            };
            entry.ratification_receipt_id = receipt.receipt_id.clone();
            entry.ratified_by = field("ratified_by");
            if entry.draft_receipt_id.is_empty() {
                entry.draft_receipt_id = field("draft_receipt_id");
            }
        }
        views.into_values().collect()
    }
}

/// Number a step list into a compact procedure block.
fn number_steps(steps: &[String]) -> String {
    if steps.is_empty() {
        return "No trajectory steps were recorded for this run.".to_string();
    }
    steps
        .iter()
        .enumerate()
        .map(|(index, step)| format!("{}. {step}", index + 1))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Trim one run-event detail into a short procedure step. Keeps the leading
/// clause before any exit-code or tail noise so "run_command make test exit=0
/// tail=..." becomes "run_command make test".
fn compact_step_detail(detail: &str) -> String {
    let head = detail
        .split(" exit=")
        .next()
        .unwrap_or(detail)
        .split(" tail=")
        .next()
        .unwrap_or(detail)
        .trim();
    let capped: String = head.chars().take(120).collect();
    capped.trim().to_string()
}

/// The claim guard on a model-sharpened procedure: it must be non-empty and
/// bounded, must not carry em or en dashes, and only rides when the run
/// produced a real deterministic floor, so a model can never invent a
/// procedure for a run that has no trajectory.
fn model_procedure_claims_safe(
    candidate: &str,
    deterministic_step_count: usize,
) -> Result<(), String> {
    if candidate.is_empty() {
        return Err("the model procedure is empty".to_string());
    }
    if deterministic_step_count == 0 {
        return Err("the source run has no trajectory to sharpen".to_string());
    }
    if candidate.chars().count() > MAX_TEXT_CHARS {
        return Err(format!(
            "the model procedure is {} characters; the limit is {MAX_TEXT_CHARS}",
            candidate.chars().count()
        ));
    }
    if candidate.contains('\u{2014}') || candidate.contains('\u{2013}') {
        return Err("the model procedure uses an em or en dash".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Build a proven-green run in the ledger and return the run_finished
    // receipt id. The run records a tool execution and a passed check so the
    // trajectory distills to real steps.
    fn green_run(kernel: &mut MdxKernel, run_id: &str) -> String {
        use crate::ForgeRunEvent;
        let events = [
            ("run_started", "run started"),
            ("tool_executed", "edit_file src/lib.rs"),
            ("check_passed", "run_command cargo test exit=0 tail=ok"),
        ];
        for (event, detail) in events {
            kernel
                .record_forge_run_event(ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:builder",
                    run_id,
                    event,
                    work_item_id: "",
                    detail,
                    turn: 1,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .expect("run event");
        }
        kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:builder",
                run_id,
                event: "run_finished",
                work_item_id: "",
                detail: "status=RUN_FINISHED_DONE turns=1 files_changed=1 check_runs=1",
                turn: 1,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("run finished")
            .receipt_id
    }

    fn draft(kernel: &mut MdxKernel, run_receipt_id: &str) -> MarketplaceSkillDraftReport {
        kernel
            .draft_marketplace_skill(MarketplaceSkillDraft {
                tenant_id: "local_tenant",
                actor_id: "human:builder",
                source_run_receipt_id: run_receipt_id,
                skill_id: "rust_test_repair_v1",
                skill_name: "Rust test repair",
                summary: "Repair a failing Rust test and prove it green.",
                helps_with: "backend test repair runs",
                model_procedure: "",
                procedure_source: "",
                sensitivity: "",
            })
            .expect("draft")
    }

    #[test]
    fn drafts_a_skill_from_a_verified_green_run_with_a_real_procedure() {
        let mut kernel = MdxKernel::boot_local();
        let run_receipt = green_run(&mut kernel, "run_alpha");
        let report = draft(&mut kernel, &run_receipt);
        assert_eq!(report.skill_state, "drafted");
        assert_eq!(report.source_run_id, "run_alpha");
        assert!(report.procedure_step_count >= 2);
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .expect("receipt");
        assert_eq!(receipt.kind, MARKETPLACE_SKILL_DRAFTED_KIND);
        assert_eq!(
            receipt.payload.get("origin").map(String::as_str),
            Some("distilled_skill")
        );
        assert_eq!(
            receipt
                .payload
                .get("capability_execution_allowed")
                .map(String::as_str),
            Some("false")
        );
        let procedure = receipt
            .payload
            .get("procedure")
            .cloned()
            .unwrap_or_default();
        assert!(procedure.contains("edit_file src/lib.rs"));
        assert!(procedure.contains("prove it green"));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn draft_refuses_a_run_that_is_not_proven_green() {
        let mut kernel = MdxKernel::boot_local();
        use crate::ForgeRunEvent;
        // A run that finished but did NOT reach RUN_FINISHED_DONE.
        let not_green = kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:builder",
                run_id: "run_red",
                event: "run_finished",
                work_item_id: "",
                detail: "status=RUN_FINISHED_CANNOT_PROCEED turns=3 files_changed=0",
                turn: 3,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("run finished")
            .receipt_id;
        let bad = kernel.draft_marketplace_skill(MarketplaceSkillDraft {
            tenant_id: "local_tenant",
            actor_id: "human:builder",
            source_run_receipt_id: &not_green,
            skill_id: "bad_skill",
            skill_name: "Bad",
            summary: "Should not draft.",
            helps_with: "nothing",
            model_procedure: "",
            procedure_source: "",
            sensitivity: "",
        });
        assert!(matches!(
            bad,
            Err(MarketplaceSkillError::SourceRunNotGreen(_))
        ));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn draft_refuses_an_unknown_source_run_receipt() {
        let mut kernel = MdxKernel::boot_local();
        let bad = kernel.draft_marketplace_skill(MarketplaceSkillDraft {
            tenant_id: "local_tenant",
            actor_id: "human:builder",
            source_run_receipt_id: "receipt_missing",
            skill_id: "bad_skill",
            skill_name: "Bad",
            summary: "Should not draft.",
            helps_with: "nothing",
            model_procedure: "",
            procedure_source: "",
            sensitivity: "",
        });
        assert!(matches!(
            bad,
            Err(MarketplaceSkillError::UnknownSourceRun(_))
        ));
    }

    #[test]
    fn draft_refuses_a_green_finish_with_no_check_passed_event() {
        let mut kernel = MdxKernel::boot_local();
        use crate::ForgeRunEvent;
        let finished = kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:builder",
                run_id: "run_noproof",
                event: "run_finished",
                work_item_id: "",
                detail: "status=RUN_FINISHED_DONE turns=1 files_changed=1",
                turn: 1,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("run finished")
            .receipt_id;
        let bad = kernel.draft_marketplace_skill(MarketplaceSkillDraft {
            tenant_id: "local_tenant",
            actor_id: "human:builder",
            source_run_receipt_id: &finished,
            skill_id: "noproof_skill",
            skill_name: "No proof",
            summary: "No check_passed event.",
            helps_with: "nothing",
            model_procedure: "",
            procedure_source: "",
            sensitivity: "",
        });
        assert!(matches!(
            bad,
            Err(MarketplaceSkillError::SourceRunNoProof(_))
        ));
    }

    #[test]
    fn ratifies_a_draft_with_a_distinct_actor_and_carries_the_chain() {
        let mut kernel = MdxKernel::boot_local();
        let run_receipt = green_run(&mut kernel, "run_beta");
        let drafted = draft(&mut kernel, &run_receipt);
        let report = kernel
            .ratify_marketplace_skill(MarketplaceSkillRatification {
                tenant_id: "local_tenant",
                actor_id: "human:reviewer",
                skill_id: "rust_test_repair_v1",
                draft_receipt_id: &drafted.receipt_id,
                decision: "ratify",
                note: "the trajectory matches a real repair",
            })
            .expect("ratify");
        assert_eq!(report.capability_state, "ratified");
        assert_eq!(report.drafted_by, "human:builder");
        assert_eq!(report.ratified_by, "human:reviewer");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .expect("receipt");
        assert_eq!(receipt.kind, MARKETPLACE_SKILL_RATIFIED_KIND);
        assert_eq!(
            receipt.payload.get("draft_receipt_id").map(String::as_str),
            Some(drafted.receipt_id.as_str())
        );
        assert_eq!(
            receipt.payload.get("source_run_id").map(String::as_str),
            Some("run_beta")
        );
        assert_eq!(
            receipt.payload.get("origin").map(String::as_str),
            Some("distilled_skill")
        );
        let views = kernel.distilled_skill_views();
        let view = views
            .iter()
            .find(|view| view.skill_id == "rust_test_repair_v1")
            .expect("view");
        assert_eq!(view.state, "ratified");
        assert_eq!(view.source_run_id, "run_beta");
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn ratify_refuses_self_ratification() {
        let mut kernel = MdxKernel::boot_local();
        let run_receipt = green_run(&mut kernel, "run_gamma");
        let drafted = draft(&mut kernel, &run_receipt);
        let bad = kernel.ratify_marketplace_skill(MarketplaceSkillRatification {
            tenant_id: "local_tenant",
            actor_id: "human:builder",
            skill_id: "rust_test_repair_v1",
            draft_receipt_id: &drafted.receipt_id,
            decision: "ratify",
            note: "approving my own draft",
        });
        assert!(matches!(
            bad,
            Err(MarketplaceSkillError::SelfRatification(_))
        ));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn ratify_refuses_a_missing_draft() {
        let mut kernel = MdxKernel::boot_local();
        let bad = kernel.ratify_marketplace_skill(MarketplaceSkillRatification {
            tenant_id: "local_tenant",
            actor_id: "human:reviewer",
            skill_id: "rust_test_repair_v1",
            draft_receipt_id: "receipt_missing",
            decision: "ratify",
            note: "",
        });
        assert!(matches!(bad, Err(MarketplaceSkillError::UnknownDraft(_))));
    }

    #[test]
    fn ratify_refuses_a_receipt_that_is_not_a_draft() {
        let mut kernel = MdxKernel::boot_local();
        let run_receipt = green_run(&mut kernel, "run_delta");
        // The run_finished receipt is not a draft.
        let bad = kernel.ratify_marketplace_skill(MarketplaceSkillRatification {
            tenant_id: "local_tenant",
            actor_id: "human:reviewer",
            skill_id: "rust_test_repair_v1",
            draft_receipt_id: &run_receipt,
            decision: "ratify",
            note: "",
        });
        assert!(matches!(
            bad,
            Err(MarketplaceSkillError::WrongDraftKind(_, _))
        ));
    }

    #[test]
    fn model_distilled_procedure_rides_only_behind_the_claim_guard() {
        let mut kernel = MdxKernel::boot_local();
        let run_receipt = green_run(&mut kernel, "run_epsilon");
        let good = kernel
            .draft_marketplace_skill(MarketplaceSkillDraft {
                tenant_id: "local_tenant",
                actor_id: "human:builder",
                source_run_receipt_id: &run_receipt,
                skill_id: "sharpened_v1",
                skill_name: "Sharpened",
                summary: "A sharpened procedure.",
                helps_with: "backend runs",
                model_procedure: "First edit the target file, then run the proof until it is green.",
                procedure_source: "model_distilled",
                sensitivity: "",
            })
            .expect("model draft");
        assert_eq!(good.procedure_source, "model_distilled");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&good.receipt_id)
            .expect("receipt");
        // The deterministic floor is always stored beside the sharpened prose.
        assert!(
            receipt
                .payload
                .get("procedure_deterministic")
                .map(|value| value.contains("edit_file"))
                .unwrap_or(false)
        );
        // An em dash is refused.
        let bad = kernel.draft_marketplace_skill(MarketplaceSkillDraft {
            tenant_id: "local_tenant",
            actor_id: "human:builder",
            source_run_receipt_id: &run_receipt,
            skill_id: "sharpened_v2",
            skill_name: "Sharpened",
            summary: "A sharpened procedure.",
            helps_with: "backend runs",
            model_procedure: "Edit the file \u{2014} then prove it green.",
            procedure_source: "model_distilled",
            sensitivity: "",
        });
        assert!(matches!(
            bad,
            Err(MarketplaceSkillError::ModelProcedureRejected(_))
        ));
    }
}
