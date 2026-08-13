use crate::*;

/// The closed vocabulary of Studio artifact kinds, mirroring the declared
/// work-kind contract (generated/world-model/studio-work-kind-contract.json).
/// The kind is first-class on the receipt so the run records what it produced.
/// Only `document` lands a governed artifact in slice 1; the rest are declared
/// so the taxonomy is fixed, but refused as not-yet-built rather than silently
/// accepted.
pub const STUDIO_ARTIFACT_KINDS: [&str; 6] =
    ["document", "code", "sheet", "deck", "dataset", "decision"];

/// The kinds that land a governed artifact in Studio. Slice 1 shipped
/// `document`; Slice 4 generalizes the registry so a sheet, deck, dataset, or
/// decision lands the same way - as one typed Pages object, provably. `code` is
/// deliberately absent: code work opens in Forge, not Studio.
pub const STUDIO_IMPLEMENTED_KINDS: [&str; 5] =
    ["document", "sheet", "deck", "dataset", "decision"];

/// Map a Studio artifact kind to a sanctioned Pages page_type. The kind is the
/// authoritative shape (recorded as `studio_artifact_kind` on the landed
/// publication); page_type is the Pages-side classification, kept within the
/// existing PAGE_TYPES vocabulary so Studio never widens the Pages contract.
/// A decision lands as a decision page; the rest land as knowledge, their real
/// kind carried alongside.
pub fn studio_kind_page_type(artifact_kind: &str) -> &'static str {
    match artifact_kind {
        "decision" => "decision",
        _ => "knowledge",
    }
}

/// Normalize a requested artifact kind to one of the sanctioned kinds. Empty
/// means the run did not declare one, which defaults to `document`. A non-empty
/// value outside the vocabulary is refused rather than coerced, so a typo never
/// lands as the wrong shape.
pub fn normalize_studio_artifact_kind(raw: &str) -> Result<&'static str, StudioRunError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok("document");
    }
    let lowered = trimmed.to_ascii_lowercase();
    STUDIO_ARTIFACT_KINDS
        .iter()
        .copied()
        .find(|kind| *kind == lowered)
        .ok_or_else(|| StudioRunError::InvalidKind(trimmed.to_string()))
}

/// The closed vocabulary of source kinds a run can be grounded in, mirroring the
/// `source_bindings` registry in the work-kind contract. `evals` is the default
/// local grounding (the run's first ledger receipt) used when the operator binds
/// nothing; `pages_object` grounds a run in an internal Page; `connector_item`
/// grounds it in an external connector item carrying its own sensitivity and
/// handling. The kind is recorded on the opened receipt so the run's provenance
/// names where its inputs came from, not just that a source receipt existed.
pub const STUDIO_SOURCE_KINDS: [&str; 3] = ["evals", "pages_object", "connector_item"];

/// Normalize a requested source kind. Empty defaults to `evals` (the local
/// grounding the route picks when nothing is bound). A non-empty value outside
/// the vocabulary is refused rather than coerced, so the recorded provenance is
/// always one of the sanctioned origins.
pub fn normalize_studio_source_kind(raw: &str) -> Result<&'static str, StudioRunError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok("evals");
    }
    let lowered = trimmed.to_ascii_lowercase();
    STUDIO_SOURCE_KINDS
        .iter()
        .copied()
        .find(|kind| *kind == lowered)
        .ok_or_else(|| StudioRunError::InvalidSourceKind(trimmed.to_string()))
}

/// A Studio run that drafts a document artifact against grounded evidence and
/// lands it as a governed Pages object. The body is govern-by-reference: the
/// run carries a `body_ref`, never a copy of the document body, so Studio holds
/// the work session and provenance while Pages holds the single body of truth.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudioDocumentRun<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub run_id: &'a str,
    pub goal: &'a str,
    /// The artifact kind, one of STUDIO_ARTIFACT_KINDS. Empty defaults to
    /// `document`. Slice 1 lands only `document`.
    pub artifact_kind: &'a str,
    pub document_id: &'a str,
    pub title: &'a str,
    pub body_ref: &'a str,
    pub source_receipt_id: &'a str,
    pub revision_id: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudioDocumentRunReport {
    pub status: &'static str,
    pub run_id: String,
    pub goal: String,
    pub artifact_kind: &'static str,
    pub document_id: String,
    pub title: String,
    pub policy_decision_id: String,
    pub run_opened_receipt_id: String,
    pub kind_selected_receipt_id: String,
    pub artifact_drafted_receipt_id: String,
    /// The reused `pages.document.published` receipt. This is the single body of
    /// truth: the landed document appears in the Pages projection, and Studio
    /// holds only this reference plus the run and provenance receipts.
    pub publication_receipt_id: String,
    pub run_landed_receipt_id: String,
    pub source_receipt_id: String,
    pub revision_id: String,
    pub kind_selection_mode: &'static str,
    pub standalone_document_store_allowed: bool,
    pub live_co_edit_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StudioRunError {
    Missing(&'static str),
    UnknownSourceReceipt(String),
    ActorAdmission(String),
    InvalidKind(String),
    InvalidSourceKind(String),
    KindNotImplemented(String),
    RunAlreadyOpen(String),
    UnknownRun(String),
}

impl StudioRunError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("studio document run missing {field}"),
            Self::UnknownSourceReceipt(id) => {
                format!("studio document run source receipt {id} is unknown")
            }
            Self::ActorAdmission(message) => message.clone(),
            Self::InvalidKind(kind) => format!(
                "studio artifact kind {kind} is not one of {}",
                STUDIO_ARTIFACT_KINDS.join(", ")
            ),
            Self::InvalidSourceKind(kind) => format!(
                "studio source kind {kind} is not one of {}",
                STUDIO_SOURCE_KINDS.join(", ")
            ),
            Self::KindNotImplemented(kind) => format!(
                "studio artifact kind {kind} is declared but not yet landed; slice 1 lands {}",
                STUDIO_IMPLEMENTED_KINDS.join(", ")
            ),
            Self::RunAlreadyOpen(run_id) => {
                format!("studio run {run_id} is already open")
            }
            Self::UnknownRun(run_id) => {
                format!("studio run {run_id} is not open")
            }
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn run_studio_document_local(
        &mut self,
        request: StudioDocumentRun<'_>,
    ) -> Result<StudioDocumentRunReport, StudioRunError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.run_studio_document_local_with_identity(request, &identity)
    }

    pub fn run_studio_document_local_with_identity(
        &mut self,
        request: StudioDocumentRun<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<StudioDocumentRunReport, StudioRunError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("run_id", request.run_id),
            ("goal", request.goal),
            ("document_id", request.document_id),
            ("title", request.title),
            ("body_ref", request.body_ref),
            ("source_receipt_id", request.source_receipt_id),
            ("revision_id", request.revision_id),
        ] {
            if value.trim().is_empty() {
                return Err(StudioRunError::Missing(field));
            }
        }
        let artifact_kind = normalize_studio_artifact_kind(request.artifact_kind)?;
        if !STUDIO_IMPLEMENTED_KINDS.contains(&artifact_kind) {
            return Err(StudioRunError::KindNotImplemented(
                artifact_kind.to_string(),
            ));
        }
        if self
            .storage
            .ledger()
            .query()
            .by_id(request.source_receipt_id)
            .is_none()
        {
            return Err(StudioRunError::UnknownSourceReceipt(
                request.source_receipt_id.to_string(),
            ));
        }
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "operator",
            "/studio/runs.json",
            "studio.run.landed",
            request.run_id,
        )
        .map_err(|error| StudioRunError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("studio_document_run"),
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
        let decision = self.decide_with_receipt(&correlation, ActionKind::RecordStudioDocumentRun);

        // 1. The run opens against a goal and grounded evidence.
        let run_opened = self.transition_receipt(
            &run_id,
            "STUDIO_RUN_OPENED",
            &correlation,
            &decision,
            "studio.run.opened",
            payload(&[
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("studio_run_id", request.run_id),
                ("goal", request.goal),
                ("artifact_kind", artifact_kind),
                ("source_receipt_id", request.source_receipt_id),
            ]),
        );

        // 2. The coworker proposes a kind and the human confirms it. Slice 1 is
        //    always propose-and-confirm; auto-pick is not allowed yet.
        let kind_selected = self.transition_receipt(
            &run_id,
            "STUDIO_KIND_SELECTED",
            &correlation,
            &decision,
            "studio.kind.selected",
            payload(&[
                ("studio_run_id", request.run_id),
                ("artifact_kind", artifact_kind),
                ("kind_selection_mode", "propose_and_confirm"),
                ("auto_kind_select_allowed", "false"),
            ]),
        );

        // 3. The artifact is drafted as a reference, not a stored body copy.
        let artifact_drafted = self.transition_receipt(
            &run_id,
            "STUDIO_ARTIFACT_DRAFTED",
            &correlation,
            &decision,
            "studio.artifact.drafted",
            payload(&[
                ("studio_run_id", request.run_id),
                ("artifact_kind", artifact_kind),
                ("document_id", request.document_id),
                ("title", request.title),
                ("body_ref", request.body_ref),
                ("revision_id", request.revision_id),
                ("standalone_document_store_allowed", "false"),
            ]),
        );

        // 4. Landing reuses the Pages publication receipt kind so the document
        //    becomes a single Pages object - one body of truth - and shows in the
        //    Pages projection. Studio adds provenance (origin, studio_run_id),
        //    it does not fork a second document store.
        let publication = self.transition_receipt(
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
                ("page_type", studio_kind_page_type(artifact_kind)),
                ("studio_artifact_kind", artifact_kind),
                ("body_ref", request.body_ref),
                ("source_receipt_id", request.source_receipt_id),
                ("revision_id", request.revision_id),
                ("visibility", "tenant_only"),
                ("origin", "studio"),
                ("studio_run_id", request.run_id),
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

        // 5. The run lands, referencing the body-of-truth publication receipt.
        let run_landed = self.transition_receipt(
            &run_id,
            "STUDIO_RUN_LANDED",
            &correlation,
            &decision,
            "studio.run.landed",
            payload(&[
                ("studio_run_id", request.run_id),
                ("artifact_kind", artifact_kind),
                ("document_id", request.document_id),
                ("publication_receipt_id", &publication.receipt_id),
                ("body_ref", request.body_ref),
                ("terminal_state", "STUDIO_DOCUMENT_RUN_LANDED"),
                ("live_co_edit_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );

        self.finish_studio_document_run(&run_id);
        Ok(StudioDocumentRunReport {
            status: "STUDIO_DOCUMENT_RUN_LANDED",
            run_id: request.run_id.to_string(),
            goal: request.goal.to_string(),
            artifact_kind,
            document_id: request.document_id.to_string(),
            title: request.title.to_string(),
            policy_decision_id: decision.policy_decision_id,
            run_opened_receipt_id: run_opened.receipt_id,
            kind_selected_receipt_id: kind_selected.receipt_id,
            artifact_drafted_receipt_id: artifact_drafted.receipt_id,
            publication_receipt_id: publication.receipt_id,
            run_landed_receipt_id: run_landed.receipt_id,
            source_receipt_id: request.source_receipt_id.to_string(),
            revision_id: request.revision_id.to_string(),
            kind_selection_mode: "propose_and_confirm",
            standalone_document_store_allowed: false,
            live_co_edit_allowed: false,
            production_write_allowed: false,
        })
    }

    fn finish_studio_document_run(&mut self, run_id: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "STUDIO_DOCUMENT_RUN_LANDED".to_string();
        }
    }
}
