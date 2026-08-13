use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagesEditDraft<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub draft_id: &'a str,
    pub document_id: &'a str,
    pub title: &'a str,
    pub body_ref: &'a str,
    pub source_publication_receipt_id: &'a str,
    /// Optional receipt for the object that started this draft, such as a
    /// Message post. This is lineage only and grants no publication authority.
    pub origin_receipt_id: &'a str,
    pub origin_surface: &'a str,
    pub revision_id: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagesEditDraftReport {
    pub status: &'static str,
    pub draft_id: String,
    pub edit_draft_receipt_id: String,
    pub policy_decision_id: String,
    pub source_publication_receipt_id: String,
    pub origin_receipt_id: String,
    pub origin_surface: String,
    pub document_id: String,
    pub rich_editor_allowed: bool,
    pub approval_rail_allowed: bool,
    pub standalone_store_allowed: bool,
    pub production_publish_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PagesEditDraftError {
    Missing(&'static str),
    UnknownPublicationReceipt(String),
    InvalidPublicationReceipt(String),
    UnknownOriginReceipt(String),
    InvalidOriginReceipt(String),
    ActorAdmission(String),
}

impl PagesEditDraftError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("pages edit draft missing {field}"),
            Self::UnknownPublicationReceipt(id) => {
                format!("pages edit draft source receipt {id} is unknown")
            }
            Self::InvalidPublicationReceipt(id) => format!(
                "pages edit draft source receipt {id} is not a publication for the named tenant and document"
            ),
            Self::UnknownOriginReceipt(id) => {
                format!("pages edit draft origin receipt {id} is unknown")
            }
            Self::InvalidOriginReceipt(id) => format!(
                "pages edit draft origin receipt {id} does not match the named tenant and surface"
            ),
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_pages_edit_draft_local(
        &mut self,
        request: PagesEditDraft<'_>,
    ) -> Result<PagesEditDraftReport, PagesEditDraftError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_pages_edit_draft_local_with_identity(request, &identity)
    }

    pub fn save_pages_edit_draft_local_with_identity(
        &mut self,
        request: PagesEditDraft<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<PagesEditDraftReport, PagesEditDraftError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("draft_id", request.draft_id),
            ("document_id", request.document_id),
            ("title", request.title),
            ("body_ref", request.body_ref),
            ("revision_id", request.revision_id),
        ] {
            if value.trim().is_empty() {
                return Err(PagesEditDraftError::Missing(field));
            }
        }
        let source_publication_receipt_id = self.ensure_pages_edit_draft_source(
            request.source_publication_receipt_id,
            request.tenant_id,
            request.document_id,
        )?;
        let (origin_receipt_id, origin_surface) = self.ensure_pages_edit_draft_origin(
            request.origin_receipt_id,
            request.origin_surface,
            request.tenant_id,
        )?;
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "operator",
            "/pages/edit-drafts.json",
            "pages.edit.draft.saved",
            request.draft_id,
        )
        .map_err(|error| PagesEditDraftError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("pages_edit_draft"),
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
        let decision = self.decide_with_receipt(&correlation, ActionKind::SavePagesEditDraft);
        let draft_receipt = self.transition_receipt(
            &run_id,
            "SAVE_PAGES_EDIT_DRAFT",
            &correlation,
            &decision,
            "pages.edit.draft.saved",
            payload(&[
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("draft_id", request.draft_id),
                ("document_id", request.document_id),
                ("title", request.title),
                ("body_ref", request.body_ref),
                ("source_route", "/pages/edit-drafts.json"),
                ("projection_route", "/pages/edit-drafts/projection.json"),
                (
                    "source_publication_receipt_id",
                    &source_publication_receipt_id,
                ),
                ("origin_receipt_id", &origin_receipt_id),
                ("origin_surface", &origin_surface),
                ("origin_grants_authority", "false"),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("revision_id", request.revision_id),
                ("terminal_state", "PAGE_EDIT_DRAFT_SAVED_EDITOR_BLOCKED"),
                ("rich_editor_allowed", "false"),
                ("approval_rail_allowed", "false"),
                ("standalone_store_allowed", "false"),
                ("production_publish_allowed", "false"),
            ]),
        );
        self.finish_pages_edit_draft_run(&run_id);
        Ok(PagesEditDraftReport {
            status: "PAGE_EDIT_DRAFT_SAVED_EDITOR_BLOCKED",
            draft_id: request.draft_id.to_string(),
            edit_draft_receipt_id: draft_receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            source_publication_receipt_id,
            origin_receipt_id,
            origin_surface,
            document_id: request.document_id.to_string(),
            rich_editor_allowed: false,
            approval_rail_allowed: false,
            standalone_store_allowed: false,
            production_publish_allowed: false,
        })
    }

    fn ensure_pages_edit_draft_source(
        &self,
        requested_id: &str,
        tenant_id: &str,
        document_id: &str,
    ) -> Result<String, PagesEditDraftError> {
        if requested_id.trim().is_empty() {
            return Ok(String::new());
        }
        let Some(receipt) = self.storage.ledger().query().by_id(requested_id) else {
            return Err(PagesEditDraftError::UnknownPublicationReceipt(
                requested_id.to_string(),
            ));
        };
        if receipt.kind != "pages.document.published"
            || receipt.tenant_id.as_str() != tenant_id
            || receipt
                .payload
                .get("document_id")
                .map(String::as_str)
                .unwrap_or("")
                != document_id
        {
            return Err(PagesEditDraftError::InvalidPublicationReceipt(
                requested_id.to_string(),
            ));
        }
        Ok(requested_id.to_string())
    }

    fn ensure_pages_edit_draft_origin(
        &self,
        requested_id: &str,
        requested_surface: &str,
        tenant_id: &str,
    ) -> Result<(String, String), PagesEditDraftError> {
        let receipt_id = requested_id.trim();
        let surface = requested_surface.trim().to_ascii_lowercase();
        if receipt_id.is_empty() && surface.is_empty() {
            return Ok((String::new(), String::new()));
        }
        if receipt_id.is_empty() || surface.is_empty() {
            return Err(PagesEditDraftError::InvalidOriginReceipt(
                receipt_id.to_string(),
            ));
        }
        let Some(receipt) = self.storage.ledger().query().by_id(receipt_id) else {
            return Err(PagesEditDraftError::UnknownOriginReceipt(
                receipt_id.to_string(),
            ));
        };
        let kind_matches = match surface.as_str() {
            "message" => receipt.kind.starts_with("message."),
            "forge" => receipt.kind.starts_with("forge."),
            "twin" => receipt.kind.starts_with("twin."),
            "pages" => receipt.kind.starts_with("pages."),
            _ => false,
        };
        if receipt.tenant_id.as_str() != tenant_id || !kind_matches {
            return Err(PagesEditDraftError::InvalidOriginReceipt(
                receipt_id.to_string(),
            ));
        }
        Ok((receipt_id.to_string(), surface))
    }

    fn finish_pages_edit_draft_run(&mut self, run_id: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "PAGE_EDIT_DRAFT_SAVED_EDITOR_BLOCKED".to_string();
        }
    }
}
