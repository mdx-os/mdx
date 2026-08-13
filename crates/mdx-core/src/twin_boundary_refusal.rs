use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinBoundaryRefusalRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub session_id: &'a str,
    pub source_draft_receipt_id: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinBoundaryRefusalReport {
    pub status: &'static str,
    pub session_id: String,
    pub source_draft_receipt_id: String,
    pub refusal_receipt_id: String,
    pub refused_capability: &'static str,
    pub terminal_state: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TwinBoundaryRefusalError {
    Missing(&'static str),
    UnknownSourceReceipt(String),
    ActorAdmission(String),
}

impl TwinBoundaryRefusalError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("twin boundary refusal missing {field}"),
            Self::UnknownSourceReceipt(id) => {
                format!("twin boundary refusal source receipt {id} is unknown")
            }
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_twin_provider_call_refusal_local(
        &mut self,
        request: TwinBoundaryRefusalRequest<'_>,
    ) -> Result<TwinBoundaryRefusalReport, TwinBoundaryRefusalError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_twin_provider_call_refusal_local_with_identity(request, &identity)
    }

    pub fn save_twin_provider_call_refusal_local_with_identity(
        &mut self,
        request: TwinBoundaryRefusalRequest<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<TwinBoundaryRefusalReport, TwinBoundaryRefusalError> {
        self.save_twin_boundary_refusal(request, identity, TwinBoundaryRefusalKind::ProviderCall)
    }

    pub fn save_twin_production_memory_refusal_local(
        &mut self,
        request: TwinBoundaryRefusalRequest<'_>,
    ) -> Result<TwinBoundaryRefusalReport, TwinBoundaryRefusalError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_twin_production_memory_refusal_local_with_identity(request, &identity)
    }

    pub fn save_twin_production_memory_refusal_local_with_identity(
        &mut self,
        request: TwinBoundaryRefusalRequest<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<TwinBoundaryRefusalReport, TwinBoundaryRefusalError> {
        self.save_twin_boundary_refusal(
            request,
            identity,
            TwinBoundaryRefusalKind::ProductionMemoryWrite,
        )
    }

    pub fn save_twin_tool_execution_refusal_local(
        &mut self,
        request: TwinBoundaryRefusalRequest<'_>,
    ) -> Result<TwinBoundaryRefusalReport, TwinBoundaryRefusalError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_twin_tool_execution_refusal_local_with_identity(request, &identity)
    }

    pub fn save_twin_tool_execution_refusal_local_with_identity(
        &mut self,
        request: TwinBoundaryRefusalRequest<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<TwinBoundaryRefusalReport, TwinBoundaryRefusalError> {
        self.save_twin_boundary_refusal(request, identity, TwinBoundaryRefusalKind::ToolExecution)
    }

    fn save_twin_boundary_refusal(
        &mut self,
        request: TwinBoundaryRefusalRequest<'_>,
        identity: &GovernedWriteIdentity,
        kind: TwinBoundaryRefusalKind,
    ) -> Result<TwinBoundaryRefusalReport, TwinBoundaryRefusalError> {
        let action = kind.action();
        let source_route = kind.source_route();
        let receipt_kind = kind.receipt_kind();
        let status = kind.status();
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("session_id", request.session_id),
        ] {
            if value.trim().is_empty() {
                return Err(TwinBoundaryRefusalError::Missing(field));
            }
        }
        let source_draft_receipt_id = self.ensure_twin_draft_receipt(&request)?;
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "operator",
            source_route,
            receipt_kind,
            request.session_id,
        )
        .map_err(|error| TwinBoundaryRefusalError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("twin_boundary_refusal"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.start_twin_boundary_refusal_run(&correlation);
        let decision = self.decide_with_receipt(&correlation, action);
        let receipt = self.transition_receipt(
            &run_id,
            kind.transition(),
            &correlation,
            &decision,
            receipt_kind,
            payload(&[
                ("session_id", request.session_id),
                ("source_draft_receipt_id", &source_draft_receipt_id),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("refused_capability", kind.capability()),
                ("terminal_state", status),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("live_provider_call_allowed", "false"),
                ("production_memory_write_allowed", "false"),
                ("tool_execution_allowed", "false"),
                ("hidden_service_role_allowed", "false"),
                ("provider_call_allowed", "false"),
                ("worker_spawn_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_twin_boundary_refusal_run(&run_id);
        Ok(TwinBoundaryRefusalReport {
            status,
            session_id: request.session_id.to_string(),
            source_draft_receipt_id,
            refusal_receipt_id: receipt.receipt_id,
            refused_capability: kind.capability(),
            terminal_state: status,
        })
    }

    fn ensure_twin_draft_receipt(
        &mut self,
        request: &TwinBoundaryRefusalRequest<'_>,
    ) -> Result<String, TwinBoundaryRefusalError> {
        if !request.source_draft_receipt_id.trim().is_empty() {
            return self
                .storage
                .ledger()
                .query()
                .by_id(request.source_draft_receipt_id)
                .filter(|receipt| receipt.kind == "twin.session.draft.admitted")
                .map(|receipt| receipt.receipt_id.clone())
                .ok_or_else(|| {
                    TwinBoundaryRefusalError::UnknownSourceReceipt(
                        request.source_draft_receipt_id.to_string(),
                    )
                });
        }
        let evidence_receipt_id = self.ensure_twin_credential_receipt()?;
        self.save_twin_session_draft_local(TwinSessionDraftRequest {
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            session_id: request.session_id,
            companion_id: "twin_architect",
            companion_stance: "risk_and_architecture",
            persona_profile_id: "architect_stress_tests_assumptions",
            prompt_shape: "prove Twin boundary refusal before live authority",
            evidence_receipt_id: &evidence_receipt_id,
            draft_text: "Local Twin boundary refusal proof draft.",
            live_answer: None,
            recall_packet: None,
        })
        .map(|report| report.draft_receipt_id)
        .map_err(|error| TwinBoundaryRefusalError::ActorAdmission(error.message()))
    }

    fn ensure_twin_credential_receipt(&mut self) -> Result<String, TwinBoundaryRefusalError> {
        if let Some(receipt_id) = self
            .ledger()
            .entries()
            .iter()
            .find(|receipt| receipt.kind == "credential.minted")
            .map(|receipt| receipt.receipt_id.clone())
        {
            return Ok(receipt_id);
        }
        self.run_evals_runner_agent()
            .map_err(TwinBoundaryRefusalError::ActorAdmission)?;
        self.ledger()
            .entries()
            .iter()
            .find(|receipt| receipt.kind == "credential.minted")
            .map(|receipt| receipt.receipt_id.clone())
            .ok_or(TwinBoundaryRefusalError::Missing("credential_receipt"))
    }

    fn start_twin_boundary_refusal_run(&mut self, correlation: &CorrelationIds) -> String {
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

    fn finish_twin_boundary_refusal_run(&mut self, run_id: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "COMPLETED".to_string();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TwinBoundaryRefusalKind {
    ProviderCall,
    ProductionMemoryWrite,
    ToolExecution,
}

impl TwinBoundaryRefusalKind {
    fn capability(self) -> &'static str {
        match self {
            Self::ProviderCall => "live_provider_call",
            Self::ProductionMemoryWrite => "production_memory_write",
            Self::ToolExecution => "tool_execution",
        }
    }

    fn transition(self) -> &'static str {
        match self {
            Self::ProviderCall => "REFUSE_PROVIDER_CALL",
            Self::ProductionMemoryWrite => "REFUSE_PRODUCTION_MEMORY_WRITE",
            Self::ToolExecution => "REFUSE_TOOL_EXECUTION",
        }
    }

    fn action(self) -> ActionKind {
        match self {
            Self::ProviderCall => ActionKind::RefuseTwinProviderCall,
            Self::ProductionMemoryWrite => ActionKind::RefuseTwinProductionMemoryWrite,
            Self::ToolExecution => ActionKind::RefuseTwinToolExecution,
        }
    }

    fn source_route(self) -> &'static str {
        match self {
            Self::ProviderCall => "/twin/provider-call-refusals.json",
            Self::ProductionMemoryWrite => "/twin/production-memory-refusals.json",
            Self::ToolExecution => "/twin/tool-execution-refusals.json",
        }
    }

    fn receipt_kind(self) -> &'static str {
        match self {
            Self::ProviderCall => "twin.session.provider_call.refused",
            Self::ProductionMemoryWrite => "twin.session.production_memory.refused",
            Self::ToolExecution => "twin.session.tool_execution.refused",
        }
    }

    fn status(self) -> &'static str {
        match self {
            Self::ProviderCall => "TWIN_PROVIDER_CALL_REFUSED_LOCAL_ONLY",
            Self::ProductionMemoryWrite => "TWIN_PRODUCTION_MEMORY_REFUSED_LOCAL_ONLY",
            Self::ToolExecution => "TWIN_TOOL_EXECUTION_REFUSED_LOCAL_ONLY",
        }
    }
}
