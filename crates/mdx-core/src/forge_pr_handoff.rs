//! Forge PR handoff records the workflow-native bridge from a finished run to
//! a pull request description. It is deliberately a handoff, not an
//! integration action: no remote push, PR creation, approval, deployment, or
//! production write opens here.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

const MAX_FIELD_CHARS: usize = 2000;

#[derive(Clone, Debug)]
pub struct ForgePrHandoff<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub run_id: &'a str,
    pub branch: &'a str,
    pub review_status: &'a str,
    pub title: &'a str,
    pub body_sha256: &'a str,
    pub body_char_count: usize,
    pub summary_line_count: usize,
    pub review_checklist_count: usize,
    pub review_packet_route: &'a str,
    pub target_host: &'a str,
    pub delivery_guardrail_status: &'a str,
    pub delivery_blocked_reasons: &'a str,
    pub repo_intake_present: bool,
    pub repo_intake_generated_from: &'a str,
    pub repo_intake_readiness_status: &'a str,
    pub repo_intake_medium_high_work_ready: bool,
    pub repo_intake_source_host: &'a str,
    pub repo_intake_authority_boundary_clear: bool,
    pub proof_coverage_status: &'a str,
    pub principal_orientation_gate_required: bool,
    pub successful_semantic_orientation_observed: bool,
    pub related_tests_required_for_delivery: bool,
    pub successful_related_tests_observed: bool,
    pub candidate_comparison_present: bool,
    pub candidate_count: u32,
    pub candidate_recommended_run_id: &'a str,
    pub candidate_current_rank: u32,
    pub candidate_current_run_is_recommended: bool,
    pub candidate_comparison_grants_authority: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgePrHandoffReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub pr_handoff_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForgePrHandoffError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    BadSha256(String),
}

impl ForgePrHandoffError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("forge PR handoff missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("forge PR handoff {field} is {len} characters; the limit is {max}")
            }
            Self::BadSha256(value) => {
                format!("forge PR handoff body_sha256 must be sha256:<64 hex>; got {value}")
            }
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn record_forge_pr_handoff(
        &mut self,
        request: ForgePrHandoff<'_>,
    ) -> Result<ForgePrHandoffReport, ForgePrHandoffError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.record_forge_pr_handoff_with_identity(request, &identity)
    }

    pub fn record_forge_pr_handoff_with_identity(
        &mut self,
        request: ForgePrHandoff<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ForgePrHandoffReport, ForgePrHandoffError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("run_id", request.run_id),
            ("title", request.title),
            ("body_sha256", request.body_sha256),
            ("review_packet_route", request.review_packet_route),
        ] {
            if value.trim().is_empty() {
                return Err(ForgePrHandoffError::Missing(field));
            }
        }
        for (field, value) in [
            ("run_id", request.run_id),
            ("branch", request.branch),
            ("review_status", request.review_status),
            ("title", request.title),
            ("review_packet_route", request.review_packet_route),
            ("target_host", request.target_host),
            (
                "delivery_guardrail_status",
                request.delivery_guardrail_status,
            ),
            (
                "repo_intake_generated_from",
                request.repo_intake_generated_from,
            ),
            (
                "repo_intake_readiness_status",
                request.repo_intake_readiness_status,
            ),
            ("repo_intake_source_host", request.repo_intake_source_host),
            ("proof_coverage_status", request.proof_coverage_status),
            (
                "candidate_recommended_run_id",
                request.candidate_recommended_run_id,
            ),
        ] {
            let len = value.chars().count();
            if len > MAX_FIELD_CHARS {
                return Err(ForgePrHandoffError::TooLong(field, len, MAX_FIELD_CHARS));
            }
        }
        if !is_sha256(request.body_sha256) {
            return Err(ForgePrHandoffError::BadSha256(
                request.body_sha256.to_string(),
            ));
        }
        let pr_handoff_id = format!(
            "forge_pr_handoff_{}",
            self.ledger()
                .query()
                .by_kind("forge.pr.handoff.recorded")
                .len()
                + 1
        );
        let body_char_count = request.body_char_count.to_string();
        let summary_line_count = request.summary_line_count.to_string();
        let review_checklist_count = request.review_checklist_count.to_string();
        let repo_intake_present = bool_payload(request.repo_intake_present);
        let repo_intake_medium_high_work_ready =
            bool_payload(request.repo_intake_medium_high_work_ready);
        let repo_intake_authority_boundary_clear =
            bool_payload(request.repo_intake_authority_boundary_clear);
        let principal_orientation_gate_required =
            bool_payload(request.principal_orientation_gate_required);
        let successful_semantic_orientation_observed =
            bool_payload(request.successful_semantic_orientation_observed);
        let related_tests_required_for_delivery =
            bool_payload(request.related_tests_required_for_delivery);
        let successful_related_tests_observed =
            bool_payload(request.successful_related_tests_observed);
        let candidate_comparison_present = bool_payload(request.candidate_comparison_present);
        let candidate_count = request.candidate_count.to_string();
        let candidate_current_rank = request.candidate_current_rank.to_string();
        let candidate_current_run_is_recommended =
            bool_payload(request.candidate_current_run_is_recommended);
        let candidate_comparison_grants_authority =
            bool_payload(request.candidate_comparison_grants_authority);
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "forge_pr_handoff",
                action: ActionKind::RecordForgePrHandoff,
                transition: "RECORD_FORGE_PR_HANDOFF",
                kind: "forge.pr.handoff.recorded",
            },
            payload(&[
                ("pr_handoff_id", &pr_handoff_id),
                ("run_id", request.run_id.trim()),
                ("branch", request.branch.trim()),
                ("review_status", request.review_status.trim()),
                ("title", request.title.trim()),
                ("body_sha256", request.body_sha256.trim()),
                ("body_char_count", &body_char_count),
                ("summary_line_count", &summary_line_count),
                ("review_checklist_count", &review_checklist_count),
                ("review_packet_route", request.review_packet_route.trim()),
                ("target_host", target_host(request.target_host)),
                (
                    "delivery_guardrail_status",
                    request.delivery_guardrail_status.trim(),
                ),
                (
                    "delivery_blocked_reasons",
                    request.delivery_blocked_reasons.trim(),
                ),
                ("repo_intake_present", repo_intake_present),
                (
                    "repo_intake_generated_from",
                    request.repo_intake_generated_from.trim(),
                ),
                (
                    "repo_intake_readiness_status",
                    request.repo_intake_readiness_status.trim(),
                ),
                (
                    "repo_intake_medium_high_work_ready",
                    repo_intake_medium_high_work_ready,
                ),
                (
                    "repo_intake_source_host",
                    target_host(request.repo_intake_source_host),
                ),
                (
                    "repo_intake_authority_boundary_clear",
                    repo_intake_authority_boundary_clear,
                ),
                (
                    "proof_coverage_status",
                    request.proof_coverage_status.trim(),
                ),
                (
                    "principal_orientation_gate_required",
                    principal_orientation_gate_required,
                ),
                (
                    "successful_semantic_orientation_observed",
                    successful_semantic_orientation_observed,
                ),
                (
                    "related_tests_required_for_delivery",
                    related_tests_required_for_delivery,
                ),
                (
                    "successful_related_tests_observed",
                    successful_related_tests_observed,
                ),
                ("candidate_comparison_present", candidate_comparison_present),
                ("candidate_count", &candidate_count),
                (
                    "candidate_recommended_run_id",
                    request.candidate_recommended_run_id.trim(),
                ),
                ("candidate_current_rank", &candidate_current_rank),
                (
                    "candidate_current_run_is_recommended",
                    candidate_current_run_is_recommended,
                ),
                (
                    "candidate_comparison_grants_authority",
                    candidate_comparison_grants_authority,
                ),
                ("identity_source", &identity.identity_source),
                ("generated_from", "forge_review_packet.pr_handoff"),
                ("remote_push_allowed", "false"),
                ("pull_request_open_allowed", "false"),
                ("approval_allowed", "false"),
                ("deployment_authority_granted", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(ForgePrHandoffReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            pr_handoff_id,
        })
    }
}

fn target_host(value: &str) -> &str {
    let value = value.trim();
    if value.is_empty() { "generic" } else { value }
}

fn bool_payload(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn is_sha256(value: &str) -> bool {
    let digest = value.strip_prefix("sha256:").unwrap_or_default();
    digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY_SHA256: &str =
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[test]
    fn records_pr_handoff_without_opening_remote_or_ship_authority() {
        let mut kernel = MdxKernel::boot_local();
        let report = kernel
            .record_forge_pr_handoff(ForgePrHandoff {
                tenant_id: "t",
                actor_id: "human:dev",
                run_id: "forge_run_1",
                branch: "forge/run_1",
                review_status: "ready_for_review",
                title: "Forge run forge_run_1: ready_for_review",
                body_sha256: BODY_SHA256,
                body_char_count: 1200,
                summary_line_count: 12,
                review_checklist_count: 7,
                review_packet_route: "/forge/review-packet.json",
                target_host: "github",
                delivery_guardrail_status: "SATISFIED",
                delivery_blocked_reasons: "",
                repo_intake_present: true,
                repo_intake_generated_from: "repo_readiness+repo_task_scout+language_task_alignment",
                repo_intake_readiness_status: "READY_FOR_MEDIUM_HIGH_WORK",
                repo_intake_medium_high_work_ready: true,
                repo_intake_source_host: "github",
                repo_intake_authority_boundary_clear: true,
                proof_coverage_status: "satisfied",
                principal_orientation_gate_required: true,
                successful_semantic_orientation_observed: true,
                related_tests_required_for_delivery: true,
                successful_related_tests_observed: true,
                candidate_comparison_present: true,
                candidate_count: 1,
                candidate_recommended_run_id: "forge_run_1",
                candidate_current_rank: 1,
                candidate_current_run_is_recommended: true,
                candidate_comparison_grants_authority: false,
            })
            .expect("record handoff");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .expect("handoff receipt");
        assert_eq!(receipt.kind, "forge.pr.handoff.recorded");
        assert_eq!(receipt.payload["pr_handoff_id"], report.pr_handoff_id);
        assert_eq!(receipt.payload["target_host"], "github");
        assert_eq!(receipt.payload["body_sha256"], BODY_SHA256);
        assert_eq!(receipt.payload["delivery_guardrail_status"], "SATISFIED");
        assert_eq!(receipt.payload["delivery_blocked_reasons"], "");
        assert_eq!(receipt.payload["repo_intake_present"], "true");
        assert_eq!(
            receipt.payload["repo_intake_generated_from"],
            "repo_readiness+repo_task_scout+language_task_alignment"
        );
        assert_eq!(
            receipt.payload["repo_intake_readiness_status"],
            "READY_FOR_MEDIUM_HIGH_WORK"
        );
        assert_eq!(
            receipt.payload["repo_intake_medium_high_work_ready"],
            "true"
        );
        assert_eq!(receipt.payload["repo_intake_source_host"], "github");
        assert_eq!(
            receipt.payload["repo_intake_authority_boundary_clear"],
            "true"
        );
        assert_eq!(receipt.payload["proof_coverage_status"], "satisfied");
        assert_eq!(
            receipt.payload["principal_orientation_gate_required"],
            "true"
        );
        assert_eq!(
            receipt.payload["successful_semantic_orientation_observed"],
            "true"
        );
        assert_eq!(
            receipt.payload["related_tests_required_for_delivery"],
            "true"
        );
        assert_eq!(receipt.payload["successful_related_tests_observed"], "true");
        assert_eq!(receipt.payload["candidate_comparison_present"], "true");
        assert_eq!(receipt.payload["candidate_count"], "1");
        assert_eq!(
            receipt.payload["candidate_recommended_run_id"],
            "forge_run_1"
        );
        assert_eq!(receipt.payload["candidate_current_rank"], "1");
        assert_eq!(
            receipt.payload["candidate_current_run_is_recommended"],
            "true"
        );
        assert_eq!(
            receipt.payload["candidate_comparison_grants_authority"],
            "false"
        );
        assert_eq!(receipt.payload["remote_push_allowed"], "false");
        assert_eq!(receipt.payload["pull_request_open_allowed"], "false");
        assert_eq!(receipt.payload["approval_allowed"], "false");
        assert_eq!(receipt.payload["production_write_allowed"], "false");
    }

    #[test]
    fn refuses_malformed_body_digest() {
        let mut kernel = MdxKernel::boot_local();
        let err = kernel
            .record_forge_pr_handoff(ForgePrHandoff {
                tenant_id: "t",
                actor_id: "human:dev",
                run_id: "forge_run_1",
                branch: "forge/run_1",
                review_status: "ready_for_review",
                title: "Forge run forge_run_1: ready_for_review",
                body_sha256: "sha256:not-a-real-digest",
                body_char_count: 1200,
                summary_line_count: 12,
                review_checklist_count: 7,
                review_packet_route: "/forge/review-packet.json",
                target_host: "github",
                delivery_guardrail_status: "BLOCKED",
                delivery_blocked_reasons: "proof_coverage_not_satisfied",
                repo_intake_present: true,
                repo_intake_generated_from: "repo_readiness+repo_task_scout+language_task_alignment",
                repo_intake_readiness_status: "SETUP_REQUIRED",
                repo_intake_medium_high_work_ready: false,
                repo_intake_source_host: "github",
                repo_intake_authority_boundary_clear: true,
                proof_coverage_status: "missing",
                principal_orientation_gate_required: true,
                successful_semantic_orientation_observed: false,
                related_tests_required_for_delivery: true,
                successful_related_tests_observed: false,
                candidate_comparison_present: true,
                candidate_count: 2,
                candidate_recommended_run_id: "forge_run_2",
                candidate_current_rank: 2,
                candidate_current_run_is_recommended: false,
                candidate_comparison_grants_authority: false,
            })
            .expect_err("bad digest refused");
        assert!(matches!(err, ForgePrHandoffError::BadSha256(_)));
    }
}
