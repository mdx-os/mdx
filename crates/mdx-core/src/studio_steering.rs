use crate::*;

// Mid-flight steering for a Studio run, implementing
// docs/STUDIO-RUNTIME-STEERING-CONTRACT.md. Slice 1 landed a document
// synchronously; Slice 2 lets a run HOLD: it opens into drafting, and the
// operator can pause it, add steering, resume it, and only then land it. Run
// state is derived from the receipt ledger - there is no separate mutable run
// store, so the receipts remain the single source of truth. Steering refusal is
// evidence, not a hard error: a refused transition records a
// studio.run.steering.refused receipt and returns a report.

/// The observable state of a steerable Studio run, derived from its receipts.
/// The contract names finer intermediate states; locally, pause and resume
/// requests are accepted in the same step, so the stable observable states are
/// drafting, paused, and the terminal landed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StudioRunState {
    Unknown,
    Drafting,
    Paused,
    Landed,
}

impl StudioRunState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Drafting => "drafting",
            Self::Paused => "paused",
            Self::Landed => "landed",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Landed)
    }
}

/// Open a steerable run. Records the run and its kind, then holds in drafting.
/// Title and body are not set here: the draft evolves through steering and is
/// finalized at land time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudioOpenRun<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub run_id: &'a str,
    pub goal: &'a str,
    pub artifact_kind: &'a str,
    pub source_receipt_id: &'a str,
    /// One of STUDIO_SOURCE_KINDS. Names what the source_receipt_id grounds the
    /// run in: the default local `evals`, an internal `pages_object`, or an
    /// external `connector_item`. Empty defaults to `evals`.
    pub source_kind: &'a str,
    /// A human label for the bound source (the Page title or connector item
    /// title), recorded for provenance so the record reads "Grounded in X".
    pub source_title: &'a str,
}

/// A pause or resume control on a run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudioControl<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub run_id: &'a str,
    pub operator_id: &'a str,
    pub idempotency_key: &'a str,
}

/// A steering intervention recorded against a paused run. The payload may carry
/// operator intent and source references; it may not carry a replacement
/// document body, bypass Pages publication, change the kind, request code work,
/// or open a co-edit channel - those are structurally absent from this struct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudioSteering<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub run_id: &'a str,
    pub operator_id: &'a str,
    pub idempotency_key: &'a str,
    pub instruction: &'a str,
    pub target_scope: &'a str,
    pub reason: &'a str,
    pub source_refs: &'a str,
}

/// Finalize a held run: draft the artifact and land it as one Pages object,
/// reusing the Slice 1 one-body-of-truth landing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudioLanding<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub run_id: &'a str,
    pub operator_id: &'a str,
    pub idempotency_key: &'a str,
    pub document_id: &'a str,
    pub title: &'a str,
    pub body_ref: &'a str,
    pub revision_id: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudioOpenReport {
    pub status: &'static str,
    pub run_id: String,
    pub state: &'static str,
    pub artifact_kind: &'static str,
    pub source_kind: &'static str,
    pub opened_receipt_id: String,
    pub kind_selected_receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudioControlReport {
    pub status: &'static str,
    pub run_id: String,
    pub state: &'static str,
    pub receipt_id: String,
    pub idempotency_key: String,
    pub idempotent_replay: bool,
    pub refused: bool,
    pub refusal_reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudioLandReport {
    pub status: &'static str,
    pub run_id: String,
    pub state: &'static str,
    pub document_id: String,
    pub artifact_drafted_receipt_id: String,
    pub publication_receipt_id: String,
    pub run_landed_receipt_id: String,
    pub idempotency_key: String,
    pub idempotent_replay: bool,
    pub refused: bool,
    pub refusal_reason: String,
}

impl<S: StorageProvider> MdxKernel<S> {
    /// The current state of a run, derived by walking its receipts in order.
    pub fn studio_run_state(&self, run_id: &str) -> StudioRunState {
        let mut state = StudioRunState::Unknown;
        for receipt in self.storage.ledger().entries() {
            if receipt.payload.get("studio_run_id").map(String::as_str) != Some(run_id) {
                continue;
            }
            match receipt.kind.as_str() {
                "studio.run.opened" => state = StudioRunState::Drafting,
                "studio.run.pause.accepted" => state = StudioRunState::Paused,
                "studio.run.resume.accepted" => state = StudioRunState::Drafting,
                "studio.run.landed" => state = StudioRunState::Landed,
                _ => {}
            }
        }
        state
    }

    /// The existing control receipt for an idempotency key on a run, if any.
    fn studio_control_for_key(&self, run_id: &str, idempotency_key: &str) -> Option<Receipt> {
        self.storage
            .ledger()
            .entries()
            .iter()
            .find(|receipt| {
                receipt.payload.get("studio_run_id").map(String::as_str) == Some(run_id)
                    && receipt.payload.get("idempotency_key").map(String::as_str)
                        == Some(idempotency_key)
            })
            .cloned()
    }

    /// Record one governed Studio control receipt: correlation, loop run, policy
    /// decision, and the transition receipt, then finish the run.
    pub(crate) fn emit_studio_control_receipt(
        &mut self,
        tenant_id: &str,
        actor_id: &str,
        transition: &str,
        kind: &str,
        payload: std::collections::BTreeMap<String, String>,
    ) -> Receipt {
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(actor_id),
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
        let receipt =
            self.transition_receipt(&run_id, transition, &correlation, &decision, kind, payload);
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "STUDIO_CONTROL_RECORDED".to_string();
        }
        receipt
    }

    pub(crate) fn studio_admit(
        &self,
        tenant_id: &str,
        actor_id: &str,
        run_id: &str,
    ) -> Result<(), StudioRunError> {
        admit_local_route_actor(
            tenant_id,
            actor_id,
            "operator",
            "/studio/runs.json",
            "studio.run.steering",
            run_id,
        )
        .map(|_| ())
        .map_err(|error| StudioRunError::ActorAdmission(error.message()))
    }

    fn studio_refusal(
        &mut self,
        tenant_id: &str,
        actor_id: &str,
        run_id: &str,
        operator_id: &str,
        idempotency_key: &str,
        reason: &str,
    ) -> StudioControlReport {
        let receipt = self.emit_studio_control_receipt(
            tenant_id,
            actor_id,
            "STUDIO_STEERING_REFUSED",
            "studio.run.steering.refused",
            payload(&[
                ("studio_run_id", run_id),
                ("operator_id", operator_id),
                ("idempotency_key", idempotency_key),
                ("refusal_reason", reason),
            ]),
        );
        let state = self.studio_run_state(run_id);
        StudioControlReport {
            status: "STUDIO_STEERING_REFUSED",
            run_id: run_id.to_string(),
            state: state.as_str(),
            receipt_id: receipt.receipt_id,
            idempotency_key: idempotency_key.to_string(),
            idempotent_replay: false,
            refused: true,
            refusal_reason: reason.to_string(),
        }
    }

    /// A field from the run's opening receipt (source receipt, kind, goal).
    fn studio_opened_field(&self, run_id: &str, field: &str) -> Option<String> {
        self.storage
            .ledger()
            .entries()
            .iter()
            .find(|receipt| {
                receipt.kind == "studio.run.opened"
                    && receipt.payload.get("studio_run_id").map(String::as_str) == Some(run_id)
            })
            .and_then(|receipt| receipt.payload.get(field).cloned())
    }

    /// The run's receipt of a given kind, if it exists (used for idempotent
    /// replay of a landing without a second publication).
    fn studio_receipt_of_kind(&self, run_id: &str, kind: &str) -> Option<Receipt> {
        self.storage
            .ledger()
            .entries()
            .iter()
            .find(|receipt| {
                receipt.kind == kind
                    && receipt.payload.get("studio_run_id").map(String::as_str) == Some(run_id)
            })
            .cloned()
    }

    /// Open a steerable run: record it and its kind, then hold in drafting.
    pub fn open_studio_run(
        &mut self,
        request: StudioOpenRun<'_>,
    ) -> Result<StudioOpenReport, StudioRunError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("run_id", request.run_id),
            ("goal", request.goal),
            ("source_receipt_id", request.source_receipt_id),
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
        let source_kind = normalize_studio_source_kind(request.source_kind)?;
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
        if self.studio_run_state(request.run_id) != StudioRunState::Unknown {
            return Err(StudioRunError::RunAlreadyOpen(request.run_id.to_string()));
        }
        self.studio_admit(request.tenant_id, request.actor_id, request.run_id)?;
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        let opened = self.emit_studio_control_receipt(
            request.tenant_id,
            request.actor_id,
            "STUDIO_RUN_OPENED",
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
                ("source_kind", source_kind),
                ("source_title", request.source_title),
            ]),
        );
        let kind_selected = self.emit_studio_control_receipt(
            request.tenant_id,
            request.actor_id,
            "STUDIO_KIND_SELECTED",
            "studio.kind.selected",
            payload(&[
                ("studio_run_id", request.run_id),
                ("artifact_kind", artifact_kind),
                ("kind_selection_mode", "propose_and_confirm"),
                ("auto_kind_select_allowed", "false"),
            ]),
        );
        Ok(StudioOpenReport {
            status: "STUDIO_RUN_DRAFTING",
            run_id: request.run_id.to_string(),
            state: StudioRunState::Drafting.as_str(),
            artifact_kind,
            source_kind,
            opened_receipt_id: opened.receipt_id,
            kind_selected_receipt_id: kind_selected.receipt_id,
        })
    }

    /// Pause a drafting run. Refused after a terminal state or if already paused.
    pub fn pause_studio_run(
        &mut self,
        control: StudioControl<'_>,
    ) -> Result<StudioControlReport, StudioRunError> {
        self.studio_admit(control.tenant_id, control.actor_id, control.run_id)?;
        if let Some(existing) = self.studio_control_for_key(control.run_id, control.idempotency_key)
        {
            return Ok(self.studio_replay_or_refuse(
                &control,
                &existing,
                "studio.run.pause.accepted",
                "STUDIO_RUN_PAUSED",
            ));
        }
        match self.studio_run_state(control.run_id) {
            StudioRunState::Unknown => Ok(self.studio_refusal(
                control.tenant_id,
                control.actor_id,
                control.run_id,
                control.operator_id,
                control.idempotency_key,
                "run not found",
            )),
            StudioRunState::Landed => Ok(self.studio_refusal(
                control.tenant_id,
                control.actor_id,
                control.run_id,
                control.operator_id,
                control.idempotency_key,
                "run already landed",
            )),
            StudioRunState::Paused => Ok(self.studio_refusal(
                control.tenant_id,
                control.actor_id,
                control.run_id,
                control.operator_id,
                control.idempotency_key,
                "run is already paused",
            )),
            StudioRunState::Drafting => {
                // The idempotency key is carried only on the consequential
                // accepted receipt, so a replay lookup resolves to it, not to
                // this intermediate request.
                self.emit_studio_control_receipt(
                    control.tenant_id,
                    control.actor_id,
                    "STUDIO_PAUSE_REQUESTED",
                    "studio.run.pause.requested",
                    payload(&[
                        ("studio_run_id", control.run_id),
                        ("operator_id", control.operator_id),
                    ]),
                );
                let accepted = self.emit_studio_control_receipt(
                    control.tenant_id,
                    control.actor_id,
                    "STUDIO_PAUSE_ACCEPTED",
                    "studio.run.pause.accepted",
                    payload(&[
                        ("studio_run_id", control.run_id),
                        ("operator_id", control.operator_id),
                        ("idempotency_key", control.idempotency_key),
                    ]),
                );
                Ok(StudioControlReport {
                    status: "STUDIO_RUN_PAUSED",
                    run_id: control.run_id.to_string(),
                    state: StudioRunState::Paused.as_str(),
                    receipt_id: accepted.receipt_id,
                    idempotency_key: control.idempotency_key.to_string(),
                    idempotent_replay: false,
                    refused: false,
                    refusal_reason: String::new(),
                })
            }
        }
    }

    /// Add a steering intervention to a paused run.
    pub fn steer_studio_run(
        &mut self,
        steering: StudioSteering<'_>,
    ) -> Result<StudioControlReport, StudioRunError> {
        for (field, value) in [
            ("idempotency_key", steering.idempotency_key),
            ("instruction", steering.instruction),
        ] {
            if value.trim().is_empty() {
                return Err(StudioRunError::Missing(field));
            }
        }
        self.studio_admit(steering.tenant_id, steering.actor_id, steering.run_id)?;
        if let Some(existing) =
            self.studio_control_for_key(steering.run_id, steering.idempotency_key)
        {
            let same_payload = existing.kind == "studio.run.steering.added"
                && existing.payload.get("instruction").map(String::as_str)
                    == Some(steering.instruction)
                && existing.payload.get("target_scope").map(String::as_str)
                    == Some(steering.target_scope)
                && existing.payload.get("reason").map(String::as_str) == Some(steering.reason);
            if same_payload {
                return Ok(StudioControlReport {
                    status: "STUDIO_STEERING_RECORDED",
                    run_id: steering.run_id.to_string(),
                    state: self.studio_run_state(steering.run_id).as_str(),
                    receipt_id: existing.receipt_id,
                    idempotency_key: steering.idempotency_key.to_string(),
                    idempotent_replay: true,
                    refused: false,
                    refusal_reason: String::new(),
                });
            }
            return Ok(self.studio_refusal(
                steering.tenant_id,
                steering.actor_id,
                steering.run_id,
                steering.operator_id,
                steering.idempotency_key,
                "idempotency key reused with a different payload",
            ));
        }
        if self.studio_run_state(steering.run_id) != StudioRunState::Paused {
            return Ok(self.studio_refusal(
                steering.tenant_id,
                steering.actor_id,
                steering.run_id,
                steering.operator_id,
                steering.idempotency_key,
                "steering is recorded only on a paused run",
            ));
        }
        let added = self.emit_studio_control_receipt(
            steering.tenant_id,
            steering.actor_id,
            "STUDIO_STEERING_ADDED",
            "studio.run.steering.added",
            payload(&[
                ("studio_run_id", steering.run_id),
                ("operator_id", steering.operator_id),
                ("idempotency_key", steering.idempotency_key),
                ("instruction", steering.instruction),
                ("target_scope", steering.target_scope),
                ("reason", steering.reason),
                ("source_refs", steering.source_refs),
            ]),
        );
        Ok(StudioControlReport {
            status: "STUDIO_STEERING_RECORDED",
            run_id: steering.run_id.to_string(),
            state: StudioRunState::Paused.as_str(),
            receipt_id: added.receipt_id,
            idempotency_key: steering.idempotency_key.to_string(),
            idempotent_replay: false,
            refused: false,
            refusal_reason: String::new(),
        })
    }

    /// Resume a paused run back to drafting.
    pub fn resume_studio_run(
        &mut self,
        control: StudioControl<'_>,
    ) -> Result<StudioControlReport, StudioRunError> {
        self.studio_admit(control.tenant_id, control.actor_id, control.run_id)?;
        if let Some(existing) = self.studio_control_for_key(control.run_id, control.idempotency_key)
        {
            return Ok(self.studio_replay_or_refuse(
                &control,
                &existing,
                "studio.run.resume.accepted",
                "STUDIO_RUN_RESUMED",
            ));
        }
        if self.studio_run_state(control.run_id) != StudioRunState::Paused {
            return Ok(self.studio_refusal(
                control.tenant_id,
                control.actor_id,
                control.run_id,
                control.operator_id,
                control.idempotency_key,
                "resume applies only to a paused run",
            ));
        }
        // The key is carried only on the accepted receipt (see pause).
        self.emit_studio_control_receipt(
            control.tenant_id,
            control.actor_id,
            "STUDIO_RESUME_REQUESTED",
            "studio.run.resume.requested",
            payload(&[
                ("studio_run_id", control.run_id),
                ("operator_id", control.operator_id),
            ]),
        );
        let accepted = self.emit_studio_control_receipt(
            control.tenant_id,
            control.actor_id,
            "STUDIO_RESUME_ACCEPTED",
            "studio.run.resume.accepted",
            payload(&[
                ("studio_run_id", control.run_id),
                ("operator_id", control.operator_id),
                ("idempotency_key", control.idempotency_key),
            ]),
        );
        Ok(StudioControlReport {
            status: "STUDIO_RUN_RESUMED",
            run_id: control.run_id.to_string(),
            state: StudioRunState::Drafting.as_str(),
            receipt_id: accepted.receipt_id,
            idempotency_key: control.idempotency_key.to_string(),
            idempotent_replay: false,
            refused: false,
            refusal_reason: String::new(),
        })
    }

    /// Replay an existing control receipt for a key, or refuse if the key was
    /// reused for a different operation.
    fn studio_replay_or_refuse(
        &mut self,
        control: &StudioControl<'_>,
        existing: &Receipt,
        expected_kind: &str,
        replay_status: &'static str,
    ) -> StudioControlReport {
        if existing.kind == expected_kind {
            return StudioControlReport {
                status: replay_status,
                run_id: control.run_id.to_string(),
                state: self.studio_run_state(control.run_id).as_str(),
                receipt_id: existing.receipt_id.clone(),
                idempotency_key: control.idempotency_key.to_string(),
                idempotent_replay: true,
                refused: false,
                refusal_reason: String::new(),
            };
        }
        self.studio_refusal(
            control.tenant_id,
            control.actor_id,
            control.run_id,
            control.operator_id,
            control.idempotency_key,
            "idempotency key reused with a different payload",
        )
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    /// Finalize a held run: draft and land the document as one Pages object,
    /// reusing the Slice 1 one-body-of-truth landing. Refused while paused or
    /// after a terminal state. A replay of an already-landed run returns the
    /// existing landing and never creates a second pages.document.published.
    pub fn land_studio_run(
        &mut self,
        landing: StudioLanding<'_>,
    ) -> Result<StudioLandReport, StudioRunError> {
        for (field, value) in [
            ("run_id", landing.run_id),
            ("idempotency_key", landing.idempotency_key),
            ("document_id", landing.document_id),
            ("title", landing.title),
            ("body_ref", landing.body_ref),
            ("revision_id", landing.revision_id),
        ] {
            if value.trim().is_empty() {
                return Err(StudioRunError::Missing(field));
            }
        }
        let actor_admission = admit_local_route_actor(
            landing.tenant_id,
            landing.actor_id,
            "operator",
            "/studio/runs.json",
            "studio.run.landed",
            landing.run_id,
        )
        .map_err(|error| StudioRunError::ActorAdmission(error.message()))?;

        // Idempotent landing: an already-landed run returns its existing landing,
        // never a second publication.
        if self.studio_run_state(landing.run_id) == StudioRunState::Landed {
            let publication = self
                .studio_receipt_of_kind(landing.run_id, "pages.document.published")
                .map(|receipt| receipt.receipt_id)
                .unwrap_or_default();
            let landed = self
                .studio_receipt_of_kind(landing.run_id, "studio.run.landed")
                .map(|receipt| receipt.receipt_id)
                .unwrap_or_default();
            let drafted = self
                .studio_receipt_of_kind(landing.run_id, "studio.artifact.drafted")
                .map(|receipt| receipt.receipt_id)
                .unwrap_or_default();
            return Ok(StudioLandReport {
                status: "STUDIO_DOCUMENT_RUN_LANDED",
                run_id: landing.run_id.to_string(),
                state: StudioRunState::Landed.as_str(),
                document_id: landing.document_id.to_string(),
                artifact_drafted_receipt_id: drafted,
                publication_receipt_id: publication,
                run_landed_receipt_id: landed,
                idempotency_key: landing.idempotency_key.to_string(),
                idempotent_replay: true,
                refused: false,
                refusal_reason: String::new(),
            });
        }

        let reason = match self.studio_run_state(landing.run_id) {
            StudioRunState::Unknown => Some("run not found"),
            StudioRunState::Paused => Some("resume the run before landing it"),
            _ => None,
        };
        if let Some(reason) = reason {
            let refusal = self.emit_studio_control_receipt(
                landing.tenant_id,
                landing.actor_id,
                "STUDIO_STEERING_REFUSED",
                "studio.run.steering.refused",
                payload(&[
                    ("studio_run_id", landing.run_id),
                    ("operator_id", landing.operator_id),
                    ("idempotency_key", landing.idempotency_key),
                    ("refusal_reason", reason),
                ]),
            );
            return Ok(StudioLandReport {
                status: "STUDIO_STEERING_REFUSED",
                run_id: landing.run_id.to_string(),
                state: self.studio_run_state(landing.run_id).as_str(),
                document_id: landing.document_id.to_string(),
                artifact_drafted_receipt_id: String::new(),
                publication_receipt_id: String::new(),
                run_landed_receipt_id: refusal.receipt_id,
                idempotency_key: landing.idempotency_key.to_string(),
                idempotent_replay: false,
                refused: true,
                refusal_reason: reason.to_string(),
            });
        }

        let identity = GovernedWriteIdentity::local_demo(landing.actor_id);
        let source_receipt_id = self
            .studio_opened_field(landing.run_id, "source_receipt_id")
            .unwrap_or_default();
        // The kind is whatever the run opened as, not a hardcoded document. It
        // lands as one typed Pages object; page_type stays within the Pages
        // vocabulary while studio_artifact_kind carries the authoritative kind.
        let artifact_kind = self
            .studio_opened_field(landing.run_id, "artifact_kind")
            .filter(|kind| !kind.is_empty())
            .unwrap_or_else(|| "document".to_string());
        let page_type = studio_kind_page_type(&artifact_kind);
        let drafted = self.emit_studio_control_receipt(
            landing.tenant_id,
            landing.actor_id,
            "STUDIO_ARTIFACT_DRAFTED",
            "studio.artifact.drafted",
            payload(&[
                ("studio_run_id", landing.run_id),
                ("artifact_kind", &artifact_kind),
                ("document_id", landing.document_id),
                ("title", landing.title),
                ("body_ref", landing.body_ref),
                ("revision_id", landing.revision_id),
                ("standalone_document_store_allowed", "false"),
            ]),
        );
        // Landing reuses the Pages publication receipt kind: one body of truth in
        // the existing Pages projection, with studio provenance added.
        let publication = self.emit_studio_control_receipt(
            landing.tenant_id,
            landing.actor_id,
            "PUBLISH_PAGES_PROJECTION",
            "pages.document.published",
            payload(&[
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("document_id", landing.document_id),
                ("title", landing.title),
                ("page_type", page_type),
                ("studio_artifact_kind", &artifact_kind),
                ("body_ref", landing.body_ref),
                ("source_receipt_id", &source_receipt_id),
                ("revision_id", landing.revision_id),
                ("visibility", "tenant_only"),
                ("origin", "studio"),
                ("studio_run_id", landing.run_id),
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
        let landed = self.emit_studio_control_receipt(
            landing.tenant_id,
            landing.actor_id,
            "STUDIO_RUN_LANDED",
            "studio.run.landed",
            payload(&[
                ("studio_run_id", landing.run_id),
                ("artifact_kind", &artifact_kind),
                ("document_id", landing.document_id),
                ("publication_receipt_id", &publication.receipt_id),
                ("body_ref", landing.body_ref),
                ("idempotency_key", landing.idempotency_key),
                ("terminal_state", "STUDIO_DOCUMENT_RUN_LANDED"),
                ("live_co_edit_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(StudioLandReport {
            status: "STUDIO_DOCUMENT_RUN_LANDED",
            run_id: landing.run_id.to_string(),
            state: StudioRunState::Landed.as_str(),
            document_id: landing.document_id.to_string(),
            artifact_drafted_receipt_id: drafted.receipt_id,
            publication_receipt_id: publication.receipt_id,
            run_landed_receipt_id: landed.receipt_id,
            idempotency_key: landing.idempotency_key.to_string(),
            idempotent_replay: false,
            refused: false,
            refusal_reason: String::new(),
        })
    }
}
