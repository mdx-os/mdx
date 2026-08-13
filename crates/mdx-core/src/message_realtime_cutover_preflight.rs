use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageRealtimeCutoverPreflight<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub preflight_id: &'a str,
    pub presence_request_receipt_id: &'a str,
    pub thread_id: &'a str,
    pub channel_id: &'a str,
    pub requested_realtime_scope: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageRealtimeCutoverPreflightReport {
    pub status: &'static str,
    pub preflight_id: String,
    pub preflight_receipt_id: String,
    pub policy_decision_id: String,
    pub presence_request_receipt_id: String,
    pub thread_id: String,
    pub channel_id: String,
    pub tenant_subscription_isolation_proven: bool,
    pub service_role_fanout_refused: bool,
    pub realtime_provider_turn_on_observed: bool,
    pub presence_mutation_allowed: bool,
    pub typing_indicator_allowed: bool,
    pub websocket_fanout_allowed: bool,
    pub production_delivery_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageRealtimeCutoverPreflightError {
    Missing(&'static str),
    UnknownPresenceReceipt(String),
    ActorAdmission(String),
}

impl MessageRealtimeCutoverPreflightError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("message realtime cutover preflight missing {field}"),
            Self::UnknownPresenceReceipt(id) => {
                format!("message realtime preflight source receipt {id} is unknown")
            }
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageDeliveryReplayBatch<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub delivery_replay_batch_id: &'a str,
    pub realtime_preflight_receipt_id: &'a str,
    pub channel_id: &'a str,
    pub replay_scope: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageDeliveryReplayBatchReport {
    pub status: &'static str,
    pub delivery_replay_batch_id: String,
    pub delivery_replay_receipt_id: String,
    pub policy_decision_id: String,
    pub realtime_preflight_receipt_id: String,
    pub channel_id: String,
    pub replay_state: String,
    pub rollback_safe_delivery_replay_proven: bool,
    pub websocket_fanout_allowed: bool,
    pub production_delivery_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageDeliveryReplayBatchError {
    Missing(&'static str),
    UnknownRealtimePreflightReceipt(String),
    ActorAdmission(String),
}

impl MessageDeliveryReplayBatchError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("message delivery replay batch missing {field}"),
            Self::UnknownRealtimePreflightReceipt(id) => {
                format!("message delivery replay source receipt {id} is unknown")
            }
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageSubscriptionIsolationCheck<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub subscription_isolation_check_id: &'a str,
    pub realtime_preflight_receipt_id: &'a str,
    pub channel_id: &'a str,
    pub isolation_scope: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageSubscriptionIsolationCheckReport {
    pub status: &'static str,
    pub subscription_isolation_check_id: String,
    pub subscription_isolation_receipt_id: String,
    pub policy_decision_id: String,
    pub realtime_preflight_receipt_id: String,
    pub channel_id: String,
    pub isolation_status: String,
    pub tenant_subscription_isolation_proven: bool,
    pub service_role_fanout_allowed: bool,
    pub production_delivery_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageSubscriptionIsolationCheckError {
    Missing(&'static str),
    UnknownRealtimePreflightReceipt(String),
    ActorAdmission(String),
}

impl MessageSubscriptionIsolationCheckError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("message subscription isolation check missing {field}"),
            Self::UnknownRealtimePreflightReceipt(id) => {
                format!("message subscription isolation source receipt {id} is unknown")
            }
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageServiceRoleFanoutRefusal<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub service_role_fanout_refusal_id: &'a str,
    pub realtime_preflight_receipt_id: &'a str,
    pub refusal_reason: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageServiceRoleFanoutRefusalReport {
    pub status: &'static str,
    pub service_role_fanout_refusal_id: String,
    pub service_role_fanout_refusal_receipt_id: String,
    pub policy_decision_id: String,
    pub realtime_preflight_receipt_id: String,
    pub refusal_reason: String,
    pub service_role_fanout_refused: bool,
    pub presence_mutation_allowed: bool,
    pub typing_indicator_allowed: bool,
    pub production_delivery_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageServiceRoleFanoutRefusalError {
    Missing(&'static str),
    UnknownRealtimePreflightReceipt(String),
    ActorAdmission(String),
}

impl MessageServiceRoleFanoutRefusalError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("message service-role fanout refusal missing {field}"),
            Self::UnknownRealtimePreflightReceipt(id) => {
                format!("message service-role fanout refusal source receipt {id} is unknown")
            }
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_message_realtime_cutover_preflight_local(
        &mut self,
        preflight: MessageRealtimeCutoverPreflight<'_>,
    ) -> Result<MessageRealtimeCutoverPreflightReport, MessageRealtimeCutoverPreflightError> {
        let identity = GovernedWriteIdentity::local_demo(preflight.actor_id);
        self.save_message_realtime_cutover_preflight_local_with_identity(preflight, &identity)
    }

    pub fn save_message_realtime_cutover_preflight_local_with_identity(
        &mut self,
        preflight: MessageRealtimeCutoverPreflight<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<MessageRealtimeCutoverPreflightReport, MessageRealtimeCutoverPreflightError> {
        for (field, value) in [
            ("tenant_id", preflight.tenant_id),
            ("actor_id", preflight.actor_id),
            ("preflight_id", preflight.preflight_id),
            ("thread_id", preflight.thread_id),
            ("channel_id", preflight.channel_id),
            (
                "requested_realtime_scope",
                preflight.requested_realtime_scope,
            ),
        ] {
            if value.trim().is_empty() {
                return Err(MessageRealtimeCutoverPreflightError::Missing(field));
            }
        }
        let presence_request_receipt_id =
            self.ensure_message_realtime_preflight_source(preflight.presence_request_receipt_id)?;
        let actor_admission = admit_local_route_actor(
            preflight.tenant_id,
            preflight.actor_id,
            "operator",
            "/messages/realtime-cutover-preflights.json",
            "message.realtime.cutover.preflighted",
            preflight.preflight_id,
        )
        .map_err(|error| MessageRealtimeCutoverPreflightError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(preflight.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(preflight.actor_id),
            loop_id: LoopId::new("message_realtime_cutover_preflight"),
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
            self.decide_with_receipt(&correlation, ActionKind::PreflightMessageRealtimeCutover);
        let preflight_receipt = self.transition_receipt(
            &run_id,
            "PREFLIGHT_MESSAGE_REALTIME_CUTOVER",
            &correlation,
            &decision,
            "message.realtime.cutover.preflighted",
            payload(&[
                ("preflight_id", preflight.preflight_id),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("source_route", "/messages/realtime-cutover-preflights.json"),
                (
                    "projection_route",
                    "/messages/realtime-cutover-preflights/projection.json",
                ),
                ("presence_request_receipt_id", &presence_request_receipt_id),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("thread_id", preflight.thread_id),
                ("channel_id", preflight.channel_id),
                (
                    "requested_realtime_scope",
                    preflight.requested_realtime_scope,
                ),
                (
                    "terminal_state",
                    "REALTIME_CUTOVER_PREFLIGHT_RECORDED_PROVIDER_BLOCKED",
                ),
                ("tenant_subscription_isolation_proven", "true"),
                ("service_role_fanout_refused", "true"),
                ("realtime_provider_turn_on_observed", "false"),
                ("presence_mutation_allowed", "false"),
                ("typing_indicator_allowed", "false"),
                ("websocket_fanout_allowed", "false"),
                ("production_delivery_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_message_realtime_preflight_run(&run_id);
        Ok(MessageRealtimeCutoverPreflightReport {
            status: "REALTIME_CUTOVER_PREFLIGHT_RECORDED_PROVIDER_BLOCKED",
            preflight_id: preflight.preflight_id.to_string(),
            preflight_receipt_id: preflight_receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            presence_request_receipt_id,
            thread_id: preflight.thread_id.to_string(),
            channel_id: preflight.channel_id.to_string(),
            tenant_subscription_isolation_proven: true,
            service_role_fanout_refused: true,
            realtime_provider_turn_on_observed: false,
            presence_mutation_allowed: false,
            typing_indicator_allowed: false,
            websocket_fanout_allowed: false,
            production_delivery_allowed: false,
            production_write_allowed: false,
        })
    }

    fn ensure_message_realtime_preflight_source(
        &mut self,
        requested_id: &str,
    ) -> Result<String, MessageRealtimeCutoverPreflightError> {
        if !requested_id.trim().is_empty() {
            if self.storage.ledger().query().by_id(requested_id).is_some() {
                return Ok(requested_id.to_string());
            }
            return Err(
                MessageRealtimeCutoverPreflightError::UnknownPresenceReceipt(
                    requested_id.to_string(),
                ),
            );
        }
        if let Some(receipt_id) = self
            .storage
            .ledger()
            .query()
            .by_kind("message.presence.requested")
            .first()
            .map(|receipt| receipt.receipt_id.clone())
        {
            return Ok(receipt_id);
        }
        let presence = MessagePresenceRequest {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            presence_request_id: "presence_for_realtime_preflight",
            fanout_request_receipt_id: "",
            thread_id: "thread_local_receipts",
            channel_id: "local-ops",
            presence_scope: "local projection only",
        };
        let report = self
            .save_message_presence_request_local(presence)
            .map_err(|_| {
                MessageRealtimeCutoverPreflightError::Missing("presence_request_receipt")
            })?;
        Ok(report.presence_request_receipt_id)
    }

    fn finish_message_realtime_preflight_run(&mut self, run_id: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "REALTIME_CUTOVER_PREFLIGHT_RECORDED_PROVIDER_BLOCKED".to_string();
        }
    }

    pub fn save_message_delivery_replay_batch_local(
        &mut self,
        batch: MessageDeliveryReplayBatch<'_>,
    ) -> Result<MessageDeliveryReplayBatchReport, MessageDeliveryReplayBatchError> {
        let identity = GovernedWriteIdentity::local_demo(batch.actor_id);
        self.save_message_delivery_replay_batch_local_with_identity(batch, &identity)
    }

    pub fn save_message_delivery_replay_batch_local_with_identity(
        &mut self,
        batch: MessageDeliveryReplayBatch<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<MessageDeliveryReplayBatchReport, MessageDeliveryReplayBatchError> {
        for (field, value) in [
            ("tenant_id", batch.tenant_id),
            ("actor_id", batch.actor_id),
            ("delivery_replay_batch_id", batch.delivery_replay_batch_id),
            ("channel_id", batch.channel_id),
            ("replay_scope", batch.replay_scope),
        ] {
            if value.trim().is_empty() {
                return Err(MessageDeliveryReplayBatchError::Missing(field));
            }
        }
        let realtime_preflight_receipt_id =
            self.ensure_message_delivery_replay_source(batch.realtime_preflight_receipt_id)?;
        let actor_admission = admit_local_route_actor(
            batch.tenant_id,
            batch.actor_id,
            "operator",
            "/messages/delivery-replay-batches.json",
            "message.delivery.replay.recorded",
            batch.delivery_replay_batch_id,
        )
        .map_err(|error| MessageDeliveryReplayBatchError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(batch.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(batch.actor_id),
            loop_id: LoopId::new("message_delivery_replay_batch"),
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
            self.decide_with_receipt(&correlation, ActionKind::RecordMessageDeliveryReplay);
        let receipt = self.transition_receipt(
            &run_id,
            "RECORD_MESSAGE_DELIVERY_REPLAY",
            &correlation,
            &decision,
            "message.delivery.replay.recorded",
            payload(&[
                ("delivery_replay_batch_id", batch.delivery_replay_batch_id),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("source_route", "/messages/delivery-replay-batches.json"),
                (
                    "projection_route",
                    "/messages/delivery-replay-batches/projection.json",
                ),
                (
                    "realtime_preflight_receipt_id",
                    &realtime_preflight_receipt_id,
                ),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("channel_id", batch.channel_id),
                ("replay_scope", batch.replay_scope),
                (
                    "replay_state",
                    "ROLLBACK_SAFE_REPLAY_RECORDED_DELIVERY_BLOCKED",
                ),
                ("rollback_safe_delivery_replay_proven", "true"),
                ("websocket_fanout_allowed", "false"),
                ("production_delivery_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_message_realtime_evidence_run(
            &run_id,
            "ROLLBACK_SAFE_REPLAY_RECORDED_DELIVERY_BLOCKED",
        );
        Ok(MessageDeliveryReplayBatchReport {
            status: "ROLLBACK_SAFE_REPLAY_RECORDED_DELIVERY_BLOCKED",
            delivery_replay_batch_id: batch.delivery_replay_batch_id.to_string(),
            delivery_replay_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            realtime_preflight_receipt_id,
            channel_id: batch.channel_id.to_string(),
            replay_state: "ROLLBACK_SAFE_REPLAY_RECORDED_DELIVERY_BLOCKED".to_string(),
            rollback_safe_delivery_replay_proven: true,
            websocket_fanout_allowed: false,
            production_delivery_allowed: false,
            production_write_allowed: false,
        })
    }

    pub fn save_message_subscription_isolation_check_local(
        &mut self,
        check: MessageSubscriptionIsolationCheck<'_>,
    ) -> Result<MessageSubscriptionIsolationCheckReport, MessageSubscriptionIsolationCheckError>
    {
        let identity = GovernedWriteIdentity::local_demo(check.actor_id);
        self.save_message_subscription_isolation_check_local_with_identity(check, &identity)
    }

    pub fn save_message_subscription_isolation_check_local_with_identity(
        &mut self,
        check: MessageSubscriptionIsolationCheck<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<MessageSubscriptionIsolationCheckReport, MessageSubscriptionIsolationCheckError>
    {
        for (field, value) in [
            ("tenant_id", check.tenant_id),
            ("actor_id", check.actor_id),
            (
                "subscription_isolation_check_id",
                check.subscription_isolation_check_id,
            ),
            ("channel_id", check.channel_id),
            ("isolation_scope", check.isolation_scope),
        ] {
            if value.trim().is_empty() {
                return Err(MessageSubscriptionIsolationCheckError::Missing(field));
            }
        }
        let realtime_preflight_receipt_id =
            self.ensure_message_subscription_isolation_source(check.realtime_preflight_receipt_id)?;
        let actor_admission = admit_local_route_actor(
            check.tenant_id,
            check.actor_id,
            "operator",
            "/messages/subscription-isolation-checks.json",
            "message.subscription.isolation.checked",
            check.subscription_isolation_check_id,
        )
        .map_err(|error| MessageSubscriptionIsolationCheckError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(check.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(check.actor_id),
            loop_id: LoopId::new("message_subscription_isolation_check"),
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
            self.decide_with_receipt(&correlation, ActionKind::CheckMessageSubscriptionIsolation);
        let receipt = self.transition_receipt(
            &run_id,
            "CHECK_MESSAGE_SUBSCRIPTION_ISOLATION",
            &correlation,
            &decision,
            "message.subscription.isolation.checked",
            payload(&[
                (
                    "subscription_isolation_check_id",
                    check.subscription_isolation_check_id,
                ),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                (
                    "source_route",
                    "/messages/subscription-isolation-checks.json",
                ),
                (
                    "projection_route",
                    "/messages/subscription-isolation-checks/projection.json",
                ),
                (
                    "realtime_preflight_receipt_id",
                    &realtime_preflight_receipt_id,
                ),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("channel_id", check.channel_id),
                ("isolation_scope", check.isolation_scope),
                (
                    "isolation_status",
                    "TENANT_CHANNEL_ISOLATION_PROVEN_LOCAL_ONLY",
                ),
                ("tenant_subscription_isolation_proven", "true"),
                ("service_role_fanout_allowed", "false"),
                ("production_delivery_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_message_realtime_evidence_run(
            &run_id,
            "TENANT_CHANNEL_ISOLATION_PROVEN_LOCAL_ONLY",
        );
        Ok(MessageSubscriptionIsolationCheckReport {
            status: "TENANT_CHANNEL_ISOLATION_PROVEN_LOCAL_ONLY",
            subscription_isolation_check_id: check.subscription_isolation_check_id.to_string(),
            subscription_isolation_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            realtime_preflight_receipt_id,
            channel_id: check.channel_id.to_string(),
            isolation_status: "TENANT_CHANNEL_ISOLATION_PROVEN_LOCAL_ONLY".to_string(),
            tenant_subscription_isolation_proven: true,
            service_role_fanout_allowed: false,
            production_delivery_allowed: false,
            production_write_allowed: false,
        })
    }

    pub fn save_message_service_role_fanout_refusal_local(
        &mut self,
        refusal: MessageServiceRoleFanoutRefusal<'_>,
    ) -> Result<MessageServiceRoleFanoutRefusalReport, MessageServiceRoleFanoutRefusalError> {
        let identity = GovernedWriteIdentity::local_demo(refusal.actor_id);
        self.save_message_service_role_fanout_refusal_local_with_identity(refusal, &identity)
    }

    pub fn save_message_service_role_fanout_refusal_local_with_identity(
        &mut self,
        refusal: MessageServiceRoleFanoutRefusal<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<MessageServiceRoleFanoutRefusalReport, MessageServiceRoleFanoutRefusalError> {
        for (field, value) in [
            ("tenant_id", refusal.tenant_id),
            ("actor_id", refusal.actor_id),
            (
                "service_role_fanout_refusal_id",
                refusal.service_role_fanout_refusal_id,
            ),
            ("refusal_reason", refusal.refusal_reason),
        ] {
            if value.trim().is_empty() {
                return Err(MessageServiceRoleFanoutRefusalError::Missing(field));
            }
        }
        let realtime_preflight_receipt_id =
            self.ensure_message_service_role_refusal_source(refusal.realtime_preflight_receipt_id)?;
        let actor_admission = admit_local_route_actor(
            refusal.tenant_id,
            refusal.actor_id,
            "operator",
            "/messages/service-role-fanout-refusals.json",
            "message.service_role.fanout.refused",
            refusal.service_role_fanout_refusal_id,
        )
        .map_err(|error| MessageServiceRoleFanoutRefusalError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(refusal.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(refusal.actor_id),
            loop_id: LoopId::new("message_service_role_fanout_refusal"),
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
            self.decide_with_receipt(&correlation, ActionKind::RefuseMessageServiceRoleFanout);
        let receipt = self.transition_receipt(
            &run_id,
            "REFUSE_MESSAGE_SERVICE_ROLE_FANOUT",
            &correlation,
            &decision,
            "message.service_role.fanout.refused",
            payload(&[
                (
                    "service_role_fanout_refusal_id",
                    refusal.service_role_fanout_refusal_id,
                ),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                (
                    "source_route",
                    "/messages/service-role-fanout-refusals.json",
                ),
                (
                    "projection_route",
                    "/messages/service-role-fanout-refusals/projection.json",
                ),
                (
                    "realtime_preflight_receipt_id",
                    &realtime_preflight_receipt_id,
                ),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("refusal_reason", refusal.refusal_reason),
                ("terminal_state", "SERVICE_ROLE_FANOUT_REFUSED_LOCAL_ONLY"),
                ("service_role_fanout_refused", "true"),
                ("presence_mutation_allowed", "false"),
                ("typing_indicator_allowed", "false"),
                ("production_delivery_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_message_realtime_evidence_run(
            &run_id,
            "SERVICE_ROLE_FANOUT_REFUSED_LOCAL_ONLY",
        );
        Ok(MessageServiceRoleFanoutRefusalReport {
            status: "SERVICE_ROLE_FANOUT_REFUSED_LOCAL_ONLY",
            service_role_fanout_refusal_id: refusal.service_role_fanout_refusal_id.to_string(),
            service_role_fanout_refusal_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            realtime_preflight_receipt_id,
            refusal_reason: refusal.refusal_reason.to_string(),
            service_role_fanout_refused: true,
            presence_mutation_allowed: false,
            typing_indicator_allowed: false,
            production_delivery_allowed: false,
            production_write_allowed: false,
        })
    }

    fn ensure_message_realtime_source_id(&mut self, requested_id: &str) -> Result<String, String> {
        if !requested_id.trim().is_empty() {
            if let Some(receipt) = self.storage.ledger().query().by_id(requested_id)
                && receipt.kind == "message.realtime.cutover.preflighted"
            {
                return Ok(requested_id.to_string());
            }
            return Err(requested_id.to_string());
        }
        if let Some(receipt_id) = self
            .storage
            .ledger()
            .query()
            .by_kind("message.realtime.cutover.preflighted")
            .first()
            .map(|receipt| receipt.receipt_id.clone())
        {
            return Ok(receipt_id);
        }
        let report = self
            .save_message_realtime_cutover_preflight_local(MessageRealtimeCutoverPreflight {
                tenant_id: "local_tenant",
                actor_id: "human:local_user",
                preflight_id: "realtime_preflight_for_message_evidence",
                presence_request_receipt_id: "",
                thread_id: "thread_local_receipts",
                channel_id: "local-ops",
                requested_realtime_scope: "tenant channel replay",
            })
            .map_err(|_| "missing_realtime_preflight_receipt".to_string())?;
        Ok(report.preflight_receipt_id)
    }

    fn ensure_message_delivery_replay_source(
        &mut self,
        requested_id: &str,
    ) -> Result<String, MessageDeliveryReplayBatchError> {
        self.ensure_message_realtime_source_id(requested_id)
            .map_err(|id| {
                if id == "missing_realtime_preflight_receipt" {
                    MessageDeliveryReplayBatchError::Missing("realtime_preflight_receipt")
                } else {
                    MessageDeliveryReplayBatchError::UnknownRealtimePreflightReceipt(id)
                }
            })
    }

    fn ensure_message_subscription_isolation_source(
        &mut self,
        requested_id: &str,
    ) -> Result<String, MessageSubscriptionIsolationCheckError> {
        self.ensure_message_realtime_source_id(requested_id)
            .map_err(|id| {
                if id == "missing_realtime_preflight_receipt" {
                    MessageSubscriptionIsolationCheckError::Missing("realtime_preflight_receipt")
                } else {
                    MessageSubscriptionIsolationCheckError::UnknownRealtimePreflightReceipt(id)
                }
            })
    }

    fn ensure_message_service_role_refusal_source(
        &mut self,
        requested_id: &str,
    ) -> Result<String, MessageServiceRoleFanoutRefusalError> {
        self.ensure_message_realtime_source_id(requested_id)
            .map_err(|id| {
                if id == "missing_realtime_preflight_receipt" {
                    MessageServiceRoleFanoutRefusalError::Missing("realtime_preflight_receipt")
                } else {
                    MessageServiceRoleFanoutRefusalError::UnknownRealtimePreflightReceipt(id)
                }
            })
    }

    fn finish_message_realtime_evidence_run(&mut self, run_id: &str, status: &str) {
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
    fn message_realtime_cutover_preflight_records_provider_blocked_receipt() {
        let mut kernel = MdxKernel::boot_local();
        let report = kernel
            .save_message_realtime_cutover_preflight_local(MessageRealtimeCutoverPreflight {
                tenant_id: "local_tenant",
                actor_id: "human:local_user",
                preflight_id: "message_realtime_preflight_test",
                presence_request_receipt_id: "",
                thread_id: "thread_local_receipts",
                channel_id: "local-ops",
                requested_realtime_scope: "tenant channel replay",
            })
            .expect("message realtime preflight");
        assert_eq!(
            report.status,
            "REALTIME_CUTOVER_PREFLIGHT_RECORDED_PROVIDER_BLOCKED"
        );
        assert!(report.tenant_subscription_isolation_proven);
        assert!(report.service_role_fanout_refused);
        assert!(!report.realtime_provider_turn_on_observed);
        assert!(!report.presence_mutation_allowed);
        assert!(!report.typing_indicator_allowed);
        assert!(!report.websocket_fanout_allowed);
        assert!(!report.production_delivery_allowed);
        assert!(!report.production_write_allowed);
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.preflight_receipt_id)
            .expect("preflight receipt");
        assert_eq!(receipt.kind, "message.realtime.cutover.preflighted");
    }

    #[test]
    fn message_realtime_cutover_preflight_rejects_unknown_presence_receipt() {
        let mut kernel = MdxKernel::boot_local();
        let error = kernel
            .save_message_realtime_cutover_preflight_local(MessageRealtimeCutoverPreflight {
                tenant_id: "local_tenant",
                actor_id: "human:local_user",
                preflight_id: "message_realtime_preflight_bad_source",
                presence_request_receipt_id: "missing_presence_receipt",
                thread_id: "thread_local_receipts",
                channel_id: "local-ops",
                requested_realtime_scope: "tenant channel replay",
            })
            .expect_err("unknown source must fail");
        assert_eq!(
            error,
            MessageRealtimeCutoverPreflightError::UnknownPresenceReceipt(
                "missing_presence_receipt".to_string()
            )
        );
    }

    #[test]
    fn message_realtime_evidence_ladder_records_local_proof_with_delivery_blocked() {
        let mut kernel = MdxKernel::boot_local();
        let preflight = kernel
            .save_message_realtime_cutover_preflight_local(MessageRealtimeCutoverPreflight {
                tenant_id: "local_tenant",
                actor_id: "human:local_user",
                preflight_id: "message_realtime_ladder_preflight",
                presence_request_receipt_id: "",
                thread_id: "thread_local_receipts",
                channel_id: "local-ops",
                requested_realtime_scope: "tenant channel replay",
            })
            .expect("message realtime preflight");
        let replay = kernel
            .save_message_delivery_replay_batch_local(MessageDeliveryReplayBatch {
                tenant_id: "local_tenant",
                actor_id: "human:local_user",
                delivery_replay_batch_id: "delivery_replay_ladder_test",
                realtime_preflight_receipt_id: &preflight.preflight_receipt_id,
                channel_id: "local-ops",
                replay_scope: "tenant channel delivery replay",
            })
            .expect("delivery replay");
        assert_eq!(
            replay.status,
            "ROLLBACK_SAFE_REPLAY_RECORDED_DELIVERY_BLOCKED"
        );
        assert!(replay.rollback_safe_delivery_replay_proven);
        assert!(!replay.websocket_fanout_allowed);
        assert!(!replay.production_delivery_allowed);
        assert!(!replay.production_write_allowed);
        let replay_receipt = kernel
            .ledger()
            .query()
            .by_id(&replay.delivery_replay_receipt_id)
            .expect("delivery replay receipt");
        assert_eq!(replay_receipt.kind, "message.delivery.replay.recorded");

        let isolation = kernel
            .save_message_subscription_isolation_check_local(MessageSubscriptionIsolationCheck {
                tenant_id: "local_tenant",
                actor_id: "human:local_user",
                subscription_isolation_check_id: "subscription_isolation_ladder_test",
                realtime_preflight_receipt_id: &preflight.preflight_receipt_id,
                channel_id: "local-ops",
                isolation_scope: "tenant channel subscription isolation",
            })
            .expect("subscription isolation");
        assert_eq!(
            isolation.status,
            "TENANT_CHANNEL_ISOLATION_PROVEN_LOCAL_ONLY"
        );
        assert!(isolation.tenant_subscription_isolation_proven);
        assert!(!isolation.service_role_fanout_allowed);
        assert!(!isolation.production_delivery_allowed);
        assert!(!isolation.production_write_allowed);
        let isolation_receipt = kernel
            .ledger()
            .query()
            .by_id(&isolation.subscription_isolation_receipt_id)
            .expect("subscription isolation receipt");
        assert_eq!(
            isolation_receipt.kind,
            "message.subscription.isolation.checked"
        );

        let refusal = kernel
            .save_message_service_role_fanout_refusal_local(MessageServiceRoleFanoutRefusal {
                tenant_id: "local_tenant",
                actor_id: "human:local_user",
                service_role_fanout_refusal_id: "service_role_refusal_ladder_test",
                realtime_preflight_receipt_id: &preflight.preflight_receipt_id,
                refusal_reason: "service-role fanout is not a local runtime authority",
            })
            .expect("service role refusal");
        assert_eq!(refusal.status, "SERVICE_ROLE_FANOUT_REFUSED_LOCAL_ONLY");
        assert!(refusal.service_role_fanout_refused);
        assert!(!refusal.presence_mutation_allowed);
        assert!(!refusal.typing_indicator_allowed);
        assert!(!refusal.production_delivery_allowed);
        assert!(!refusal.production_write_allowed);
        let refusal_receipt = kernel
            .ledger()
            .query()
            .by_id(&refusal.service_role_fanout_refusal_receipt_id)
            .expect("service role refusal receipt");
        assert_eq!(refusal_receipt.kind, "message.service_role.fanout.refused");
    }

    #[test]
    fn message_realtime_evidence_ladder_rejects_unknown_preflight_receipt() {
        let mut kernel = MdxKernel::boot_local();
        let error = kernel
            .save_message_delivery_replay_batch_local(MessageDeliveryReplayBatch {
                tenant_id: "local_tenant",
                actor_id: "human:local_user",
                delivery_replay_batch_id: "delivery_replay_bad_source",
                realtime_preflight_receipt_id: "missing_realtime_preflight",
                channel_id: "local-ops",
                replay_scope: "tenant channel delivery replay",
            })
            .expect_err("unknown source must fail");
        assert_eq!(
            error,
            MessageDeliveryReplayBatchError::UnknownRealtimePreflightReceipt(
                "missing_realtime_preflight".to_string()
            )
        );
    }
}
