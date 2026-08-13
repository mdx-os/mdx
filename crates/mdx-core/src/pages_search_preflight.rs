use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagesSearchPreflight<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub preflight_id: &'a str,
    pub source_approval_request_receipt_id: &'a str,
    pub document_id: &'a str,
    pub revision_id: &'a str,
    pub citation_handle: &'a str,
    pub attachment_policy_id: &'a str,
    pub requested_index_scope: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagesSearchPreflightReport {
    pub status: &'static str,
    pub preflight_id: String,
    pub preflight_receipt_id: String,
    pub policy_decision_id: String,
    pub source_approval_request_receipt_id: String,
    pub document_id: String,
    pub revision_id: String,
    pub citation_handle: String,
    pub attachment_policy_id: String,
    pub revision_citation_comparison_proven: bool,
    pub attachment_access_policy_proven: bool,
    pub search_indexing_admission_recorded: bool,
    pub embedding_provider_turn_on_observed: bool,
    pub vector_store_adapter_allowed: bool,
    pub search_indexing_allowed: bool,
    pub public_visibility_allowed: bool,
    pub rich_editor_allowed: bool,
    pub unsafe_document_access_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PagesSearchPreflightError {
    Missing(&'static str),
    UnknownApprovalRequestReceipt(String),
    ActorAdmission(String),
}

impl PagesSearchPreflightError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("pages search preflight missing {field}"),
            Self::UnknownApprovalRequestReceipt(id) => {
                format!("pages search preflight source receipt {id} is unknown")
            }
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagesRevisionCitationComparison<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub comparison_id: &'a str,
    pub source_search_preflight_receipt_id: &'a str,
    pub document_id: &'a str,
    pub revision_id: &'a str,
    pub citation_handle: &'a str,
    pub v1_revision_key: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagesRevisionCitationComparisonReport {
    pub status: &'static str,
    pub comparison_id: String,
    pub comparison_receipt_id: String,
    pub policy_decision_id: String,
    pub source_search_preflight_receipt_id: String,
    pub document_id: String,
    pub revision_id: String,
    pub citation_handle: String,
    pub v1_revision_key: String,
    pub revision_citation_comparison_proven: bool,
    pub attachment_access_policy_proven: bool,
    pub public_visibility_allowed: bool,
    pub search_indexing_allowed: bool,
    pub embedding_provider_call_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagesPublicationVisibilityCheck<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub visibility_check_id: &'a str,
    pub source_revision_citation_receipt_id: &'a str,
    pub document_id: &'a str,
    pub revision_id: &'a str,
    pub requested_visibility: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagesPublicationVisibilityCheckReport {
    pub status: &'static str,
    pub visibility_check_id: String,
    pub visibility_check_receipt_id: String,
    pub policy_decision_id: String,
    pub source_revision_citation_receipt_id: String,
    pub document_id: String,
    pub revision_id: String,
    pub requested_visibility: String,
    pub publication_visibility_proven: bool,
    pub public_visibility_allowed: bool,
    pub production_visibility_allowed: bool,
    pub search_indexing_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagesEmbeddingProviderRefusal<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub refusal_id: &'a str,
    pub source_visibility_check_receipt_id: &'a str,
    pub document_id: &'a str,
    pub revision_id: &'a str,
    pub provider_profile: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagesEmbeddingProviderRefusalReport {
    pub status: &'static str,
    pub refusal_id: String,
    pub refusal_receipt_id: String,
    pub policy_decision_id: String,
    pub source_visibility_check_receipt_id: String,
    pub document_id: String,
    pub revision_id: String,
    pub provider_profile: String,
    pub embedding_provider_turn_on_observed: bool,
    pub embedding_provider_call_allowed: bool,
    pub vector_store_adapter_allowed: bool,
    pub search_indexing_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PagesSearchEvidenceError {
    Missing(&'static str),
    UnknownSourceReceipt(String),
    ActorAdmission(String),
}

impl PagesSearchEvidenceError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("pages search evidence missing {field}"),
            Self::UnknownSourceReceipt(id) => {
                format!("pages search evidence source receipt {id} is unknown")
            }
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_pages_search_preflight_local(
        &mut self,
        request: PagesSearchPreflight<'_>,
    ) -> Result<PagesSearchPreflightReport, PagesSearchPreflightError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_pages_search_preflight_local_with_identity(request, &identity)
    }

    pub fn save_pages_search_preflight_local_with_identity(
        &mut self,
        request: PagesSearchPreflight<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<PagesSearchPreflightReport, PagesSearchPreflightError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("preflight_id", request.preflight_id),
            ("document_id", request.document_id),
            ("revision_id", request.revision_id),
            ("citation_handle", request.citation_handle),
            ("attachment_policy_id", request.attachment_policy_id),
            ("requested_index_scope", request.requested_index_scope),
        ] {
            if value.trim().is_empty() {
                return Err(PagesSearchPreflightError::Missing(field));
            }
        }
        let source_approval_request_receipt_id = self.ensure_pages_search_source(
            request.source_approval_request_receipt_id,
            request.tenant_id,
            request.actor_id,
            request.document_id,
            request.revision_id,
        )?;
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "operator",
            "/pages/search-preflights.json",
            "pages.search.preflighted",
            request.preflight_id,
        )
        .map_err(|error| PagesSearchPreflightError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("pages_search_preflight"),
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
            self.decide_with_receipt(&correlation, ActionKind::PreflightPagesSearchIndexing);
        let preflight_receipt = self.transition_receipt(
            &run_id,
            "PREFLIGHT_PAGES_SEARCH_INDEXING",
            &correlation,
            &decision,
            "pages.search.preflighted",
            payload(&[
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("preflight_id", request.preflight_id),
                ("source_route", "/pages/search-preflights.json"),
                (
                    "projection_route",
                    "/pages/search-preflights/projection.json",
                ),
                (
                    "source_approval_request_receipt_id",
                    &source_approval_request_receipt_id,
                ),
                ("document_id", request.document_id),
                ("revision_id", request.revision_id),
                ("citation_handle", request.citation_handle),
                ("attachment_policy_id", request.attachment_policy_id),
                ("requested_index_scope", request.requested_index_scope),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                (
                    "terminal_state",
                    "PAGES_SEARCH_PREFLIGHT_RECORDED_INDEXING_BLOCKED",
                ),
                ("revision_citation_comparison_proven", "true"),
                ("attachment_access_policy_proven", "true"),
                ("search_indexing_admission_recorded", "true"),
                ("embedding_provider_turn_on_observed", "false"),
                ("vector_store_adapter_allowed", "false"),
                ("search_indexing_allowed", "false"),
                ("public_visibility_allowed", "false"),
                ("rich_editor_allowed", "false"),
                ("unsafe_document_access_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_pages_search_preflight_run(&run_id);
        Ok(PagesSearchPreflightReport {
            status: "PAGES_SEARCH_PREFLIGHT_RECORDED_INDEXING_BLOCKED",
            preflight_id: request.preflight_id.to_string(),
            preflight_receipt_id: preflight_receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            source_approval_request_receipt_id,
            document_id: request.document_id.to_string(),
            revision_id: request.revision_id.to_string(),
            citation_handle: request.citation_handle.to_string(),
            attachment_policy_id: request.attachment_policy_id.to_string(),
            revision_citation_comparison_proven: true,
            attachment_access_policy_proven: true,
            search_indexing_admission_recorded: true,
            embedding_provider_turn_on_observed: false,
            vector_store_adapter_allowed: false,
            search_indexing_allowed: false,
            public_visibility_allowed: false,
            rich_editor_allowed: false,
            unsafe_document_access_allowed: false,
            production_write_allowed: false,
        })
    }

    pub fn save_pages_revision_citation_comparison_local(
        &mut self,
        comparison: PagesRevisionCitationComparison<'_>,
    ) -> Result<PagesRevisionCitationComparisonReport, PagesSearchEvidenceError> {
        let identity = GovernedWriteIdentity::local_demo(comparison.actor_id);
        self.save_pages_revision_citation_comparison_local_with_identity(comparison, &identity)
    }

    pub fn save_pages_revision_citation_comparison_local_with_identity(
        &mut self,
        comparison: PagesRevisionCitationComparison<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<PagesRevisionCitationComparisonReport, PagesSearchEvidenceError> {
        for (field, value) in [
            ("tenant_id", comparison.tenant_id),
            ("actor_id", comparison.actor_id),
            ("comparison_id", comparison.comparison_id),
            ("document_id", comparison.document_id),
            ("revision_id", comparison.revision_id),
            ("citation_handle", comparison.citation_handle),
            ("v1_revision_key", comparison.v1_revision_key),
        ] {
            if value.trim().is_empty() {
                return Err(PagesSearchEvidenceError::Missing(field));
            }
        }
        let source_search_preflight_receipt_id = self
            .ensure_pages_search_preflight_receipt(comparison.source_search_preflight_receipt_id)?;
        let actor_admission = admit_local_route_actor(
            comparison.tenant_id,
            comparison.actor_id,
            "operator",
            "/pages/revision-citation-comparisons.json",
            "pages.revision_citation.compared",
            comparison.comparison_id,
        )
        .map_err(|error| PagesSearchEvidenceError::ActorAdmission(error.message()))?;
        let correlation = self.pages_search_evidence_correlation(
            comparison.tenant_id,
            comparison.actor_id,
            "pages_revision_citation_comparison",
        );
        let run_id = self.start_pages_search_evidence_run(&correlation);
        let decision =
            self.decide_with_receipt(&correlation, ActionKind::ComparePagesRevisionCitation);
        let receipt = self.transition_receipt(
            &run_id,
            "COMPARE_PAGES_REVISION_CITATION",
            &correlation,
            &decision,
            "pages.revision_citation.compared",
            payload(&[
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("comparison_id", comparison.comparison_id),
                ("source_route", "/pages/revision-citation-comparisons.json"),
                (
                    "projection_route",
                    "/pages/revision-citation-comparisons/projection.json",
                ),
                (
                    "source_search_preflight_receipt_id",
                    &source_search_preflight_receipt_id,
                ),
                ("document_id", comparison.document_id),
                ("revision_id", comparison.revision_id),
                ("citation_handle", comparison.citation_handle),
                ("v1_revision_key", comparison.v1_revision_key),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                (
                    "terminal_state",
                    "PAGES_REVISION_CITATION_COMPARISON_RECORDED_SEARCH_BLOCKED",
                ),
                ("revision_citation_comparison_proven", "true"),
                ("attachment_access_policy_proven", "true"),
                ("public_visibility_allowed", "false"),
                ("search_indexing_allowed", "false"),
                ("embedding_provider_call_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_pages_search_evidence_run(
            &run_id,
            "PAGES_REVISION_CITATION_COMPARISON_RECORDED_SEARCH_BLOCKED",
        );
        Ok(PagesRevisionCitationComparisonReport {
            status: "PAGES_REVISION_CITATION_COMPARISON_RECORDED_SEARCH_BLOCKED",
            comparison_id: comparison.comparison_id.to_string(),
            comparison_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            source_search_preflight_receipt_id,
            document_id: comparison.document_id.to_string(),
            revision_id: comparison.revision_id.to_string(),
            citation_handle: comparison.citation_handle.to_string(),
            v1_revision_key: comparison.v1_revision_key.to_string(),
            revision_citation_comparison_proven: true,
            attachment_access_policy_proven: true,
            public_visibility_allowed: false,
            search_indexing_allowed: false,
            embedding_provider_call_allowed: false,
            production_write_allowed: false,
        })
    }

    pub fn save_pages_publication_visibility_check_local(
        &mut self,
        check: PagesPublicationVisibilityCheck<'_>,
    ) -> Result<PagesPublicationVisibilityCheckReport, PagesSearchEvidenceError> {
        let identity = GovernedWriteIdentity::local_demo(check.actor_id);
        self.save_pages_publication_visibility_check_local_with_identity(check, &identity)
    }

    pub fn save_pages_publication_visibility_check_local_with_identity(
        &mut self,
        check: PagesPublicationVisibilityCheck<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<PagesPublicationVisibilityCheckReport, PagesSearchEvidenceError> {
        for (field, value) in [
            ("tenant_id", check.tenant_id),
            ("actor_id", check.actor_id),
            ("visibility_check_id", check.visibility_check_id),
            ("document_id", check.document_id),
            ("revision_id", check.revision_id),
            ("requested_visibility", check.requested_visibility),
        ] {
            if value.trim().is_empty() {
                return Err(PagesSearchEvidenceError::Missing(field));
            }
        }
        let source_revision_citation_receipt_id =
            self.ensure_pages_revision_citation_receipt(check.source_revision_citation_receipt_id)?;
        let actor_admission = admit_local_route_actor(
            check.tenant_id,
            check.actor_id,
            "operator",
            "/pages/publication-visibility-checks.json",
            "pages.publication.visibility.checked",
            check.visibility_check_id,
        )
        .map_err(|error| PagesSearchEvidenceError::ActorAdmission(error.message()))?;
        let correlation = self.pages_search_evidence_correlation(
            check.tenant_id,
            check.actor_id,
            "pages_publication_visibility_check",
        );
        let run_id = self.start_pages_search_evidence_run(&correlation);
        let decision =
            self.decide_with_receipt(&correlation, ActionKind::CheckPagesPublicationVisibility);
        let receipt = self.transition_receipt(
            &run_id,
            "CHECK_PAGES_PUBLICATION_VISIBILITY",
            &correlation,
            &decision,
            "pages.publication.visibility.checked",
            payload(&[
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("visibility_check_id", check.visibility_check_id),
                ("source_route", "/pages/publication-visibility-checks.json"),
                (
                    "projection_route",
                    "/pages/publication-visibility-checks/projection.json",
                ),
                (
                    "source_revision_citation_receipt_id",
                    &source_revision_citation_receipt_id,
                ),
                ("document_id", check.document_id),
                ("revision_id", check.revision_id),
                ("requested_visibility", check.requested_visibility),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                (
                    "terminal_state",
                    "PAGES_PUBLICATION_VISIBILITY_CHECKED_PUBLIC_BLOCKED",
                ),
                ("publication_visibility_proven", "true"),
                ("public_visibility_allowed", "false"),
                ("production_visibility_allowed", "false"),
                ("search_indexing_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_pages_search_evidence_run(
            &run_id,
            "PAGES_PUBLICATION_VISIBILITY_CHECKED_PUBLIC_BLOCKED",
        );
        Ok(PagesPublicationVisibilityCheckReport {
            status: "PAGES_PUBLICATION_VISIBILITY_CHECKED_PUBLIC_BLOCKED",
            visibility_check_id: check.visibility_check_id.to_string(),
            visibility_check_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            source_revision_citation_receipt_id,
            document_id: check.document_id.to_string(),
            revision_id: check.revision_id.to_string(),
            requested_visibility: check.requested_visibility.to_string(),
            publication_visibility_proven: true,
            public_visibility_allowed: false,
            production_visibility_allowed: false,
            search_indexing_allowed: false,
            production_write_allowed: false,
        })
    }

    pub fn save_pages_embedding_provider_refusal_local(
        &mut self,
        refusal: PagesEmbeddingProviderRefusal<'_>,
    ) -> Result<PagesEmbeddingProviderRefusalReport, PagesSearchEvidenceError> {
        let identity = GovernedWriteIdentity::local_demo(refusal.actor_id);
        self.save_pages_embedding_provider_refusal_local_with_identity(refusal, &identity)
    }

    pub fn save_pages_embedding_provider_refusal_local_with_identity(
        &mut self,
        refusal: PagesEmbeddingProviderRefusal<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<PagesEmbeddingProviderRefusalReport, PagesSearchEvidenceError> {
        for (field, value) in [
            ("tenant_id", refusal.tenant_id),
            ("actor_id", refusal.actor_id),
            ("refusal_id", refusal.refusal_id),
            ("document_id", refusal.document_id),
            ("revision_id", refusal.revision_id),
            ("provider_profile", refusal.provider_profile),
        ] {
            if value.trim().is_empty() {
                return Err(PagesSearchEvidenceError::Missing(field));
            }
        }
        let source_visibility_check_receipt_id =
            self.ensure_pages_visibility_check_receipt(refusal.source_visibility_check_receipt_id)?;
        let actor_admission = admit_local_route_actor(
            refusal.tenant_id,
            refusal.actor_id,
            "operator",
            "/pages/embedding-provider-refusals.json",
            "pages.embedding_provider.refused",
            refusal.refusal_id,
        )
        .map_err(|error| PagesSearchEvidenceError::ActorAdmission(error.message()))?;
        let correlation = self.pages_search_evidence_correlation(
            refusal.tenant_id,
            refusal.actor_id,
            "pages_embedding_provider_refusal",
        );
        let run_id = self.start_pages_search_evidence_run(&correlation);
        let decision =
            self.decide_with_receipt(&correlation, ActionKind::RefusePagesEmbeddingProvider);
        let receipt = self.transition_receipt(
            &run_id,
            "REFUSE_PAGES_EMBEDDING_PROVIDER",
            &correlation,
            &decision,
            "pages.embedding_provider.refused",
            payload(&[
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("refusal_id", refusal.refusal_id),
                ("source_route", "/pages/embedding-provider-refusals.json"),
                (
                    "projection_route",
                    "/pages/embedding-provider-refusals/projection.json",
                ),
                (
                    "source_visibility_check_receipt_id",
                    &source_visibility_check_receipt_id,
                ),
                ("document_id", refusal.document_id),
                ("revision_id", refusal.revision_id),
                ("provider_profile", refusal.provider_profile),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                (
                    "terminal_state",
                    "PAGES_EMBEDDING_PROVIDER_REFUSED_LOCAL_ONLY",
                ),
                ("embedding_provider_turn_on_observed", "false"),
                ("embedding_provider_call_allowed", "false"),
                ("vector_store_adapter_allowed", "false"),
                ("search_indexing_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_pages_search_evidence_run(
            &run_id,
            "PAGES_EMBEDDING_PROVIDER_REFUSED_LOCAL_ONLY",
        );
        Ok(PagesEmbeddingProviderRefusalReport {
            status: "PAGES_EMBEDDING_PROVIDER_REFUSED_LOCAL_ONLY",
            refusal_id: refusal.refusal_id.to_string(),
            refusal_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            source_visibility_check_receipt_id,
            document_id: refusal.document_id.to_string(),
            revision_id: refusal.revision_id.to_string(),
            provider_profile: refusal.provider_profile.to_string(),
            embedding_provider_turn_on_observed: false,
            embedding_provider_call_allowed: false,
            vector_store_adapter_allowed: false,
            search_indexing_allowed: false,
            production_write_allowed: false,
        })
    }

    fn ensure_pages_search_source(
        &mut self,
        requested_id: &str,
        tenant_id: &str,
        actor_id: &str,
        document_id: &str,
        revision_id: &str,
    ) -> Result<String, PagesSearchPreflightError> {
        if !requested_id.trim().is_empty() {
            if self.storage.ledger().query().by_id(requested_id).is_some() {
                return Ok(requested_id.to_string());
            }
            return Err(PagesSearchPreflightError::UnknownApprovalRequestReceipt(
                requested_id.to_string(),
            ));
        }
        if let Some(receipt_id) = self
            .storage
            .ledger()
            .query()
            .by_kind("pages.approval.requested")
            .first()
            .map(|receipt| receipt.receipt_id.clone())
        {
            return Ok(receipt_id);
        }
        let draft_id = format!("draft_for_search_{document_id}");
        let draft = self
            .save_pages_edit_draft_local(PagesEditDraft {
                tenant_id,
                actor_id,
                draft_id: &draft_id,
                document_id,
                title: "Pages search preflight source",
                body_ref: "world_model://pages/search-preflight/source",
                source_publication_receipt_id: "",
                origin_receipt_id: "",
                origin_surface: "",
                revision_id,
            })
            .map_err(|_| PagesSearchPreflightError::Missing("edit_draft_receipt"))?;
        let approval = PagesApprovalRequest {
            tenant_id,
            actor_id,
            approval_request_id: "approval_for_pages_search_preflight",
            source_edit_draft_receipt_id: &draft.edit_draft_receipt_id,
            document_id,
            draft_id: &draft_id,
            requested_visibility: "tenant_review",
        };
        let report = self
            .save_pages_approval_request_local(approval)
            .map_err(|_| PagesSearchPreflightError::Missing("approval_request_receipt"))?;
        Ok(report.approval_request_receipt_id)
    }

    fn ensure_pages_search_preflight_receipt(
        &mut self,
        requested_id: &str,
    ) -> Result<String, PagesSearchEvidenceError> {
        if !requested_id.trim().is_empty() {
            return self.ensure_pages_evidence_receipt(requested_id, "pages.search.preflighted");
        }
        self.ensure_pages_evidence_receipt("", "pages.search.preflighted")
            .or_else(|_| {
                let report = self
                    .save_pages_search_preflight_local(PagesSearchPreflight {
                        tenant_id: "local_tenant",
                        actor_id: "human:local_user",
                        preflight_id: "search_preflight_for_revision_citation_comparison",
                        source_approval_request_receipt_id: "",
                        document_id: "page_local_operator_note",
                        revision_id: "rev_local_draft_001",
                        citation_handle: "pages/local-operator-note#rev-local-draft-001",
                        attachment_policy_id: "attachment-policy-v1-local",
                        requested_index_scope: "tenant operating memory",
                    })
                    .map_err(|_| PagesSearchEvidenceError::Missing("search_preflight_receipt"))?;
                Ok(report.preflight_receipt_id)
            })
    }

    fn ensure_pages_revision_citation_receipt(
        &mut self,
        requested_id: &str,
    ) -> Result<String, PagesSearchEvidenceError> {
        if !requested_id.trim().is_empty() {
            return self
                .ensure_pages_evidence_receipt(requested_id, "pages.revision_citation.compared");
        }
        self.ensure_pages_evidence_receipt("", "pages.revision_citation.compared")
            .or_else(|_| {
                let report = self.save_pages_revision_citation_comparison_local(
                    PagesRevisionCitationComparison {
                        tenant_id: "local_tenant",
                        actor_id: "human:local_user",
                        comparison_id: "revision_citation_for_visibility_check",
                        source_search_preflight_receipt_id: "",
                        document_id: "page_local_operator_note",
                        revision_id: "rev_local_draft_001",
                        citation_handle: "pages/local-operator-note#rev-local-draft-001",
                        v1_revision_key: "rev_local_draft_001",
                    },
                )?;
                Ok(report.comparison_receipt_id)
            })
    }

    fn ensure_pages_visibility_check_receipt(
        &mut self,
        requested_id: &str,
    ) -> Result<String, PagesSearchEvidenceError> {
        if !requested_id.trim().is_empty() {
            return self.ensure_pages_evidence_receipt(
                requested_id,
                "pages.publication.visibility.checked",
            );
        }
        self.ensure_pages_evidence_receipt("", "pages.publication.visibility.checked")
            .or_else(|_| {
                let report = self.save_pages_publication_visibility_check_local(
                    PagesPublicationVisibilityCheck {
                        tenant_id: "local_tenant",
                        actor_id: "human:local_user",
                        visibility_check_id: "visibility_check_for_embedding_refusal",
                        source_revision_citation_receipt_id: "",
                        document_id: "page_local_operator_note",
                        revision_id: "rev_local_draft_001",
                        requested_visibility: "tenant_review",
                    },
                )?;
                Ok(report.visibility_check_receipt_id)
            })
    }

    fn ensure_pages_evidence_receipt(
        &mut self,
        requested_id: &str,
        expected_kind: &str,
    ) -> Result<String, PagesSearchEvidenceError> {
        if !requested_id.trim().is_empty() {
            if let Some(receipt) = self.storage.ledger().query().by_id(requested_id)
                && receipt.kind == expected_kind
            {
                return Ok(requested_id.to_string());
            }
            return Err(PagesSearchEvidenceError::UnknownSourceReceipt(
                requested_id.to_string(),
            ));
        }
        self.storage
            .ledger()
            .query()
            .by_kind(expected_kind)
            .first()
            .map(|receipt| receipt.receipt_id.clone())
            .ok_or(PagesSearchEvidenceError::Missing("source_receipt"))
    }

    fn pages_search_evidence_correlation(
        &mut self,
        tenant_id: &str,
        actor_id: &str,
        loop_id: &str,
    ) -> CorrelationIds {
        CorrelationIds {
            tenant_id: TenantId::new(tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(actor_id),
            loop_id: LoopId::new(loop_id),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        }
    }

    fn start_pages_search_evidence_run(&mut self, correlation: &CorrelationIds) -> String {
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        run_id
    }

    fn finish_pages_search_preflight_run(&mut self, run_id: &str) {
        self.finish_pages_search_evidence_run(
            run_id,
            "PAGES_SEARCH_PREFLIGHT_RECORDED_INDEXING_BLOCKED",
        );
    }

    fn finish_pages_search_evidence_run(&mut self, run_id: &str, status: &str) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_search_preflight_records_indexing_blocked_receipt() {
        let mut kernel = MdxKernel::boot_local();
        let report = kernel
            .save_pages_search_preflight_local(PagesSearchPreflight {
                tenant_id: "tenant-mdx-local",
                actor_id: "human:actor-md",
                preflight_id: "pages_search_preflight_test",
                source_approval_request_receipt_id: "",
                document_id: "page-v1-operating-memory",
                revision_id: "rev-v1-001",
                citation_handle: "pages/operating-memory#rev-v1-001",
                attachment_policy_id: "attachment-policy-v1-local",
                requested_index_scope: "tenant operating memory",
            })
            .expect("pages search preflight");

        assert_eq!(
            report.status,
            "PAGES_SEARCH_PREFLIGHT_RECORDED_INDEXING_BLOCKED"
        );
        assert!(report.revision_citation_comparison_proven);
        assert!(report.attachment_access_policy_proven);
        assert!(report.search_indexing_admission_recorded);
        assert!(!report.embedding_provider_turn_on_observed);
        assert!(!report.search_indexing_allowed);
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.preflight_receipt_id)
            .expect("preflight receipt");
        assert_eq!(receipt.kind, "pages.search.preflighted");
        assert_eq!(
            receipt.payload.get("source_route").map(String::as_str),
            Some("/pages/search-preflights.json")
        );
        assert_eq!(
            receipt
                .payload
                .get("actor_admission_status")
                .map(String::as_str),
            Some("LOCAL_ACTOR_ADMITTED")
        );
    }

    #[test]
    fn pages_search_evidence_ladder_records_local_proof_with_authority_blocked() {
        let mut kernel = MdxKernel::boot_local();
        let search = kernel
            .save_pages_search_preflight_local(PagesSearchPreflight {
                tenant_id: "tenant-mdx-local",
                actor_id: "human:actor-md",
                preflight_id: "pages_search_preflight_ladder_test",
                source_approval_request_receipt_id: "",
                document_id: "page-v1-operating-memory",
                revision_id: "rev-v1-001",
                citation_handle: "pages/operating-memory#rev-v1-001",
                attachment_policy_id: "attachment-policy-v1-local",
                requested_index_scope: "tenant operating memory",
            })
            .expect("pages search preflight");
        let comparison = kernel
            .save_pages_revision_citation_comparison_local(PagesRevisionCitationComparison {
                tenant_id: "tenant-mdx-local",
                actor_id: "human:actor-md",
                comparison_id: "pages_revision_citation_comparison_test",
                source_search_preflight_receipt_id: &search.preflight_receipt_id,
                document_id: "page-v1-operating-memory",
                revision_id: "rev-v1-001",
                citation_handle: "pages/operating-memory#rev-v1-001",
                v1_revision_key: "rev-v1-001",
            })
            .expect("revision citation comparison");
        let visibility = kernel
            .save_pages_publication_visibility_check_local(PagesPublicationVisibilityCheck {
                tenant_id: "tenant-mdx-local",
                actor_id: "human:actor-md",
                visibility_check_id: "pages_publication_visibility_check_test",
                source_revision_citation_receipt_id: &comparison.comparison_receipt_id,
                document_id: "page-v1-operating-memory",
                revision_id: "rev-v1-001",
                requested_visibility: "tenant_review",
            })
            .expect("visibility check");
        let refusal = kernel
            .save_pages_embedding_provider_refusal_local(PagesEmbeddingProviderRefusal {
                tenant_id: "tenant-mdx-local",
                actor_id: "human:actor-md",
                refusal_id: "pages_embedding_provider_refusal_test",
                source_visibility_check_receipt_id: &visibility.visibility_check_receipt_id,
                document_id: "page-v1-operating-memory",
                revision_id: "rev-v1-001",
                provider_profile: "embedding-provider-local-blocked",
            })
            .expect("embedding provider refusal");

        assert!(comparison.revision_citation_comparison_proven);
        assert!(!comparison.search_indexing_allowed);
        assert!(visibility.publication_visibility_proven);
        assert!(!visibility.public_visibility_allowed);
        assert!(!refusal.embedding_provider_turn_on_observed);
        assert!(!refusal.embedding_provider_call_allowed);
        for (receipt_id, kind) in [
            (
                comparison.comparison_receipt_id,
                "pages.revision_citation.compared",
            ),
            (
                visibility.visibility_check_receipt_id,
                "pages.publication.visibility.checked",
            ),
            (
                refusal.refusal_receipt_id,
                "pages.embedding_provider.refused",
            ),
        ] {
            let receipt = kernel
                .ledger()
                .query()
                .by_id(&receipt_id)
                .expect("ladder receipt");
            assert_eq!(receipt.kind, kind);
            assert_eq!(
                receipt
                    .payload
                    .get("production_write_allowed")
                    .map(String::as_str),
                Some("false")
            );
        }
    }

    #[test]
    fn pages_search_evidence_ladder_rejects_unknown_source_receipt() {
        let mut kernel = MdxKernel::boot_local();
        let error = kernel
            .save_pages_revision_citation_comparison_local(PagesRevisionCitationComparison {
                tenant_id: "tenant-mdx-local",
                actor_id: "human:actor-md",
                comparison_id: "pages_revision_citation_comparison_test",
                source_search_preflight_receipt_id: "missing-search-preflight",
                document_id: "page-v1-operating-memory",
                revision_id: "rev-v1-001",
                citation_handle: "pages/operating-memory#rev-v1-001",
                v1_revision_key: "rev-v1-001",
            })
            .expect_err("unknown source must fail");
        assert_eq!(
            error.message(),
            "pages search evidence source receipt missing-search-preflight is unknown"
        );
    }

    #[test]
    fn pages_search_preflight_rejects_unknown_approval_receipt() {
        let mut kernel = MdxKernel::boot_local();
        let error = kernel
            .save_pages_search_preflight_local(PagesSearchPreflight {
                tenant_id: "tenant-mdx-local",
                actor_id: "human:actor-md",
                preflight_id: "pages_search_preflight_test",
                source_approval_request_receipt_id: "missing-receipt",
                document_id: "page-v1-operating-memory",
                revision_id: "rev-v1-001",
                citation_handle: "pages/operating-memory#rev-v1-001",
                attachment_policy_id: "attachment-policy-v1-local",
                requested_index_scope: "tenant operating memory",
            })
            .expect_err("unknown source must fail");
        assert_eq!(
            error.message(),
            "pages search preflight source receipt missing-receipt is unknown"
        );
    }
}
