use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinRuntimeControlRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub session_id: &'a str,
    pub item_id: &'a str,
    pub source_draft_receipt_id: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinRuntimeControlReport {
    pub status: &'static str,
    pub session_id: String,
    pub item_kind: &'static str,
    pub item_id: String,
    pub source_draft_receipt_id: String,
    pub control_receipt_id: String,
    pub terminal_state: &'static str,
    pub local_preview_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TwinRuntimeControlError {
    Missing(&'static str),
    UnknownItem { kind: &'static str, item_id: String },
    UnknownSourceReceipt(String),
    ActorAdmission(String),
}

impl TwinRuntimeControlError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("twin runtime control missing {field}"),
            Self::UnknownItem { kind, item_id } => {
                format!("twin runtime control item {item_id} is not a known {kind}")
            }
            Self::UnknownSourceReceipt(id) => {
                format!("twin runtime control source receipt {id} is unknown")
            }
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn preflight_twin_agent_mode_local(
        &mut self,
        request: TwinRuntimeControlRequest<'_>,
    ) -> Result<TwinRuntimeControlReport, TwinRuntimeControlError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.preflight_twin_agent_mode_local_with_identity(request, &identity)
    }

    pub fn preflight_twin_hook_local(
        &mut self,
        request: TwinRuntimeControlRequest<'_>,
    ) -> Result<TwinRuntimeControlReport, TwinRuntimeControlError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.preflight_twin_hook_local_with_identity(request, &identity)
    }

    pub fn preflight_twin_skill_local(
        &mut self,
        request: TwinRuntimeControlRequest<'_>,
    ) -> Result<TwinRuntimeControlReport, TwinRuntimeControlError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.preflight_twin_skill_local_with_identity(request, &identity)
    }

    pub fn preflight_twin_connector_local(
        &mut self,
        request: TwinRuntimeControlRequest<'_>,
    ) -> Result<TwinRuntimeControlReport, TwinRuntimeControlError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.preflight_twin_connector_local_with_identity(request, &identity)
    }

    pub fn preflight_twin_agent_handoff_local(
        &mut self,
        request: TwinRuntimeControlRequest<'_>,
    ) -> Result<TwinRuntimeControlReport, TwinRuntimeControlError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.preflight_twin_agent_handoff_local_with_identity(request, &identity)
    }

    pub fn preflight_twin_agent_mode_local_with_identity(
        &mut self,
        request: TwinRuntimeControlRequest<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<TwinRuntimeControlReport, TwinRuntimeControlError> {
        self.save_twin_runtime_control(request, identity, TwinRuntimeControlKind::AgentMode)
    }

    pub fn preflight_twin_hook_local_with_identity(
        &mut self,
        request: TwinRuntimeControlRequest<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<TwinRuntimeControlReport, TwinRuntimeControlError> {
        self.save_twin_runtime_control(request, identity, TwinRuntimeControlKind::Hook)
    }

    pub fn preflight_twin_skill_local_with_identity(
        &mut self,
        request: TwinRuntimeControlRequest<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<TwinRuntimeControlReport, TwinRuntimeControlError> {
        self.save_twin_runtime_control(request, identity, TwinRuntimeControlKind::Skill)
    }

    pub fn preflight_twin_connector_local_with_identity(
        &mut self,
        request: TwinRuntimeControlRequest<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<TwinRuntimeControlReport, TwinRuntimeControlError> {
        self.save_twin_runtime_control(request, identity, TwinRuntimeControlKind::Connector)
    }

    pub fn preflight_twin_agent_handoff_local_with_identity(
        &mut self,
        request: TwinRuntimeControlRequest<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<TwinRuntimeControlReport, TwinRuntimeControlError> {
        self.save_twin_runtime_control(request, identity, TwinRuntimeControlKind::AgentHandoff)
    }

    fn save_twin_runtime_control(
        &mut self,
        request: TwinRuntimeControlRequest<'_>,
        identity: &GovernedWriteIdentity,
        kind: TwinRuntimeControlKind,
    ) -> Result<TwinRuntimeControlReport, TwinRuntimeControlError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("session_id", request.session_id),
            ("item_id", request.item_id),
        ] {
            if value.trim().is_empty() {
                return Err(TwinRuntimeControlError::Missing(field));
            }
        }
        let item =
            kind.item(request.item_id)
                .ok_or_else(|| TwinRuntimeControlError::UnknownItem {
                    kind: kind.item_kind(),
                    item_id: request.item_id.to_string(),
                })?;
        let source_draft_receipt_id = self.ensure_twin_runtime_control_draft_receipt(&request)?;
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "operator",
            kind.write_route(),
            kind.receipt_kind(),
            request.session_id,
        )
        .map_err(|error| TwinRuntimeControlError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("twin_runtime_control"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.start_twin_runtime_control_run(&correlation);
        let decision = self.decide_with_receipt(&correlation, kind.action());
        let receipt = self.transition_receipt(
            &run_id,
            kind.transition(),
            &correlation,
            &decision,
            kind.receipt_kind(),
            payload(&[
                ("session_id", request.session_id),
                ("source_draft_receipt_id", &source_draft_receipt_id),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("item_kind", kind.item_kind()),
                ("item_id", request.item_id),
                ("required_receipt_kind", item.required_receipt_kind),
                ("terminal_state", kind.terminal_state()),
                (
                    "local_preview_allowed",
                    bool_str(item.local_preview_allowed),
                ),
                ("authority_required", bool_str(item.authority_required)),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("live_provider_call_allowed", "false"),
                ("tool_execution_allowed", "false"),
                ("connector_execution_allowed", "false"),
                ("network_allowed", "false"),
                ("secret_values_recorded", "false"),
                ("provider_secret_values_recorded", "false"),
                ("output_text_recorded", "false"),
                ("worker_spawn_allowed", "false"),
                ("agent_swarm_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_twin_runtime_control_run(&run_id);
        Ok(TwinRuntimeControlReport {
            status: kind.status(),
            session_id: request.session_id.to_string(),
            item_kind: kind.item_kind(),
            item_id: request.item_id.to_string(),
            source_draft_receipt_id,
            control_receipt_id: receipt.receipt_id,
            terminal_state: kind.terminal_state(),
            local_preview_allowed: item.local_preview_allowed,
        })
    }

    fn ensure_twin_runtime_control_draft_receipt(
        &mut self,
        request: &TwinRuntimeControlRequest<'_>,
    ) -> Result<String, TwinRuntimeControlError> {
        if !request.source_draft_receipt_id.trim().is_empty() {
            return self
                .storage
                .ledger()
                .query()
                .by_id(request.source_draft_receipt_id)
                .filter(|receipt| receipt.kind == "twin.session.draft.admitted")
                .map(|receipt| receipt.receipt_id.clone())
                .ok_or_else(|| {
                    TwinRuntimeControlError::UnknownSourceReceipt(
                        request.source_draft_receipt_id.to_string(),
                    )
                });
        }
        let evidence_receipt_id = if let Some(receipt_id) = self
            .ledger()
            .entries()
            .iter()
            .find(|receipt| receipt.kind == "credential.minted")
            .map(|receipt| receipt.receipt_id.clone())
        {
            receipt_id
        } else {
            self.run_evals_runner_agent()
                .map_err(TwinRuntimeControlError::ActorAdmission)?;
            self.ledger()
                .entries()
                .iter()
                .find(|receipt| receipt.kind == "credential.minted")
                .map(|receipt| receipt.receipt_id.clone())
                .ok_or(TwinRuntimeControlError::Missing("credential_receipt"))?
        };
        self.save_twin_session_draft_local(TwinSessionDraftRequest {
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            session_id: request.session_id,
            companion_id: "twin_architect",
            companion_stance: "risk_and_architecture",
            persona_profile_id: "architect_stress_tests_assumptions",
            prompt_shape: "prove Twin runtime control before live authority",
            evidence_receipt_id: &evidence_receipt_id,
            draft_text: "Local Twin runtime control proof draft.",
            live_answer: None,
            recall_packet: None,
        })
        .map(|report| report.draft_receipt_id)
        .map_err(|error| TwinRuntimeControlError::ActorAdmission(error.message()))
    }

    fn start_twin_runtime_control_run(&mut self, correlation: &CorrelationIds) -> String {
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

    fn finish_twin_runtime_control_run(&mut self, run_id: &str) {
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
enum TwinRuntimeControlKind {
    AgentMode,
    Hook,
    Skill,
    Connector,
    AgentHandoff,
}

impl TwinRuntimeControlKind {
    fn action(self) -> ActionKind {
        match self {
            Self::AgentMode => ActionKind::PreflightTwinAgentMode,
            Self::Hook => ActionKind::PreflightTwinHook,
            Self::Skill => ActionKind::PreflightTwinSkill,
            Self::Connector => ActionKind::PreflightTwinConnector,
            Self::AgentHandoff => ActionKind::PreflightTwinAgentHandoff,
        }
    }

    fn item_kind(self) -> &'static str {
        match self {
            Self::AgentMode => "agent_mode",
            Self::Hook => "hook",
            Self::Skill => "skill",
            Self::Connector => "connector",
            Self::AgentHandoff => "agent_handoff",
        }
    }

    fn write_route(self) -> &'static str {
        match self {
            Self::AgentMode => "/twin/agent-mode-preflights.json",
            Self::Hook => "/twin/hook-preflights.json",
            Self::Skill => "/twin/skill-preflights.json",
            Self::Connector => "/twin/connector-preflights.json",
            Self::AgentHandoff => "/twin/agent-handoff-preflights.json",
        }
    }

    fn receipt_kind(self) -> &'static str {
        match self {
            Self::AgentMode => "twin.agent_mode.preflighted",
            Self::Hook => "twin.hook.preflighted",
            Self::Skill => "twin.skill.preflighted",
            Self::Connector => "twin.connector.preflighted",
            Self::AgentHandoff => "twin.agent_handoff.preflighted",
        }
    }

    fn status(self) -> &'static str {
        match self {
            Self::AgentMode => "TWIN_AGENT_MODE_PREFLIGHT_RECORDED_AUTHORITY_BLOCKED",
            Self::Hook => "TWIN_HOOK_PREFLIGHT_RECORDED_EXECUTION_BLOCKED",
            Self::Skill => "TWIN_SKILL_PREFLIGHT_RECORDED_PROVIDER_OR_TOOL_BLOCKED",
            Self::Connector => "TWIN_CONNECTOR_PREFLIGHT_RECORDED_EXECUTION_BLOCKED",
            Self::AgentHandoff => "TWIN_AGENT_HANDOFF_PREFLIGHT_RECORDED_SWARM_BLOCKED",
        }
    }

    fn terminal_state(self) -> &'static str {
        self.status()
    }

    fn transition(self) -> &'static str {
        match self {
            Self::AgentMode => "PREFLIGHT_AGENT_MODE",
            Self::Hook => "PREFLIGHT_HOOK",
            Self::Skill => "PREFLIGHT_SKILL",
            Self::Connector => "PREFLIGHT_CONNECTOR",
            Self::AgentHandoff => "PREFLIGHT_AGENT_HANDOFF",
        }
    }

    fn item(self, id: &str) -> Option<TwinRuntimeControlItem> {
        match self {
            Self::AgentMode => item_from(
                id,
                &[
                    ("companion_copilot", "none", true, false),
                    (
                        "research_scout",
                        "twin.research_scout.approved",
                        false,
                        true,
                    ),
                    (
                        "product_partner",
                        "twin.product_partner.approved",
                        false,
                        true,
                    ),
                    (
                        "memory_curator",
                        "twin.memory_curator.approved",
                        false,
                        true,
                    ),
                    (
                        "forge_reviewer",
                        "twin.forge_reviewer.approved",
                        false,
                        true,
                    ),
                ],
            ),
            Self::Hook => item_from(
                id,
                &[
                    (
                        "session_start",
                        "twin.hook.session_start.observed",
                        true,
                        false,
                    ),
                    (
                        "pre_provider_call",
                        "twin.hook.pre_provider_call.approved",
                        false,
                        true,
                    ),
                    (
                        "memory_consolidation",
                        "twin.hook.memory_consolidation.observed",
                        true,
                        false,
                    ),
                    (
                        "pre_tool_execution",
                        "twin.hook.pre_tool_execution.approved",
                        false,
                        true,
                    ),
                    (
                        "handoff_review",
                        "twin.hook.handoff_review.approved",
                        false,
                        true,
                    ),
                ],
            ),
            Self::Skill => item_from(
                id,
                &[
                    (
                        "grounded_answer",
                        "twin.skill.grounded_answer.observed",
                        true,
                        false,
                    ),
                    (
                        "memory_recall",
                        "twin.skill.memory_recall.observed",
                        true,
                        false,
                    ),
                    (
                        "provider_reasoning",
                        "twin.skill.provider_reasoning.approved",
                        false,
                        true,
                    ),
                    ("tool_use", "twin.skill.tool_use.approved", false, true),
                    (
                        "forge_review",
                        "twin.skill.forge_review.approved",
                        false,
                        true,
                    ),
                ],
            ),
            Self::Connector => item_from(
                id,
                &[
                    (
                        "pages_world_model",
                        "twin.connector.pages_world_model.observed",
                        true,
                        false,
                    ),
                    (
                        "message_context",
                        "twin.connector.message_context.observed",
                        true,
                        false,
                    ),
                    (
                        "provider_gateway",
                        "twin.connector.provider_gateway.approved",
                        false,
                        true,
                    ),
                    (
                        "mem0_memory",
                        "twin.connector.mem0_memory.approved",
                        false,
                        true,
                    ),
                    (
                        "external_tools",
                        "twin.connector.external_tools.approved",
                        false,
                        true,
                    ),
                ],
            ),
            Self::AgentHandoff => item_from(
                id,
                &[
                    ("forge_reviewer", "twin.agent_handoff.approved", false, true),
                    ("memory_curator", "twin.agent_handoff.approved", false, true),
                    ("research_scout", "twin.agent_handoff.approved", false, true),
                ],
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TwinRuntimeControlItem {
    required_receipt_kind: &'static str,
    local_preview_allowed: bool,
    authority_required: bool,
}

fn item_from(
    id: &str,
    items: &[(&'static str, &'static str, bool, bool)],
) -> Option<TwinRuntimeControlItem> {
    items.iter().find(|(item_id, _, _, _)| *item_id == id).map(
        |(_, required_receipt_kind, local_preview_allowed, authority_required)| {
            TwinRuntimeControlItem {
                required_receipt_kind,
                local_preview_allowed: *local_preview_allowed,
                authority_required: *authority_required,
            }
        },
    )
}

fn bool_str(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(item_id: &str) -> TwinRuntimeControlRequest<'_> {
        TwinRuntimeControlRequest {
            tenant_id: "tenant_local",
            actor_id: "local_operator",
            session_id: "twin_runtime_control_test",
            item_id,
            source_draft_receipt_id: "",
        }
    }

    #[test]
    fn twin_runtime_controls_record_all_lanes_without_opening_authority() {
        let mut kernel = MdxKernel::boot_local();
        let mode = kernel
            .preflight_twin_agent_mode_local(request("research_scout"))
            .expect("mode preflight");
        let hook = kernel
            .preflight_twin_hook_local(TwinRuntimeControlRequest {
                item_id: "pre_provider_call",
                source_draft_receipt_id: &mode.source_draft_receipt_id,
                ..request("pre_provider_call")
            })
            .expect("hook preflight");
        let skill = kernel
            .preflight_twin_skill_local(TwinRuntimeControlRequest {
                item_id: "provider_reasoning",
                source_draft_receipt_id: &mode.source_draft_receipt_id,
                ..request("provider_reasoning")
            })
            .expect("skill preflight");
        let connector = kernel
            .preflight_twin_connector_local(TwinRuntimeControlRequest {
                item_id: "provider_gateway",
                source_draft_receipt_id: &mode.source_draft_receipt_id,
                ..request("provider_gateway")
            })
            .expect("connector preflight");
        let handoff = kernel
            .preflight_twin_agent_handoff_local(TwinRuntimeControlRequest {
                item_id: "forge_reviewer",
                source_draft_receipt_id: &mode.source_draft_receipt_id,
                ..request("forge_reviewer")
            })
            .expect("handoff preflight");

        for report in [&mode, &hook, &skill, &connector, &handoff] {
            assert!(!report.control_receipt_id.is_empty());
            assert!(!report.source_draft_receipt_id.is_empty());
        }
        let receipts = kernel.ledger().entries();
        for kind in [
            "twin.agent_mode.preflighted",
            "twin.hook.preflighted",
            "twin.skill.preflighted",
            "twin.connector.preflighted",
            "twin.agent_handoff.preflighted",
        ] {
            let receipt = receipts
                .iter()
                .find(|receipt| receipt.kind == kind)
                .expect(kind);
            assert_eq!(
                receipt
                    .payload
                    .get("live_provider_call_allowed")
                    .map(String::as_str),
                Some("false")
            );
            assert_eq!(
                receipt
                    .payload
                    .get("tool_execution_allowed")
                    .map(String::as_str),
                Some("false")
            );
            assert_eq!(
                receipt
                    .payload
                    .get("connector_execution_allowed")
                    .map(String::as_str),
                Some("false")
            );
            assert_eq!(
                receipt.payload.get("network_allowed").map(String::as_str),
                Some("false")
            );
            assert_eq!(
                receipt
                    .payload
                    .get("agent_swarm_allowed")
                    .map(String::as_str),
                Some("false")
            );
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
    fn twin_runtime_control_rejects_unknown_items() {
        let mut kernel = MdxKernel::boot_local();
        let error = kernel
            .preflight_twin_skill_local(request("unsafe_unknown_skill"))
            .expect_err("unknown skill is rejected");
        assert!(matches!(
            error,
            TwinRuntimeControlError::UnknownItem { kind: "skill", .. }
        ));
    }
}
