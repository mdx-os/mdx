use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessagePresenceRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub presence_request_id: &'a str,
    pub fanout_request_receipt_id: &'a str,
    pub thread_id: &'a str,
    pub channel_id: &'a str,
    pub presence_scope: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessagePresenceRequestReport {
    pub status: &'static str,
    pub presence_request_id: String,
    pub presence_request_receipt_id: String,
    pub policy_decision_id: String,
    pub fanout_request_receipt_id: String,
    pub thread_id: String,
    pub channel_id: String,
    pub realtime_presence_allowed: bool,
    pub websocket_fanout_allowed: bool,
    pub typing_indicator_allowed: bool,
    pub provider_call_allowed: bool,
    pub production_delivery_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessagePresenceRequestError {
    Missing(&'static str),
    UnknownFanoutReceipt(String),
    ActorAdmission(String),
}

impl MessagePresenceRequestError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("message presence request missing {field}"),
            Self::UnknownFanoutReceipt(id) => {
                format!("message presence source receipt {id} is unknown")
            }
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_message_presence_request_local(
        &mut self,
        request: MessagePresenceRequest<'_>,
    ) -> Result<MessagePresenceRequestReport, MessagePresenceRequestError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_message_presence_request_local_with_identity(request, &identity)
    }

    pub fn save_message_presence_request_local_with_identity(
        &mut self,
        request: MessagePresenceRequest<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<MessagePresenceRequestReport, MessagePresenceRequestError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("presence_request_id", request.presence_request_id),
            ("thread_id", request.thread_id),
            ("channel_id", request.channel_id),
            ("presence_scope", request.presence_scope),
        ] {
            if value.trim().is_empty() {
                return Err(MessagePresenceRequestError::Missing(field));
            }
        }
        let fanout_request_receipt_id =
            self.ensure_message_presence_source(request.fanout_request_receipt_id)?;
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "operator",
            "/messages/presence-requests.json",
            "message.presence.requested",
            request.presence_request_id,
        )
        .map_err(|error| MessagePresenceRequestError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("message_presence_request"),
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
        let decision = self.decide_with_receipt(&correlation, ActionKind::RequestMessagePresence);
        let presence_receipt = self.transition_receipt(
            &run_id,
            "REQUEST_MESSAGE_PRESENCE",
            &correlation,
            &decision,
            "message.presence.requested",
            payload(&[
                ("presence_request_id", request.presence_request_id),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("source_route", "/messages/presence-requests.json"),
                (
                    "projection_route",
                    "/messages/presence-requests/projection.json",
                ),
                ("fanout_request_receipt_id", &fanout_request_receipt_id),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("thread_id", request.thread_id),
                ("channel_id", request.channel_id),
                ("presence_scope", request.presence_scope),
                (
                    "terminal_state",
                    "PRESENCE_REQUEST_RECORDED_REALTIME_BLOCKED",
                ),
                ("realtime_presence_allowed", "false"),
                ("websocket_fanout_allowed", "false"),
                ("typing_indicator_allowed", "false"),
                ("provider_call_allowed", "false"),
                ("production_delivery_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_message_presence_request_run(&run_id);
        Ok(MessagePresenceRequestReport {
            status: "PRESENCE_REQUEST_RECORDED_REALTIME_BLOCKED",
            presence_request_id: request.presence_request_id.to_string(),
            presence_request_receipt_id: presence_receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            fanout_request_receipt_id,
            thread_id: request.thread_id.to_string(),
            channel_id: request.channel_id.to_string(),
            realtime_presence_allowed: false,
            websocket_fanout_allowed: false,
            typing_indicator_allowed: false,
            provider_call_allowed: false,
            production_delivery_allowed: false,
            production_write_allowed: false,
        })
    }

    fn ensure_message_presence_source(
        &mut self,
        requested_id: &str,
    ) -> Result<String, MessagePresenceRequestError> {
        if !requested_id.trim().is_empty() {
            if self.storage.ledger().query().by_id(requested_id).is_some() {
                return Ok(requested_id.to_string());
            }
            return Err(MessagePresenceRequestError::UnknownFanoutReceipt(
                requested_id.to_string(),
            ));
        }
        if let Some(receipt_id) = self
            .storage
            .ledger()
            .query()
            .by_kind("message.fanout.requested")
            .first()
            .map(|receipt| receipt.receipt_id.clone())
        {
            return Ok(receipt_id);
        }
        let fanout = MessageFanoutRequest {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            fanout_request_id: "fanout_for_presence_request",
            message_receipt_id: "",
            channel_id: "local-ops",
            delivery_scope: "local projection only",
        };
        let report = self
            .save_message_fanout_request_local(fanout)
            .map_err(|_| MessagePresenceRequestError::Missing("fanout_request_receipt"))?;
        Ok(report.fanout_request_receipt_id)
    }

    fn finish_message_presence_request_run(&mut self, run_id: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "PRESENCE_REQUEST_RECORDED_REALTIME_BLOCKED".to_string();
        }
    }
}
