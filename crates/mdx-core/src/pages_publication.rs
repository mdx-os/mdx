use crate::*;

/// The closed vocabulary of page kinds. Type is first-class: a page declares
/// what it IS, so the library can sort and the agent path can filter by shape
/// without parsing titles. Advisory to authoring, but recorded on the receipt
/// so the projection and the world model carry it. `knowledge` is the default
/// for an untyped page - the plain document with no stronger claim.
pub const PAGE_TYPES: [&str; 6] = [
    "knowledge",
    "spec",
    "decision",
    "standard",
    "signal",
    "changelog",
];

/// Normalize a requested page type to one of the sanctioned kinds. Empty means
/// the author did not declare one, which is the honest default `knowledge`; a
/// non-empty value outside the vocabulary is refused rather than silently
/// coerced, so a typo never lands as the wrong shape.
pub fn normalize_page_type(raw: &str) -> Result<&'static str, PagesPublicationError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok("knowledge");
    }
    let lowered = trimmed.to_ascii_lowercase();
    PAGE_TYPES
        .iter()
        .copied()
        .find(|kind| *kind == lowered)
        .ok_or_else(|| PagesPublicationError::InvalidType(trimmed.to_string()))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagesPublication<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub document_id: &'a str,
    pub title: &'a str,
    pub body_ref: &'a str,
    pub source_receipt_id: &'a str,
    pub revision_id: &'a str,
    /// The page kind, one of PAGE_TYPES. Empty defaults to `knowledge`.
    pub page_type: &'a str,
}

/// A publication requested through the human Pages editor. The kernel derives
/// the title, body reference, revision, and source receipt from the approved
/// draft chain. The client cannot substitute different words after approval.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagesApprovedPublication<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub document_id: &'a str,
    pub approval_decision_receipt_id: &'a str,
    /// The page kind, one of PAGE_TYPES. Empty defaults to `knowledge`.
    pub page_type: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagesPublicationReport {
    pub status: &'static str,
    pub document_id: String,
    pub title: String,
    pub body_ref: String,
    pub publication_receipt_id: String,
    pub policy_decision_id: String,
    pub source_receipt_id: String,
    pub revision_id: String,
    pub approval_decision_receipt_id: String,
    pub source_edit_draft_receipt_id: String,
    pub origin_receipt_id: String,
    pub origin_surface: String,
    pub approval_binding: &'static str,
    pub reviewer_independence: &'static str,
    pub page_type: &'static str,
    pub visibility: &'static str,
    pub standalone_store_allowed: bool,
    pub rich_editor_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PagesPublicationError {
    Missing(&'static str),
    UnknownSourceReceipt(String),
    UnknownApprovalDecision(String),
    ApprovalNotGranted(String),
    ApprovalChainMismatch(String),
    ApprovalSuperseded(String),
    ApprovalAlreadyConsumed(String),
    ActorAdmission(String),
    InvalidType(String),
}

impl PagesPublicationError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("pages publication missing {field}"),
            Self::UnknownSourceReceipt(id) => {
                format!("pages publication source receipt {id} is unknown")
            }
            Self::UnknownApprovalDecision(id) => {
                format!("pages publication approval decision {id} is unknown")
            }
            Self::ApprovalNotGranted(id) => {
                format!("pages publication approval decision {id} did not grant approval")
            }
            Self::ApprovalChainMismatch(detail) => {
                format!("pages publication approval chain mismatch: {detail}")
            }
            Self::ApprovalSuperseded(id) => format!(
                "pages publication approval decision {id} no longer covers the latest draft and decision"
            ),
            Self::ApprovalAlreadyConsumed(id) => {
                format!("pages publication approval decision {id} was already published")
            }
            Self::ActorAdmission(message) => message.clone(),
            Self::InvalidType(kind) => format!(
                "pages publication type {kind} is not one of {}",
                PAGE_TYPES.join(", ")
            ),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_pages_publication_local(
        &mut self,
        request: PagesPublication<'_>,
    ) -> Result<PagesPublicationReport, PagesPublicationError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_pages_publication_local_with_identity(request, &identity)
    }

    pub fn save_pages_publication_local_with_identity(
        &mut self,
        request: PagesPublication<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<PagesPublicationReport, PagesPublicationError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("document_id", request.document_id),
            ("title", request.title),
            ("body_ref", request.body_ref),
            ("source_receipt_id", request.source_receipt_id),
            ("revision_id", request.revision_id),
        ] {
            if value.trim().is_empty() {
                return Err(PagesPublicationError::Missing(field));
            }
        }
        normalize_page_type(request.page_type)?;
        if self
            .storage
            .ledger()
            .query()
            .by_id(request.source_receipt_id)
            .is_none()
        {
            return Err(PagesPublicationError::UnknownSourceReceipt(
                request.source_receipt_id.to_string(),
            ));
        }
        self.record_pages_publication(request, identity, None)
    }

    pub fn save_pages_approved_publication_local(
        &mut self,
        request: PagesApprovedPublication<'_>,
    ) -> Result<PagesPublicationReport, PagesPublicationError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_pages_approved_publication_local_with_identity(request, &identity)
    }

    pub fn save_pages_approved_publication_local_with_identity(
        &mut self,
        request: PagesApprovedPublication<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<PagesPublicationReport, PagesPublicationError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("document_id", request.document_id),
            (
                "approval_decision_receipt_id",
                request.approval_decision_receipt_id,
            ),
        ] {
            if value.trim().is_empty() {
                return Err(PagesPublicationError::Missing(field));
            }
        }
        normalize_page_type(request.page_type)?;

        let approval_decision = self
            .storage
            .ledger()
            .query()
            .by_id(request.approval_decision_receipt_id)
            .filter(|receipt| receipt.kind == "pages.approval.decision.recorded")
            .cloned()
            .ok_or_else(|| {
                PagesPublicationError::UnknownApprovalDecision(
                    request.approval_decision_receipt_id.to_string(),
                )
            })?;
        if approval_decision.tenant_id.as_str() != request.tenant_id {
            return Err(PagesPublicationError::ApprovalChainMismatch(
                "the decision belongs to another tenant".to_string(),
            ));
        }
        if publication_payload_value(&approval_decision, "decision_outcome") != "approved" {
            return Err(PagesPublicationError::ApprovalNotGranted(
                request.approval_decision_receipt_id.to_string(),
            ));
        }
        if publication_payload_value(&approval_decision, "document_id") != request.document_id {
            return Err(PagesPublicationError::ApprovalChainMismatch(
                "the decision names another document".to_string(),
            ));
        }

        let approval_request_receipt_id =
            publication_payload_value(&approval_decision, "approval_request_receipt_id");
        let approval_request = self
            .storage
            .ledger()
            .query()
            .by_id(approval_request_receipt_id)
            .filter(|receipt| receipt.kind == "pages.approval.requested")
            .cloned()
            .ok_or_else(|| {
                PagesPublicationError::ApprovalChainMismatch(
                    "the linked approval request is missing".to_string(),
                )
            })?;
        if approval_request.tenant_id.as_str() != request.tenant_id
            || publication_payload_value(&approval_request, "document_id") != request.document_id
        {
            return Err(PagesPublicationError::ApprovalChainMismatch(
                "the request belongs to another tenant or document".to_string(),
            ));
        }

        let source_edit_draft_receipt_id =
            publication_payload_value(&approval_request, "source_edit_draft_receipt_id");
        let edit_draft = self
            .storage
            .ledger()
            .query()
            .by_id(source_edit_draft_receipt_id)
            .filter(|receipt| receipt.kind == "pages.edit.draft.saved")
            .cloned()
            .ok_or_else(|| {
                PagesPublicationError::ApprovalChainMismatch(
                    "the linked edit draft is missing".to_string(),
                )
            })?;
        if edit_draft.tenant_id.as_str() != request.tenant_id
            || publication_payload_value(&edit_draft, "document_id") != request.document_id
            || publication_payload_value(&edit_draft, "draft_id")
                != publication_payload_value(&approval_request, "draft_id")
        {
            return Err(PagesPublicationError::ApprovalChainMismatch(
                "the draft belongs to another tenant, document, or request".to_string(),
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
                    && publication_payload_value(receipt, "document_id") == request.document_id
            });
        let latest_decision = self
            .storage
            .ledger()
            .entries()
            .iter()
            .rev()
            .find(|receipt| {
                receipt.kind == "pages.approval.decision.recorded"
                    && receipt.tenant_id.as_str() == request.tenant_id
                    && publication_payload_value(receipt, "document_id") == request.document_id
            });
        if latest_draft.map(|receipt| receipt.receipt_id.as_str())
            != Some(source_edit_draft_receipt_id)
            || latest_decision.map(|receipt| receipt.receipt_id.as_str())
                != Some(request.approval_decision_receipt_id)
        {
            return Err(PagesPublicationError::ApprovalSuperseded(
                request.approval_decision_receipt_id.to_string(),
            ));
        }
        if self.storage.ledger().entries().iter().any(|receipt| {
            receipt.kind == "pages.document.published"
                && publication_payload_value(receipt, "approval_decision_receipt_id")
                    == request.approval_decision_receipt_id
        }) {
            return Err(PagesPublicationError::ApprovalAlreadyConsumed(
                request.approval_decision_receipt_id.to_string(),
            ));
        }

        let title = publication_payload_value(&edit_draft, "title").to_string();
        let body_ref = publication_payload_value(&edit_draft, "body_ref").to_string();
        let revision_id = publication_payload_value(&edit_draft, "revision_id").to_string();
        for (field, value) in [
            ("approved_title", title.as_str()),
            ("approved_body_ref", body_ref.as_str()),
            ("approved_revision_id", revision_id.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(PagesPublicationError::ApprovalChainMismatch(format!(
                    "the approved draft is missing {field}"
                )));
            }
        }
        let reviewer_independence =
            if approval_request.actor_id.as_str() == approval_decision.actor_id.as_str() {
                "self_review_permitted"
            } else {
                "distinct_reviewer"
            };
        let binding = PagesPublicationApprovalBinding {
            approval_decision_receipt_id: request.approval_decision_receipt_id.to_string(),
            approval_request_receipt_id: approval_request_receipt_id.to_string(),
            source_edit_draft_receipt_id: source_edit_draft_receipt_id.to_string(),
            origin_receipt_id: publication_payload_value(&edit_draft, "origin_receipt_id")
                .to_string(),
            origin_surface: publication_payload_value(&edit_draft, "origin_surface").to_string(),
            reviewer_independence,
        };
        self.record_pages_publication(
            PagesPublication {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                document_id: request.document_id,
                title: &title,
                body_ref: &body_ref,
                source_receipt_id: source_edit_draft_receipt_id,
                revision_id: &revision_id,
                page_type: request.page_type,
            },
            identity,
            Some(&binding),
        )
    }

    fn record_pages_publication(
        &mut self,
        request: PagesPublication<'_>,
        identity: &GovernedWriteIdentity,
        approval: Option<&PagesPublicationApprovalBinding>,
    ) -> Result<PagesPublicationReport, PagesPublicationError> {
        let page_type = normalize_page_type(request.page_type)?;
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "operator",
            "/pages/publications.json",
            "pages.document.published",
            request.document_id,
        )
        .map_err(|error| PagesPublicationError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("pages_publication"),
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
        let decision = self.decide_with_receipt(&correlation, ActionKind::PublishPagesDocument);
        let approval_decision_receipt_id = approval
            .map(|binding| binding.approval_decision_receipt_id.as_str())
            .unwrap_or("");
        let approval_request_receipt_id = approval
            .map(|binding| binding.approval_request_receipt_id.as_str())
            .unwrap_or("");
        let source_edit_draft_receipt_id = approval
            .map(|binding| binding.source_edit_draft_receipt_id.as_str())
            .unwrap_or("");
        let origin_receipt_id = approval
            .map(|binding| binding.origin_receipt_id.as_str())
            .unwrap_or("");
        let origin_surface = approval
            .map(|binding| binding.origin_surface.as_str())
            .unwrap_or("");
        let approval_binding = if approval.is_some() {
            "approved_draft_content_bound"
        } else {
            "system_or_legacy_source_receipt"
        };
        let reviewer_independence = approval
            .map(|binding| binding.reviewer_independence)
            .unwrap_or("not_applicable");
        let publication_receipt = self.transition_receipt(
            &run_id,
            "PUBLISH_PAGES_PROJECTION",
            &correlation,
            &decision,
            "pages.document.published",
            payload(&[
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("document_id", request.document_id),
                ("title", request.title),
                ("page_type", page_type),
                ("body_ref", request.body_ref),
                ("source_receipt_id", request.source_receipt_id),
                ("approval_decision_receipt_id", approval_decision_receipt_id),
                ("approval_request_receipt_id", approval_request_receipt_id),
                ("source_edit_draft_receipt_id", source_edit_draft_receipt_id),
                ("origin_receipt_id", origin_receipt_id),
                ("origin_surface", origin_surface),
                ("origin_grants_authority", "false"),
                ("approval_binding", approval_binding),
                (
                    "human_approval_granted",
                    if approval.is_some() { "true" } else { "false" },
                ),
                ("reviewer_independence", reviewer_independence),
                ("revision_id", request.revision_id),
                ("visibility", "tenant_only"),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                (
                    "world_model_source",
                    "generated/world-model/pages-projection-fixtures.json",
                ),
                ("terminal_state", "PAGE_PUBLISHED_EDITOR_BLOCKED"),
                ("standalone_store_allowed", "false"),
                ("rich_editor_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_pages_publication_run(&run_id);
        Ok(PagesPublicationReport {
            status: "PAGE_PUBLISHED_EDITOR_BLOCKED",
            document_id: request.document_id.to_string(),
            title: request.title.to_string(),
            body_ref: request.body_ref.to_string(),
            publication_receipt_id: publication_receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            source_receipt_id: request.source_receipt_id.to_string(),
            revision_id: request.revision_id.to_string(),
            approval_decision_receipt_id: approval_decision_receipt_id.to_string(),
            source_edit_draft_receipt_id: source_edit_draft_receipt_id.to_string(),
            origin_receipt_id: origin_receipt_id.to_string(),
            origin_surface: origin_surface.to_string(),
            approval_binding,
            reviewer_independence,
            page_type,
            visibility: "tenant_only",
            standalone_store_allowed: false,
            rich_editor_allowed: false,
            production_write_allowed: false,
        })
    }

    fn finish_pages_publication_run(&mut self, run_id: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "PAGE_PUBLISHED_EDITOR_BLOCKED".to_string();
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PagesPublicationApprovalBinding {
    approval_decision_receipt_id: String,
    approval_request_receipt_id: String,
    source_edit_draft_receipt_id: String,
    origin_receipt_id: String,
    origin_surface: String,
    reviewer_independence: &'static str,
}

fn publication_payload_value<'a>(receipt: &'a Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}
