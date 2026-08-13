use crate::*;

pub const MESSAGE_THREAD_MESSAGE_POSTED_KINDS: &[&str] = &[
    "message.human.posted",
    "message.agent.posted",
    "message.worker.posted",
    "message.pipeline.posted",
    "message.system.posted",
];

pub const MESSAGE_THREAD_MESSAGE_CONTENT_TYPES: &[&str] =
    &["text", "card", "action_request", "action_response"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageThreadMessage<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub thread_id: &'a str,
    pub channel_id: &'a str,
    pub message_id: &'a str,
    pub content_type: &'a str,
    pub body: &'a str,
    pub content_payload: &'a str,
    pub source_receipt_id: &'a str,
    pub outbox_topic: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageThreadMessageReport {
    pub status: &'static str,
    pub thread_id: String,
    pub channel_id: String,
    pub message_id: String,
    pub message_receipt_id: String,
    pub message_receipt_kind: &'static str,
    pub policy_decision_id: String,
    pub actor_type: &'static str,
    pub actor_role: &'static str,
    pub content_type: &'static str,
    pub content_payload: String,
    pub source_receipt_id: String,
    pub source_receipt_kind: String,
    pub trusted_context_citation: bool,
    pub trusted_context_source_id: String,
    pub trusted_context_receipt_id: String,
    pub trusted_context_projection_route: &'static str,
    pub outbox_topic: String,
    pub sequence: usize,
    pub created_at: String,
    pub llm_allowed_on_hot_path: bool,
    pub realtime_fanout_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageThreadMessageError {
    Missing(&'static str),
    UnknownContentType(String),
    UnknownSourceReceipt(String),
    InactiveTrustedContextSource(String),
    ActorAdmission(String),
}

impl MessageThreadMessageError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("message thread message missing {field}"),
            Self::UnknownContentType(content_type) => format!(
                "message thread message content_type must be one of {}; got {content_type}",
                MESSAGE_THREAD_MESSAGE_CONTENT_TYPES.join(", ")
            ),
            Self::UnknownSourceReceipt(id) => {
                format!("message thread message source receipt {id} is unknown")
            }
            Self::InactiveTrustedContextSource(id) => {
                format!("message thread message trusted Context source receipt {id} is inactive")
            }
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_message_thread_message_local(
        &mut self,
        request: MessageThreadMessage<'_>,
    ) -> Result<MessageThreadMessageReport, MessageThreadMessageError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_message_thread_message_local_with_identity(request, &identity)
    }

    pub fn save_message_thread_message_local_with_identity(
        &mut self,
        request: MessageThreadMessage<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<MessageThreadMessageReport, MessageThreadMessageError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("thread_id", request.thread_id),
            ("channel_id", request.channel_id),
            ("message_id", request.message_id),
            ("content_type", request.content_type),
            ("body", request.body),
            ("content_payload", request.content_payload),
            ("source_receipt_id", request.source_receipt_id),
            ("outbox_topic", request.outbox_topic),
        ] {
            if value.trim().is_empty() {
                return Err(MessageThreadMessageError::Missing(field));
            }
        }
        let content_type = normalize_message_content_type(request.content_type)?;
        let source_receipt = self
            .storage
            .ledger()
            .query()
            .by_id(request.source_receipt_id)
            .cloned()
            .ok_or_else(|| {
                MessageThreadMessageError::UnknownSourceReceipt(
                    request.source_receipt_id.to_string(),
                )
            })?;
        let trusted_context_source = if source_receipt.kind
            == "pages.context_source.trust_decision.recorded"
        {
            Some(
                trusted_context_sources_from_receipts(self.storage.ledger().entries())
                    .into_iter()
                    .find(|source| source.trust_decision_receipt_id == request.source_receipt_id)
                    .ok_or_else(|| {
                        MessageThreadMessageError::InactiveTrustedContextSource(
                            request.source_receipt_id.to_string(),
                        )
                    })?,
            )
        } else {
            None
        };
        let trusted_context_citation = trusted_context_source.is_some();
        let trusted_context_source_id = trusted_context_source
            .as_ref()
            .map(|source| source.source_id.clone())
            .unwrap_or_default();
        let trusted_context_receipt_id = trusted_context_source
            .as_ref()
            .map(|source| source.trust_decision_receipt_id.clone())
            .unwrap_or_default();
        let actor_kind = message_actor_kind(request.actor_id);
        let actor_role = message_actor_role(actor_kind);
        let message_receipt_kind = message_receipt_kind_for_actor_kind(actor_kind);
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            actor_role,
            "/messages/thread-messages.json",
            message_receipt_kind,
            request.message_id,
        )
        .map_err(|error| MessageThreadMessageError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("message_thread_message"),
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
        let decision = self.decide_with_receipt(&correlation, ActionKind::PostMessageThreadMessage);
        let actor_display = request
            .actor_id
            .split(':')
            .nth(1)
            .unwrap_or("local user")
            .replace('_', " ");
        let sequence = message_thread_message_sequence(self.storage.ledger().entries()) + 1;
        let created_at = local_created_at(sequence);
        let message_receipt = self.transition_receipt(
            &run_id,
            "APPEND_MESSAGE_EVENT",
            &correlation,
            &decision,
            message_receipt_kind,
            payload(&[
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("thread_id", request.thread_id),
                ("channel_id", request.channel_id),
                ("message_id", request.message_id),
                ("message_receipt_kind", message_receipt_kind),
                ("actor_type", actor_kind),
                ("actor_display_name", &actor_display),
                ("actor_role", actor_role),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("content_type", content_type),
                ("body", request.body),
                ("content_payload", request.content_payload),
                ("source_receipt_id", request.source_receipt_id),
                ("source_receipt_kind", &source_receipt.kind),
                (
                    "trusted_context_citation",
                    bool_string(trusted_context_citation),
                ),
                ("trusted_context_source_id", &trusted_context_source_id),
                ("trusted_context_receipt_id", &trusted_context_receipt_id),
                (
                    "trusted_context_projection_route",
                    "/pages/context-sources/projection.json",
                ),
                ("outbox_topic", request.outbox_topic),
                ("sequence", &sequence.to_string()),
                ("created_at", &created_at),
                ("terminal_state", "MESSAGE_APPENDED_FANOUT_BLOCKED"),
                ("llm_allowed_on_hot_path", "false"),
                ("realtime_fanout_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_message_thread_message_run(&run_id);
        // The message reports the receipt's own trusted timestamp, not the
        // deterministic placeholder: ordering and "last synced" now read the
        // real signed time bound into the receipt hash.
        let recorded_at = message_receipt.receipt_timestamp.clone();
        Ok(MessageThreadMessageReport {
            status: "MESSAGE_APPENDED_FANOUT_BLOCKED",
            thread_id: request.thread_id.to_string(),
            channel_id: request.channel_id.to_string(),
            message_id: request.message_id.to_string(),
            message_receipt_id: message_receipt.receipt_id,
            message_receipt_kind,
            policy_decision_id: decision.policy_decision_id,
            actor_type: actor_kind,
            actor_role,
            content_type,
            content_payload: request.content_payload.to_string(),
            source_receipt_id: request.source_receipt_id.to_string(),
            source_receipt_kind: source_receipt.kind,
            trusted_context_citation,
            trusted_context_source_id,
            trusted_context_receipt_id,
            trusted_context_projection_route: "/pages/context-sources/projection.json",
            outbox_topic: request.outbox_topic.to_string(),
            sequence,
            created_at: recorded_at,
            llm_allowed_on_hot_path: false,
            realtime_fanout_allowed: false,
            production_write_allowed: false,
        })
    }

    fn finish_message_thread_message_run(&mut self, run_id: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "MESSAGE_APPENDED_FANOUT_BLOCKED".to_string();
        }
    }
}

pub fn is_message_thread_message_receipt_kind(kind: &str) -> bool {
    MESSAGE_THREAD_MESSAGE_POSTED_KINDS.contains(&kind)
}

pub fn message_receipt_kind_for_actor_id(actor_id: &str) -> &'static str {
    message_receipt_kind_for_actor_kind(message_actor_kind(actor_id))
}

fn message_receipt_kind_for_actor_kind(actor_kind: &'static str) -> &'static str {
    match actor_kind {
        "agent" => "message.agent.posted",
        "worker" => "message.worker.posted",
        "pipeline" => "message.pipeline.posted",
        "system" => "message.system.posted",
        _ => "message.human.posted",
    }
}

fn message_actor_kind(actor_id: &str) -> &'static str {
    match actor_id.split(':').next().unwrap_or("human") {
        "agent" => "agent",
        "pipeline" => "pipeline",
        "system" => "system",
        "worker" => "worker",
        _ => "human",
    }
}

fn message_actor_role(actor_kind: &str) -> &'static str {
    match actor_kind {
        "agent" => "agent",
        "worker" => "worker",
        "pipeline" => "pipeline",
        "system" => "system",
        _ => "operator",
    }
}

fn normalize_message_content_type(
    content_type: &str,
) -> Result<&'static str, MessageThreadMessageError> {
    let normalized = content_type.trim().to_ascii_lowercase();
    MESSAGE_THREAD_MESSAGE_CONTENT_TYPES
        .iter()
        .copied()
        .find(|allowed| *allowed == normalized)
        .ok_or(MessageThreadMessageError::UnknownContentType(normalized))
}

fn message_thread_message_sequence(receipts: &[Receipt]) -> usize {
    receipts
        .iter()
        .filter(|receipt| is_message_thread_message_receipt_kind(&receipt.kind))
        .count()
}

fn local_created_at(sequence: usize) -> String {
    format!("2026-01-01T00:00:{:02}Z", sequence % 60)
}

fn bool_string(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}
