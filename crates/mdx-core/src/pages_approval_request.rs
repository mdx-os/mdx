use crate::*;

pub const PAGES_APPROVAL_DECISION_APPROVE_ROUTE: &str = "/pages/approval-decisions/approve.json";
pub const PAGES_APPROVAL_DECISION_REJECT_ROUTE: &str = "/pages/approval-decisions/reject.json";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagesApprovalRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub approval_request_id: &'a str,
    pub source_edit_draft_receipt_id: &'a str,
    pub document_id: &'a str,
    pub draft_id: &'a str,
    pub requested_visibility: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagesApprovalRequestReport {
    pub status: &'static str,
    pub approval_request_id: String,
    pub approval_request_receipt_id: String,
    pub policy_decision_id: String,
    pub source_edit_draft_receipt_id: String,
    pub document_id: String,
    pub draft_id: String,
    pub public_visibility_allowed: bool,
    pub search_indexing_allowed: bool,
    pub embedding_provider_allowed: bool,
    pub rich_editor_allowed: bool,
    pub standalone_store_allowed: bool,
    pub production_publish_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagesApprovalDecision<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub approval_decision_id: &'a str,
    pub approval_request_receipt_id: &'a str,
    pub decision_outcome: &'a str,
    pub decision_note: &'a str,
    pub source_route: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagesApprovalDecisionReport {
    pub status: &'static str,
    pub approval_decision_id: String,
    pub approval_decision_receipt_id: String,
    pub policy_decision_id: String,
    pub approval_request_receipt_id: String,
    pub approval_request_id: String,
    pub document_id: String,
    pub draft_id: String,
    pub decision_outcome: String,
    pub decision_note: String,
    pub created_at: String,
    pub human_decision_recorded: bool,
    pub human_approval_granted: bool,
    pub reviewer_independence: &'static str,
    pub self_review_permitted: bool,
    pub public_visibility_allowed: bool,
    pub search_indexing_allowed: bool,
    pub embedding_provider_allowed: bool,
    pub rich_editor_allowed: bool,
    pub standalone_store_allowed: bool,
    pub production_publish_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PagesApprovalRequestError {
    Missing(&'static str),
    UnknownEditDraftReceipt(String),
    InvalidEditDraftReceipt(String),
    UnknownApprovalRequestReceipt(String),
    ApprovalRequestMismatch(String),
    ApprovalRequestSuperseded(String),
    ApprovalRequestAlreadyDecided(String),
    InvalidDecisionOutcome(String),
    ActorAdmission(String),
}

impl PagesApprovalRequestError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("pages approval request missing {field}"),
            Self::UnknownEditDraftReceipt(id) => {
                format!("pages approval request source receipt {id} is unknown")
            }
            Self::InvalidEditDraftReceipt(id) => format!(
                "pages approval request source receipt {id} is not the named tenant, document, and draft"
            ),
            Self::UnknownApprovalRequestReceipt(id) => {
                format!("pages approval decision source request receipt {id} is unknown")
            }
            Self::ApprovalRequestMismatch(detail) => {
                format!("pages approval decision request mismatch: {detail}")
            }
            Self::ApprovalRequestSuperseded(id) => format!(
                "pages approval request receipt {id} no longer covers the latest draft and request"
            ),
            Self::ApprovalRequestAlreadyDecided(id) => {
                format!("pages approval request receipt {id} already has a decision")
            }
            Self::InvalidDecisionOutcome(outcome) => {
                format!("pages approval decision outcome {outcome} is invalid")
            }
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_pages_approval_request_local(
        &mut self,
        request: PagesApprovalRequest<'_>,
    ) -> Result<PagesApprovalRequestReport, PagesApprovalRequestError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_pages_approval_request_local_with_identity(request, &identity)
    }

    pub fn save_pages_approval_request_local_with_identity(
        &mut self,
        request: PagesApprovalRequest<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<PagesApprovalRequestReport, PagesApprovalRequestError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("approval_request_id", request.approval_request_id),
            ("document_id", request.document_id),
            ("draft_id", request.draft_id),
            ("requested_visibility", request.requested_visibility),
        ] {
            if value.trim().is_empty() {
                return Err(PagesApprovalRequestError::Missing(field));
            }
        }
        let source_edit_draft_receipt_id = self.ensure_pages_approval_source(
            request.source_edit_draft_receipt_id,
            request.tenant_id,
            request.document_id,
            request.draft_id,
        )?;
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "operator",
            "/pages/approval-requests.json",
            "pages.approval.requested",
            request.approval_request_id,
        )
        .map_err(|error| PagesApprovalRequestError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("pages_approval_request"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision = self.decide_with_receipt(&correlation, ActionKind::RequestPagesApproval);
        let created_at = local_created_at(
            self.storage
                .ledger()
                .query()
                .by_kind("pages.approval.requested")
                .len()
                + 1,
        );
        let approval_receipt = self.transition_receipt(
            &run_id,
            "REQUEST_PAGES_APPROVAL",
            &correlation,
            &decision,
            "pages.approval.requested",
            payload(&[
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("approval_request_id", request.approval_request_id),
                ("source_route", "/pages/approval-requests.json"),
                (
                    "projection_route",
                    "/pages/approval-requests/projection.json",
                ),
                (
                    "source_edit_draft_receipt_id",
                    &source_edit_draft_receipt_id,
                ),
                ("document_id", request.document_id),
                ("draft_id", request.draft_id),
                ("requested_visibility", request.requested_visibility),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                (
                    "terminal_state",
                    "PAGE_APPROVAL_REQUEST_RECORDED_PUBLICATION_BLOCKED",
                ),
                ("created_at", &created_at),
                ("public_visibility_allowed", "false"),
                ("search_indexing_allowed", "false"),
                ("embedding_provider_allowed", "false"),
                ("rich_editor_allowed", "false"),
                ("standalone_store_allowed", "false"),
                ("production_publish_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_pages_approval_request_run(&run_id);
        Ok(PagesApprovalRequestReport {
            status: "PAGE_APPROVAL_REQUEST_RECORDED_PUBLICATION_BLOCKED",
            approval_request_id: request.approval_request_id.to_string(),
            approval_request_receipt_id: approval_receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            source_edit_draft_receipt_id,
            document_id: request.document_id.to_string(),
            draft_id: request.draft_id.to_string(),
            public_visibility_allowed: false,
            search_indexing_allowed: false,
            embedding_provider_allowed: false,
            rich_editor_allowed: false,
            standalone_store_allowed: false,
            production_publish_allowed: false,
            production_write_allowed: false,
        })
    }

    pub fn save_pages_approval_decision_local(
        &mut self,
        request: PagesApprovalDecision<'_>,
    ) -> Result<PagesApprovalDecisionReport, PagesApprovalRequestError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_pages_approval_decision_local_with_identity(request, &identity)
    }

    pub fn save_pages_approval_decision_local_with_identity(
        &mut self,
        request: PagesApprovalDecision<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<PagesApprovalDecisionReport, PagesApprovalRequestError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("approval_decision_id", request.approval_decision_id),
            (
                "approval_request_receipt_id",
                request.approval_request_receipt_id,
            ),
            ("decision_outcome", request.decision_outcome),
            ("decision_note", request.decision_note),
            ("source_route", request.source_route),
        ] {
            if value.trim().is_empty() {
                return Err(PagesApprovalRequestError::Missing(field));
            }
        }
        let approval_request_receipt = self
            .storage
            .ledger()
            .query()
            .by_id(request.approval_request_receipt_id)
            .filter(|receipt| receipt.kind == "pages.approval.requested")
            .cloned()
            .ok_or_else(|| {
                PagesApprovalRequestError::UnknownApprovalRequestReceipt(
                    request.approval_request_receipt_id.to_string(),
                )
            })?;
        if approval_request_receipt.tenant_id.as_str() != request.tenant_id {
            return Err(PagesApprovalRequestError::ApprovalRequestMismatch(
                "the request belongs to another tenant".to_string(),
            ));
        }
        let document_id = approval_request_receipt
            .payload
            .get("document_id")
            .map(String::as_str)
            .unwrap_or("");
        let source_edit_draft_receipt_id = approval_request_receipt
            .payload
            .get("source_edit_draft_receipt_id")
            .map(String::as_str)
            .unwrap_or("");
        let source_edit_draft = self
            .storage
            .ledger()
            .query()
            .by_id(source_edit_draft_receipt_id)
            .filter(|receipt| receipt.kind == "pages.edit.draft.saved")
            .ok_or_else(|| {
                PagesApprovalRequestError::ApprovalRequestMismatch(
                    "the linked edit draft is missing".to_string(),
                )
            })?;
        if source_edit_draft.tenant_id.as_str() != request.tenant_id
            || source_edit_draft
                .payload
                .get("document_id")
                .map(String::as_str)
                .unwrap_or("")
                != document_id
        {
            return Err(PagesApprovalRequestError::ApprovalRequestMismatch(
                "the linked draft belongs to another tenant or document".to_string(),
            ));
        }
        let latest_draft = self
            .storage
            .ledger()
            .entries()
            .iter()
            .rev()
            .find(|receipt| {
                receipt.kind == "pages.edit.draft.saved"
                    && receipt.tenant_id.as_str() == request.tenant_id
                    && receipt
                        .payload
                        .get("document_id")
                        .map(String::as_str)
                        .unwrap_or("")
                        == document_id
            });
        let latest_request = self
            .storage
            .ledger()
            .entries()
            .iter()
            .rev()
            .find(|receipt| {
                receipt.kind == "pages.approval.requested"
                    && receipt.tenant_id.as_str() == request.tenant_id
                    && receipt
                        .payload
                        .get("document_id")
                        .map(String::as_str)
                        .unwrap_or("")
                        == document_id
            });
        if latest_draft.map(|receipt| receipt.receipt_id.as_str())
            != Some(source_edit_draft_receipt_id)
            || latest_request.map(|receipt| receipt.receipt_id.as_str())
                != Some(request.approval_request_receipt_id)
        {
            return Err(PagesApprovalRequestError::ApprovalRequestSuperseded(
                request.approval_request_receipt_id.to_string(),
            ));
        }
        if self.storage.ledger().entries().iter().any(|receipt| {
            receipt.kind == "pages.approval.decision.recorded"
                && receipt
                    .payload
                    .get("approval_request_receipt_id")
                    .map(String::as_str)
                    .unwrap_or("")
                    == request.approval_request_receipt_id
        }) {
            return Err(PagesApprovalRequestError::ApprovalRequestAlreadyDecided(
                request.approval_request_receipt_id.to_string(),
            ));
        }
        let (normalized_outcome, status, human_approval_granted) = match request.decision_outcome {
            "approved" => (
                "approved",
                "PAGE_APPROVAL_APPROVED_PUBLICATION_BLOCKED",
                true,
            ),
            "rejected" => (
                "rejected",
                "PAGE_APPROVAL_REJECTED_PUBLICATION_BLOCKED",
                false,
            ),
            other => {
                return Err(PagesApprovalRequestError::InvalidDecisionOutcome(
                    other.to_string(),
                ));
            }
        };
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "operator",
            request.source_route,
            "pages.approval.decision.recorded",
            request.approval_decision_id,
        )
        .map_err(|error| PagesApprovalRequestError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("pages_approval_decision"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision =
            self.decide_with_receipt(&correlation, ActionKind::RecordPagesApprovalDecision);
        let created_at = local_created_at(
            self.storage
                .ledger()
                .query()
                .by_kind("pages.approval.decision.recorded")
                .len()
                + 1,
        );
        let reviewer_independence =
            if approval_request_receipt.actor_id.as_str() == request.actor_id {
                "self_review_permitted"
            } else {
                "distinct_reviewer"
            };
        let approval_decision_receipt = self.transition_receipt(
            &run_id,
            "RECORD_PAGES_APPROVAL_DECISION",
            &correlation,
            &decision,
            "pages.approval.decision.recorded",
            payload(&[
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("approval_decision_id", request.approval_decision_id),
                ("source_route", request.source_route),
                (
                    "projection_route",
                    "/pages/approval-decisions/projection.json",
                ),
                (
                    "approval_request_receipt_id",
                    request.approval_request_receipt_id,
                ),
                (
                    "approval_request_id",
                    approval_request_receipt
                        .payload
                        .get("approval_request_id")
                        .map(String::as_str)
                        .unwrap_or(""),
                ),
                (
                    "document_id",
                    approval_request_receipt
                        .payload
                        .get("document_id")
                        .map(String::as_str)
                        .unwrap_or(""),
                ),
                (
                    "draft_id",
                    approval_request_receipt
                        .payload
                        .get("draft_id")
                        .map(String::as_str)
                        .unwrap_or(""),
                ),
                ("decision_outcome", normalized_outcome),
                ("decision_note", request.decision_note),
                ("reviewer_independence", reviewer_independence),
                ("self_review_permitted", "true"),
                ("created_at", &created_at),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("terminal_state", status),
                ("human_decision_recorded", "true"),
                (
                    "human_approval_granted",
                    if human_approval_granted {
                        "true"
                    } else {
                        "false"
                    },
                ),
                ("public_visibility_allowed", "false"),
                ("search_indexing_allowed", "false"),
                ("embedding_provider_allowed", "false"),
                ("rich_editor_allowed", "false"),
                ("standalone_store_allowed", "false"),
                ("production_publish_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_pages_approval_decision_run(&run_id, status);
        Ok(PagesApprovalDecisionReport {
            status,
            approval_decision_id: request.approval_decision_id.to_string(),
            approval_decision_receipt_id: approval_decision_receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            approval_request_receipt_id: request.approval_request_receipt_id.to_string(),
            approval_request_id: approval_request_receipt
                .payload
                .get("approval_request_id")
                .cloned()
                .unwrap_or_default(),
            document_id: approval_request_receipt
                .payload
                .get("document_id")
                .cloned()
                .unwrap_or_default(),
            draft_id: approval_request_receipt
                .payload
                .get("draft_id")
                .cloned()
                .unwrap_or_default(),
            decision_outcome: normalized_outcome.to_string(),
            decision_note: request.decision_note.to_string(),
            created_at,
            human_decision_recorded: true,
            human_approval_granted,
            reviewer_independence,
            self_review_permitted: true,
            public_visibility_allowed: false,
            search_indexing_allowed: false,
            embedding_provider_allowed: false,
            rich_editor_allowed: false,
            standalone_store_allowed: false,
            production_publish_allowed: false,
            production_write_allowed: false,
        })
    }

    fn ensure_pages_approval_source(
        &self,
        requested_id: &str,
        tenant_id: &str,
        document_id: &str,
        draft_id: &str,
    ) -> Result<String, PagesApprovalRequestError> {
        if requested_id.trim().is_empty() {
            return Err(PagesApprovalRequestError::Missing(
                "source_edit_draft_receipt_id",
            ));
        }
        let Some(receipt) = self.storage.ledger().query().by_id(requested_id) else {
            return Err(PagesApprovalRequestError::UnknownEditDraftReceipt(
                requested_id.to_string(),
            ));
        };
        if receipt.kind != "pages.edit.draft.saved"
            || receipt.tenant_id.as_str() != tenant_id
            || receipt
                .payload
                .get("document_id")
                .map(String::as_str)
                .unwrap_or("")
                != document_id
            || receipt
                .payload
                .get("draft_id")
                .map(String::as_str)
                .unwrap_or("")
                != draft_id
        {
            return Err(PagesApprovalRequestError::InvalidEditDraftReceipt(
                requested_id.to_string(),
            ));
        }
        Ok(requested_id.to_string())
    }

    fn finish_pages_approval_request_run(&mut self, run_id: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "PAGE_APPROVAL_REQUEST_RECORDED_PUBLICATION_BLOCKED".to_string();
        }
    }

    fn finish_pages_approval_decision_run(&mut self, run_id: &str, status: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = status.to_string();
        }
    }
}

fn local_created_at(sequence: usize) -> String {
    format!("2026-01-01T00:00:{:02}Z", sequence % 60)
}
