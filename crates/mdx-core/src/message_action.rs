use crate::*;

pub const MESSAGE_ACTION_VERDICTS: &[&str] = &["approve", "edit", "reject", "respond"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageActionRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub thread_id: &'a str,
    pub channel_id: &'a str,
    pub action_request_id: &'a str,
    pub source_receipt_id: &'a str,
    pub title: &'a str,
    pub summary: &'a str,
    pub proposed_action: &'a str,
    pub action_payload: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageActionRequestReport {
    pub status: &'static str,
    pub action_request_id: String,
    pub action_request_receipt_id: String,
    pub policy_decision_id: String,
    pub thread_id: String,
    pub channel_id: String,
    pub source_receipt_id: String,
    pub source_receipt_kind: String,
    pub title: String,
    pub summary: String,
    pub proposed_action: String,
    pub action_payload: String,
    pub allowed_verdicts: String,
    pub gate_state: &'static str,
    pub execution_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageActionVerdict<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub actor_role: &'a str,
    pub action_verdict_id: &'a str,
    pub action_request_receipt_id: &'a str,
    pub verdict: &'a str,
    pub decision_note: &'a str,
    pub edited_action_payload: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageActionVerdictReport {
    pub status: &'static str,
    pub action_verdict_id: String,
    pub action_verdict_receipt_id: String,
    pub policy_decision_id: String,
    pub action_request_receipt_id: String,
    pub action_request_id: String,
    pub thread_id: String,
    pub channel_id: String,
    pub title: String,
    pub verdict: String,
    pub decision_note: String,
    pub edited_action_payload: String,
    pub human_decision_recorded: bool,
    pub human_approval_granted: bool,
    pub execution_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageActionError {
    Missing(&'static str),
    UnknownSourceReceipt(String),
    UnknownActionRequestReceipt(String),
    InvalidVerdict(String),
    ActorAdmission(String),
}

impl MessageActionError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("message action missing {field}"),
            Self::UnknownSourceReceipt(id) => {
                format!("message action source receipt {id} is unknown")
            }
            Self::UnknownActionRequestReceipt(id) => {
                format!("message action request receipt {id} is unknown")
            }
            Self::InvalidVerdict(verdict) => format!(
                "message action verdict must be one of {}; got {verdict}",
                MESSAGE_ACTION_VERDICTS.join(", ")
            ),
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_message_action_request_local(
        &mut self,
        request: MessageActionRequest<'_>,
    ) -> Result<MessageActionRequestReport, MessageActionError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_message_action_request_local_with_identity(request, &identity)
    }

    pub fn save_message_action_request_local_with_identity(
        &mut self,
        request: MessageActionRequest<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<MessageActionRequestReport, MessageActionError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("thread_id", request.thread_id),
            ("channel_id", request.channel_id),
            ("action_request_id", request.action_request_id),
            ("source_receipt_id", request.source_receipt_id),
            ("title", request.title),
            ("summary", request.summary),
            ("proposed_action", request.proposed_action),
            ("action_payload", request.action_payload),
        ] {
            if value.trim().is_empty() {
                return Err(MessageActionError::Missing(field));
            }
        }
        let source_receipt = self
            .storage
            .ledger()
            .query()
            .by_id(request.source_receipt_id)
            .cloned()
            .ok_or_else(|| {
                MessageActionError::UnknownSourceReceipt(request.source_receipt_id.to_string())
            })?;
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "agent",
            "/messages/action-requests.json",
            "message.action.requested",
            request.action_request_id,
        )
        .map_err(|error| MessageActionError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("message_action_request"),
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
        let decision = self.decide_with_receipt(&correlation, ActionKind::RequestMessageAction);
        let receipt = self.transition_receipt(
            &run_id,
            "REQUEST_MESSAGE_ACTION",
            &correlation,
            &decision,
            "message.action.requested",
            payload(&[
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("source_route", "/messages/action-requests.json"),
                (
                    "projection_route",
                    "/messages/action-requests/projection.json",
                ),
                ("action_request_id", request.action_request_id),
                ("thread_id", request.thread_id),
                ("channel_id", request.channel_id),
                ("source_receipt_id", request.source_receipt_id),
                ("source_receipt_kind", &source_receipt.kind),
                ("title", request.title),
                ("summary", request.summary),
                ("proposed_action", request.proposed_action),
                ("action_payload", request.action_payload),
                ("allowed_verdicts", &MESSAGE_ACTION_VERDICTS.join(",")),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("gate_state", "WAITING_FOR_HUMAN_VERDICT"),
                (
                    "terminal_state",
                    "MESSAGE_ACTION_REQUEST_RECORDED_EXECUTION_BLOCKED",
                ),
                ("execution_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_message_action_run(
            &run_id,
            "MESSAGE_ACTION_REQUEST_RECORDED_EXECUTION_BLOCKED",
        );
        Ok(MessageActionRequestReport {
            status: "MESSAGE_ACTION_REQUEST_RECORDED_EXECUTION_BLOCKED",
            action_request_id: request.action_request_id.to_string(),
            action_request_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            thread_id: request.thread_id.to_string(),
            channel_id: request.channel_id.to_string(),
            source_receipt_id: request.source_receipt_id.to_string(),
            source_receipt_kind: source_receipt.kind,
            title: request.title.to_string(),
            summary: request.summary.to_string(),
            proposed_action: request.proposed_action.to_string(),
            action_payload: request.action_payload.to_string(),
            allowed_verdicts: MESSAGE_ACTION_VERDICTS.join(","),
            gate_state: "WAITING_FOR_HUMAN_VERDICT",
            execution_allowed: false,
            production_write_allowed: false,
        })
    }

    pub fn save_message_action_verdict_local(
        &mut self,
        request: MessageActionVerdict<'_>,
    ) -> Result<MessageActionVerdictReport, MessageActionError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_message_action_verdict_local_with_identity(request, &identity)
    }

    pub fn save_message_action_verdict_local_with_identity(
        &mut self,
        request: MessageActionVerdict<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<MessageActionVerdictReport, MessageActionError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("actor_role", request.actor_role),
            ("action_verdict_id", request.action_verdict_id),
            (
                "action_request_receipt_id",
                request.action_request_receipt_id,
            ),
            ("verdict", request.verdict),
            ("decision_note", request.decision_note),
        ] {
            if value.trim().is_empty() {
                return Err(MessageActionError::Missing(field));
            }
        }
        let verdict = normalize_message_action_verdict(request.verdict)?;
        let action_request_receipt = self
            .storage
            .ledger()
            .query()
            .by_id(request.action_request_receipt_id)
            .filter(|receipt| receipt.kind == "message.action.requested")
            .cloned()
            .ok_or_else(|| {
                MessageActionError::UnknownActionRequestReceipt(
                    request.action_request_receipt_id.to_string(),
                )
            })?;
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            request.actor_role,
            "/messages/action-verdicts.json",
            "message.action.verdict.recorded",
            request.action_verdict_id,
        )
        .map_err(|error| MessageActionError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("message_action_verdict"),
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
            self.decide_with_receipt(&correlation, ActionKind::RecordMessageActionVerdict);
        let receipt = self.transition_receipt(
            &run_id,
            "RECORD_MESSAGE_ACTION_VERDICT",
            &correlation,
            &decision,
            "message.action.verdict.recorded",
            payload(&[
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("source_route", "/messages/action-verdicts.json"),
                (
                    "projection_route",
                    "/messages/action-verdicts/projection.json",
                ),
                ("action_verdict_id", request.action_verdict_id),
                (
                    "action_request_receipt_id",
                    request.action_request_receipt_id,
                ),
                (
                    "action_request_id",
                    payload_value(&action_request_receipt, "action_request_id"),
                ),
                (
                    "thread_id",
                    payload_value(&action_request_receipt, "thread_id"),
                ),
                (
                    "channel_id",
                    payload_value(&action_request_receipt, "channel_id"),
                ),
                ("title", payload_value(&action_request_receipt, "title")),
                ("verdict", verdict),
                ("decision_note", request.decision_note),
                ("edited_action_payload", request.edited_action_payload),
                ("actor_role", request.actor_role),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                (
                    "terminal_state",
                    "MESSAGE_ACTION_VERDICT_RECORDED_EXECUTION_BLOCKED",
                ),
                ("human_decision_recorded", "true"),
                ("human_approval_granted", bool_string(verdict == "approve")),
                ("execution_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_message_action_run(
            &run_id,
            "MESSAGE_ACTION_VERDICT_RECORDED_EXECUTION_BLOCKED",
        );
        Ok(MessageActionVerdictReport {
            status: "MESSAGE_ACTION_VERDICT_RECORDED_EXECUTION_BLOCKED",
            action_verdict_id: request.action_verdict_id.to_string(),
            action_verdict_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            action_request_receipt_id: request.action_request_receipt_id.to_string(),
            action_request_id: payload_value(&action_request_receipt, "action_request_id")
                .to_string(),
            thread_id: payload_value(&action_request_receipt, "thread_id").to_string(),
            channel_id: payload_value(&action_request_receipt, "channel_id").to_string(),
            title: payload_value(&action_request_receipt, "title").to_string(),
            verdict: verdict.to_string(),
            decision_note: request.decision_note.to_string(),
            edited_action_payload: request.edited_action_payload.to_string(),
            human_decision_recorded: true,
            human_approval_granted: verdict == "approve",
            execution_allowed: false,
            production_write_allowed: false,
        })
    }

    fn finish_message_action_run(&mut self, run_id: &str, status: &str) {
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

fn normalize_message_action_verdict(verdict: &str) -> Result<&'static str, MessageActionError> {
    let normalized = verdict.trim().to_ascii_lowercase();
    MESSAGE_ACTION_VERDICTS
        .iter()
        .copied()
        .find(|allowed| *allowed == normalized)
        .ok_or(MessageActionError::InvalidVerdict(normalized))
}

fn payload_value<'a>(receipt: &'a Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn bool_string(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}
