use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageFanoutRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub fanout_request_id: &'a str,
    pub message_receipt_id: &'a str,
    pub channel_id: &'a str,
    pub delivery_scope: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageFanoutRequestReport {
    pub status: &'static str,
    pub fanout_request_id: String,
    pub fanout_request_receipt_id: String,
    pub policy_decision_id: String,
    pub message_receipt_id: String,
    pub realtime_fanout_allowed: bool,
    pub presence_allowed: bool,
    pub typing_state_allowed: bool,
    pub provider_call_allowed: bool,
    pub production_delivery_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageFanoutRequestError {
    Missing(&'static str),
    UnknownMessageReceipt(String),
    ActorAdmission(String),
}

impl MessageFanoutRequestError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("message fanout request missing {field}"),
            Self::UnknownMessageReceipt(id) => {
                format!("message fanout source receipt {id} is unknown")
            }
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_message_fanout_request_local(
        &mut self,
        request: MessageFanoutRequest<'_>,
    ) -> Result<MessageFanoutRequestReport, MessageFanoutRequestError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_message_fanout_request_local_with_identity(request, &identity)
    }

    pub fn save_message_fanout_request_local_with_identity(
        &mut self,
        request: MessageFanoutRequest<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<MessageFanoutRequestReport, MessageFanoutRequestError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("fanout_request_id", request.fanout_request_id),
            ("channel_id", request.channel_id),
            ("delivery_scope", request.delivery_scope),
        ] {
            if value.trim().is_empty() {
                return Err(MessageFanoutRequestError::Missing(field));
            }
        }
        let message_receipt_id = self.ensure_message_fanout_source(request.message_receipt_id)?;
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "operator",
            "/messages/fanout-requests.json",
            "message.fanout.requested",
            request.fanout_request_id,
        )
        .map_err(|error| MessageFanoutRequestError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("message_fanout_request"),
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
        let decision = self.decide_with_receipt(&correlation, ActionKind::RequestMessageFanout);
        let fanout_receipt = self.transition_receipt(
            &run_id,
            "REQUEST_MESSAGE_FANOUT",
            &correlation,
            &decision,
            "message.fanout.requested",
            payload(&[
                ("fanout_request_id", request.fanout_request_id),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("source_route", "/messages/fanout-requests.json"),
                (
                    "projection_route",
                    "/messages/fanout-requests/projection.json",
                ),
                ("message_receipt_id", &message_receipt_id),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("channel_id", request.channel_id),
                ("delivery_scope", request.delivery_scope),
                ("terminal_state", "FANOUT_REQUEST_RECORDED_REALTIME_BLOCKED"),
                ("realtime_fanout_allowed", "false"),
                ("presence_allowed", "false"),
                ("typing_state_allowed", "false"),
                ("provider_call_allowed", "false"),
                ("production_delivery_allowed", "false"),
            ]),
        );
        self.finish_message_fanout_request_run(&run_id);
        Ok(MessageFanoutRequestReport {
            status: "FANOUT_REQUEST_RECORDED_REALTIME_BLOCKED",
            fanout_request_id: request.fanout_request_id.to_string(),
            fanout_request_receipt_id: fanout_receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            message_receipt_id,
            realtime_fanout_allowed: false,
            presence_allowed: false,
            typing_state_allowed: false,
            provider_call_allowed: false,
            production_delivery_allowed: false,
        })
    }

    fn ensure_message_fanout_source(
        &mut self,
        requested_id: &str,
    ) -> Result<String, MessageFanoutRequestError> {
        if !requested_id.trim().is_empty() {
            if self.storage.ledger().query().by_id(requested_id).is_some() {
                return Ok(requested_id.to_string());
            }
            return Err(MessageFanoutRequestError::UnknownMessageReceipt(
                requested_id.to_string(),
            ));
        }
        if let Some(receipt_id) = self
            .storage
            .ledger()
            .entries()
            .iter()
            .find(|receipt| is_message_thread_message_receipt_kind(&receipt.kind))
            .map(|receipt| receipt.receipt_id.clone())
        {
            return Ok(receipt_id);
        }
        if self.storage.ledger().entries().is_empty() {
            self.run_evals_runner_agent()
                .map_err(|_| MessageFanoutRequestError::Missing("source_receipt"))?;
        }
        let source_receipt_id = self
            .storage
            .ledger()
            .entries()
            .first()
            .map(|receipt| receipt.receipt_id.clone())
            .ok_or(MessageFanoutRequestError::Missing("source_receipt"))?;
        let message = MessageThreadMessage {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            thread_id: "thread_local_receipts",
            channel_id: "local-ops",
            message_id: "message_for_fanout_request",
            content_type: "text",
            body: "Seed a governed Message event before fanout.",
            content_payload: "Seed a governed Message event before fanout.",
            source_receipt_id: &source_receipt_id,
            outbox_topic: "message.human.posted",
        };
        let report = self
            .save_message_thread_message_local(message)
            .map_err(|_| MessageFanoutRequestError::Missing("message_receipt"))?;
        Ok(report.message_receipt_id)
    }

    fn finish_message_fanout_request_run(&mut self, run_id: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "FANOUT_REQUEST_RECORDED_REALTIME_BLOCKED".to_string();
        }
    }
}
