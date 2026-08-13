use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageRelayObservation<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub observation_id: &'a str,
    pub realtime_preflight_receipt_id: &'a str,
    pub relay_driver: &'a str,
    pub delivery_count: usize,
    pub p99_post_start_to_delivery_ms: u32,
    pub p99_post_finish_to_delivery_ms: u32,
    pub hot_cache_replay_status: &'a str,
    pub durable_replay_status: &'a str,
    pub typing_signal_status: &'a str,
    pub relay_health_status: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageRelayObservationReport {
    pub status: &'static str,
    pub observation_id: String,
    pub relay_observation_receipt_id: String,
    pub policy_decision_id: String,
    pub realtime_preflight_receipt_id: String,
    pub relay_driver: String,
    pub delivery_count: usize,
    pub p99_post_start_to_delivery_ms: u32,
    pub p99_post_finish_to_delivery_ms: u32,
    pub p99_budget_ms: u32,
    pub hot_cache_replay_status: String,
    pub durable_replay_status: String,
    pub typing_signal_status: String,
    pub relay_health_status: String,
    pub local_websocket_fanout_allowed: bool,
    pub production_delivery_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageRelayObservationError {
    Missing(&'static str),
    UnknownRealtimePreflightReceipt(String),
    InvalidDriver(String),
    EmptyDeliveryCount,
    BudgetExceeded { start_ms: u32, finish_ms: u32 },
    MissingProofStatus { field: &'static str, value: String },
    ActorAdmission(String),
}

impl MessageRelayObservationError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("message relay observation missing {field}"),
            Self::UnknownRealtimePreflightReceipt(id) => {
                format!("message relay observation source receipt {id} is unknown")
            }
            Self::InvalidDriver(driver) => {
                format!("message relay observation driver {driver} is not valkey")
            }
            Self::EmptyDeliveryCount => {
                "message relay observation requires at least one delivered envelope".to_string()
            }
            Self::BudgetExceeded {
                start_ms,
                finish_ms,
            } => format!(
                "message relay observation p99 budget exceeded start={start_ms} finish={finish_ms}"
            ),
            Self::MissingProofStatus { field, value } => {
                format!("message relay observation {field} did not prove required status {value}")
            }
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_message_relay_observation_local(
        &mut self,
        observation: MessageRelayObservation<'_>,
    ) -> Result<MessageRelayObservationReport, MessageRelayObservationError> {
        let identity = GovernedWriteIdentity::local_demo(observation.actor_id);
        self.save_message_relay_observation_local_with_identity(observation, &identity)
    }

    pub fn save_message_relay_observation_local_with_identity(
        &mut self,
        observation: MessageRelayObservation<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<MessageRelayObservationReport, MessageRelayObservationError> {
        for (field, value) in [
            ("tenant_id", observation.tenant_id),
            ("actor_id", observation.actor_id),
            ("observation_id", observation.observation_id),
            ("relay_driver", observation.relay_driver),
            (
                "hot_cache_replay_status",
                observation.hot_cache_replay_status,
            ),
            ("durable_replay_status", observation.durable_replay_status),
            ("typing_signal_status", observation.typing_signal_status),
            ("relay_health_status", observation.relay_health_status),
        ] {
            if value.trim().is_empty() {
                return Err(MessageRelayObservationError::Missing(field));
            }
        }
        if observation.relay_driver != "valkey" {
            return Err(MessageRelayObservationError::InvalidDriver(
                observation.relay_driver.to_string(),
            ));
        }
        if observation.delivery_count == 0 {
            return Err(MessageRelayObservationError::EmptyDeliveryCount);
        }
        if observation.p99_post_start_to_delivery_ms > 20
            || observation.p99_post_finish_to_delivery_ms > 20
        {
            return Err(MessageRelayObservationError::BudgetExceeded {
                start_ms: observation.p99_post_start_to_delivery_ms,
                finish_ms: observation.p99_post_finish_to_delivery_ms,
            });
        }
        require_relay_status(
            "hot_cache_replay_status",
            observation.hot_cache_replay_status,
            "REPLAYED_FROM_RELAY_HOT_CACHE",
        )?;
        require_relay_status(
            "durable_replay_status",
            observation.durable_replay_status,
            "REPLAYED_FROM_DURABLE_RELAY_LOG_AFTER_RESTART",
        )?;
        require_relay_status(
            "typing_signal_status",
            observation.typing_signal_status,
            "TYPING_SIGNAL_PROVEN",
        )?;
        require_relay_status(
            "relay_health_status",
            observation.relay_health_status,
            "LIVE-LOCAL-RELAY",
        )?;
        let realtime_preflight_receipt_id = self
            .ensure_message_relay_observation_source(observation.realtime_preflight_receipt_id)?;
        let actor_admission = admit_local_route_actor(
            observation.tenant_id,
            observation.actor_id,
            "operator",
            "/messages/relay-observations.json",
            "message.relay.observed",
            observation.observation_id,
        )
        .map_err(|error| MessageRelayObservationError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(observation.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(observation.actor_id),
            loop_id: LoopId::new("message_relay_observation"),
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
            self.decide_with_receipt(&correlation, ActionKind::RecordMessageRelayObservation);
        let receipt = self.transition_receipt(
            &run_id,
            "RECORD_MESSAGE_RELAY_OBSERVATION",
            &correlation,
            &decision,
            "message.relay.observed",
            payload(&[
                ("observation_id", observation.observation_id),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("source_route", "/messages/relay-observations.json"),
                (
                    "projection_route",
                    "/messages/relay-observations/projection.json",
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
                ("relay_driver", observation.relay_driver),
                ("delivery_count", &observation.delivery_count.to_string()),
                (
                    "p99_post_start_to_delivery_ms",
                    &observation.p99_post_start_to_delivery_ms.to_string(),
                ),
                (
                    "p99_post_finish_to_delivery_ms",
                    &observation.p99_post_finish_to_delivery_ms.to_string(),
                ),
                ("p99_budget_ms", "20"),
                (
                    "hot_cache_replay_status",
                    observation.hot_cache_replay_status,
                ),
                ("durable_replay_status", observation.durable_replay_status),
                ("typing_signal_status", observation.typing_signal_status),
                ("relay_health_status", observation.relay_health_status),
                (
                    "terminal_state",
                    "MESSAGE_RELAY_OBSERVED_LOCAL_PRODUCTION_BLOCKED",
                ),
                ("local_websocket_fanout_allowed", "true"),
                ("production_delivery_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_message_relay_observation_run(&run_id);
        Ok(MessageRelayObservationReport {
            status: "MESSAGE_RELAY_OBSERVED_LOCAL_PRODUCTION_BLOCKED",
            observation_id: observation.observation_id.to_string(),
            relay_observation_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            realtime_preflight_receipt_id,
            relay_driver: observation.relay_driver.to_string(),
            delivery_count: observation.delivery_count,
            p99_post_start_to_delivery_ms: observation.p99_post_start_to_delivery_ms,
            p99_post_finish_to_delivery_ms: observation.p99_post_finish_to_delivery_ms,
            p99_budget_ms: 20,
            hot_cache_replay_status: observation.hot_cache_replay_status.to_string(),
            durable_replay_status: observation.durable_replay_status.to_string(),
            typing_signal_status: observation.typing_signal_status.to_string(),
            relay_health_status: observation.relay_health_status.to_string(),
            local_websocket_fanout_allowed: true,
            production_delivery_allowed: false,
            production_write_allowed: false,
        })
    }

    fn ensure_message_relay_observation_source(
        &mut self,
        requested_id: &str,
    ) -> Result<String, MessageRelayObservationError> {
        if !requested_id.trim().is_empty() {
            return self
                .storage
                .ledger()
                .query()
                .by_id(requested_id)
                .filter(|receipt| receipt.kind == "message.realtime.cutover.preflighted")
                .map(|receipt| receipt.receipt_id.clone())
                .ok_or_else(|| {
                    MessageRelayObservationError::UnknownRealtimePreflightReceipt(
                        requested_id.to_string(),
                    )
                });
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
        self.save_message_realtime_cutover_preflight_local(MessageRealtimeCutoverPreflight {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            preflight_id: "message_relay_observation_preflight",
            presence_request_receipt_id: "",
            thread_id: "thread_local_receipts",
            channel_id: "local-ops",
            requested_realtime_scope: "relay observation evidence",
        })
        .map(|report| report.preflight_receipt_id)
        .map_err(|error| MessageRelayObservationError::ActorAdmission(error.message()))
    }

    fn finish_message_relay_observation_run(&mut self, run_id: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "MESSAGE_RELAY_OBSERVED_LOCAL_PRODUCTION_BLOCKED".to_string();
        }
    }
}

fn require_relay_status(
    field: &'static str,
    value: &str,
    expected: &'static str,
) -> Result<(), MessageRelayObservationError> {
    if value == expected {
        Ok(())
    } else {
        Err(MessageRelayObservationError::MissingProofStatus {
            field,
            value: expected.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_relay_observation_records_budgeted_live_local_proof() {
        let mut kernel = MdxKernel::boot_local();
        let report = kernel
            .save_message_relay_observation_local(MessageRelayObservation {
                tenant_id: "local_tenant",
                actor_id: "human:local_user",
                observation_id: "relay_observation_test",
                realtime_preflight_receipt_id: "",
                relay_driver: "valkey",
                delivery_count: 24,
                p99_post_start_to_delivery_ms: 12,
                p99_post_finish_to_delivery_ms: 2,
                hot_cache_replay_status: "REPLAYED_FROM_RELAY_HOT_CACHE",
                durable_replay_status: "REPLAYED_FROM_DURABLE_RELAY_LOG_AFTER_RESTART",
                typing_signal_status: "TYPING_SIGNAL_PROVEN",
                relay_health_status: "LIVE-LOCAL-RELAY",
            })
            .expect("relay observation");
        assert_eq!(
            report.status,
            "MESSAGE_RELAY_OBSERVED_LOCAL_PRODUCTION_BLOCKED"
        );
        assert!(report.local_websocket_fanout_allowed);
        assert!(!report.production_delivery_allowed);
        assert!(
            kernel
                .ledger()
                .entries()
                .iter()
                .any(|receipt| receipt.kind == "message.relay.observed")
        );
    }

    #[test]
    fn message_relay_observation_rejects_budget_or_missing_proof() {
        let mut kernel = MdxKernel::boot_local();
        let error = kernel
            .save_message_relay_observation_local(MessageRelayObservation {
                tenant_id: "local_tenant",
                actor_id: "human:local_user",
                observation_id: "relay_observation_budget_failure",
                realtime_preflight_receipt_id: "",
                relay_driver: "valkey",
                delivery_count: 24,
                p99_post_start_to_delivery_ms: 21,
                p99_post_finish_to_delivery_ms: 2,
                hot_cache_replay_status: "REPLAYED_FROM_RELAY_HOT_CACHE",
                durable_replay_status: "REPLAYED_FROM_DURABLE_RELAY_LOG_AFTER_RESTART",
                typing_signal_status: "TYPING_SIGNAL_PROVEN",
                relay_health_status: "LIVE-LOCAL-RELAY",
            })
            .expect_err("budget must fail");
        assert!(matches!(
            error,
            MessageRelayObservationError::BudgetExceeded { .. }
        ));
    }
}
