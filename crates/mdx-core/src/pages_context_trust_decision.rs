use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagesContextSourceTrustDecision<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub trust_decision_id: &'a str,
    pub source_id: &'a str,
    pub decision_outcome: &'a str,
    pub decision_note: &'a str,
    pub source_route: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagesContextSourceTrustDecisionReport {
    pub status: &'static str,
    pub trust_decision_id: String,
    pub trust_decision_receipt_id: String,
    pub policy_decision_id: String,
    pub source_id: String,
    pub artifact_id: String,
    pub source_admission_receipt_id: String,
    pub decision_outcome: String,
    pub decision_note: String,
    pub trusted_context_state: String,
    pub visible_trust_state: String,
    pub review_queue_state: String,
    pub allowed_consumers: String,
    pub created_at: String,
    pub human_decision_recorded: bool,
    pub raw_private_content_recorded: bool,
    pub frontier_raw_content_allowed: bool,
    pub memory_write_performed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedContextSourceSummary {
    pub source_id: String,
    pub artifact_id: String,
    pub title: String,
    pub source_kind: String,
    pub intake_surface: String,
    pub privacy_class: String,
    pub source_artifact_hash: String,
    pub source_admission_receipt_id: String,
    pub trust_decision_receipt_id: String,
    pub storage_receipt_id: String,
    pub parse_receipt_id: String,
    pub grounding_receipt_id: String,
    pub company_context_source: String,
    pub allowed_consumers: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PagesContextSourceTrustDecisionError {
    Missing(&'static str),
    UnknownSource(String),
    PersonalConnectorSource(String),
    DeletedSource(String),
    InvalidDecisionOutcome(String),
    ActorAdmission(String),
}

impl PagesContextSourceTrustDecisionError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("pages context trust decision missing {field}"),
            Self::UnknownSource(source_id) => {
                format!("pages context source {source_id} is unknown")
            }
            Self::PersonalConnectorSource(source_id) => {
                format!("pages context connector source {source_id} is personal scoped")
            }
            Self::DeletedSource(source_id) => {
                format!("pages context source {source_id} has been deleted")
            }
            Self::InvalidDecisionOutcome(outcome) => {
                format!("pages context trust decision outcome {outcome} is invalid")
            }
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_pages_context_source_trust_decision_local(
        &mut self,
        request: PagesContextSourceTrustDecision<'_>,
    ) -> Result<PagesContextSourceTrustDecisionReport, PagesContextSourceTrustDecisionError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_pages_context_source_trust_decision_local_with_identity(request, &identity)
    }

    pub fn save_pages_context_source_trust_decision_local_with_identity(
        &mut self,
        request: PagesContextSourceTrustDecision<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<PagesContextSourceTrustDecisionReport, PagesContextSourceTrustDecisionError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("trust_decision_id", request.trust_decision_id),
            ("source_id", request.source_id),
            ("decision_outcome", request.decision_outcome),
            ("decision_note", request.decision_note),
            ("source_route", request.source_route),
        ] {
            if value.trim().is_empty() {
                return Err(PagesContextSourceTrustDecisionError::Missing(field));
            }
        }

        let source =
            pages_context_trust_source(self.storage.ledger().entries(), request.source_id)?;

        let (
            normalized_outcome,
            status,
            trusted_context_state,
            visible_trust_state,
            review_queue_state,
            allowed_consumers,
        ) = match request.decision_outcome {
            "trusted_for_ai" | "trust_for_ai" => (
                "trusted_for_ai",
                "PAGES_CONTEXT_SOURCE_TRUSTED_FOR_AI",
                "trusted_for_ai",
                "Trusted for AI",
                "trusted",
                "twin_current_session,pages_draft_review,twin_company_context,forge_context_packet,ctx_recall_packet",
            ),
            "revoke" | "revoked" | "revoke_trusted_ai" | "keep_private" | "private" => (
                "revoke_trusted_ai",
                "PAGES_CONTEXT_SOURCE_TRUST_REVOKED",
                "read_here",
                "Private",
                "needs_review",
                "twin_current_session,pages_draft_review",
            ),
            other => {
                return Err(
                    PagesContextSourceTrustDecisionError::InvalidDecisionOutcome(other.to_string()),
                );
            }
        };

        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "operator",
            request.source_route,
            "pages.context_source.trust_decision.recorded",
            request.trust_decision_id,
        )
        .map_err(|error| PagesContextSourceTrustDecisionError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("pages_context_source_trust_decision"),
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
        let decision = self.decide_with_receipt(
            &correlation,
            ActionKind::RecordPagesContextSourceTrustDecision,
        );
        let created_at = local_context_trust_created_at(
            self.storage
                .ledger()
                .query()
                .by_kind("pages.context_source.trust_decision.recorded")
                .len()
                + 1,
        );
        let trust_receipt = self.transition_receipt(
            &run_id,
            "RECORD_PAGES_CONTEXT_SOURCE_TRUST_DECISION",
            &correlation,
            &decision,
            "pages.context_source.trust_decision.recorded",
            payload(&[
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("trust_decision_id", request.trust_decision_id),
                ("source_route", request.source_route),
                ("projection_route", "/pages/context-sources/projection.json"),
                ("source_id", &source.source_id),
                ("artifact_id", &source.artifact_id),
                ("source_kind", &source.source_kind),
                ("intake_surface", &source.intake_surface),
                (
                    "source_admission_receipt_id",
                    &source.source_admission_receipt_id,
                ),
                ("source_artifact_hash", &source.source_artifact_hash),
                ("decision_outcome", normalized_outcome),
                ("decision_note", request.decision_note),
                ("trusted_context_state", trusted_context_state),
                ("visible_trust_state", visible_trust_state),
                ("review_queue_state", review_queue_state),
                ("allowed_consumers", allowed_consumers),
                ("created_at", &created_at),
                ("created_at_authority", "local_placeholder_not_trusted_time"),
                ("freshness_authority", "deferred_to_trusted_time_arc"),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("terminal_state", status),
                ("human_decision_recorded", "true"),
                ("no_second_source_model", "true"),
                ("raw_private_content_recorded", "false"),
                ("provider_call_allowed", "false"),
                ("frontier_raw_content_allowed", "false"),
                ("memory_write_performed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_pages_context_source_trust_decision_run(&run_id, status);
        Ok(PagesContextSourceTrustDecisionReport {
            status,
            trust_decision_id: request.trust_decision_id.to_string(),
            trust_decision_receipt_id: trust_receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            source_id: source.source_id,
            artifact_id: source.artifact_id,
            source_admission_receipt_id: source.source_admission_receipt_id,
            decision_outcome: normalized_outcome.to_string(),
            decision_note: request.decision_note.to_string(),
            trusted_context_state: trusted_context_state.to_string(),
            visible_trust_state: visible_trust_state.to_string(),
            review_queue_state: review_queue_state.to_string(),
            allowed_consumers: allowed_consumers.to_string(),
            created_at,
            human_decision_recorded: true,
            raw_private_content_recorded: false,
            frontier_raw_content_allowed: false,
            memory_write_performed: false,
            production_write_allowed: false,
        })
    }

    fn finish_pages_context_source_trust_decision_run(&mut self, run_id: &str, status: &str) {
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

fn local_context_trust_created_at(sequence: usize) -> String {
    // Display-only placeholder until the trusted-time arc owns Context freshness.
    // Do not use this mod-60 value for review cadence or stale decisions.
    format!("2026-01-01T00:01:{:02}Z", sequence % 60)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PagesContextTrustSource {
    source_id: String,
    artifact_id: String,
    source_kind: String,
    intake_surface: String,
    source_admission_receipt_id: String,
    source_artifact_hash: String,
}

fn pages_context_trust_source(
    entries: &[Receipt],
    source_id: &str,
) -> Result<PagesContextTrustSource, PagesContextSourceTrustDecisionError> {
    if entries.iter().rev().any(|receipt| {
        receipt.kind == "twin.artifact.delete_requested"
            && context_payload_value(receipt, "artifact_id") == source_id
    }) {
        return Err(PagesContextSourceTrustDecisionError::DeletedSource(
            source_id.to_string(),
        ));
    }

    if let Some(admission) = entries.iter().find(|receipt| {
        receipt.kind == "twin.artifact.admitted"
            && context_payload_value(receipt, "artifact_id") == source_id
    }) {
        return Ok(PagesContextTrustSource {
            source_id: source_id.to_string(),
            artifact_id: source_id.to_string(),
            source_kind: context_payload_value(admission, "source_kind"),
            intake_surface: context_payload_value(admission, "source_intake_surface"),
            source_admission_receipt_id: admission.receipt_id.clone(),
            source_artifact_hash: context_payload_value(admission, "source_artifact_hash"),
        });
    }

    if let Some(connector) = entries.iter().find(|receipt| {
        receipt.kind == "connector.item.ingested" && receipt.receipt_id == source_id
    }) {
        if connector_item_deleted(entries, source_id) {
            return Err(PagesContextSourceTrustDecisionError::DeletedSource(
                source_id.to_string(),
            ));
        }
        if connector_payload_scope(connector) == "personal" {
            return Err(
                PagesContextSourceTrustDecisionError::PersonalConnectorSource(
                    source_id.to_string(),
                ),
            );
        }
        return Ok(PagesContextTrustSource {
            source_id: connector.receipt_id.clone(),
            artifact_id: connector.receipt_id.clone(),
            source_kind: context_payload_value(connector, "source_kind"),
            intake_surface: "connector".to_string(),
            source_admission_receipt_id: connector.receipt_id.clone(),
            source_artifact_hash: connector.hash.clone(),
        });
    }

    Err(PagesContextSourceTrustDecisionError::UnknownSource(
        source_id.to_string(),
    ))
}

pub fn trusted_context_sources_from_receipts(
    entries: &[Receipt],
) -> Vec<TrustedContextSourceSummary> {
    let mut sources: Vec<TrustedContextSourceSummary> = entries
        .iter()
        .filter(|receipt| receipt.kind == "twin.artifact.admitted")
        .filter_map(|admission| trusted_context_source_from_admission(entries, admission))
        .collect();
    sources.extend(
        entries
            .iter()
            .filter(|receipt| receipt.kind == "connector.item.ingested")
            .filter_map(|connector| trusted_context_source_from_connector(entries, connector)),
    );
    sources
}

fn trusted_context_source_from_admission(
    entries: &[Receipt],
    admission: &Receipt,
) -> Option<TrustedContextSourceSummary> {
    let artifact_id = context_payload_value(admission, "artifact_id");
    if artifact_id.trim().is_empty() {
        return None;
    }
    if entries.iter().rev().any(|receipt| {
        receipt.kind == "twin.artifact.delete_requested"
            && context_payload_value(receipt, "artifact_id") == artifact_id
    }) {
        return None;
    }
    let trust = latest_context_trust_decision(entries, &artifact_id)?;
    if context_payload_value(trust, "trusted_context_state") != "trusted_for_ai" {
        return None;
    }
    let storage_receipt_id = latest_context_receipt_for_artifact(
        entries,
        &artifact_id,
        "twin.artifact.storage.bytes_stored",
    )
    .or_else(|| {
        latest_context_receipt_for_artifact(entries, &artifact_id, "twin.artifact.storage.declared")
    })
    .unwrap_or_default();
    let parse_receipt_id = latest_context_receipt_for_artifact(
        entries,
        &artifact_id,
        "twin.artifact.context_packet.parsed",
    )
    .unwrap_or_default();
    let grounding = entries.iter().rev().find(|receipt| {
        receipt.kind == "twin.artifact.company_context.grounded"
            && context_payload_value(receipt, "artifact_id") == artifact_id
    });
    Some(TrustedContextSourceSummary {
        source_id: context_payload_value(trust, "source_id"),
        artifact_id: artifact_id.clone(),
        title: context_payload_value(admission, "artifact_name"),
        source_kind: context_payload_value(admission, "source_kind"),
        intake_surface: context_payload_value(admission, "source_intake_surface"),
        privacy_class: context_payload_value(admission, "privacy_class"),
        source_artifact_hash: context_payload_value(admission, "source_artifact_hash"),
        source_admission_receipt_id: admission.receipt_id.clone(),
        trust_decision_receipt_id: trust.receipt_id.clone(),
        storage_receipt_id,
        parse_receipt_id,
        grounding_receipt_id: grounding
            .map(|receipt| receipt.receipt_id.clone())
            .unwrap_or_default(),
        company_context_source: grounding
            .map(|receipt| context_payload_value(receipt, "company_context_source"))
            .unwrap_or_else(|| "Pages:local company context".to_string()),
        allowed_consumers: context_payload_value(trust, "allowed_consumers"),
    })
}

fn trusted_context_source_from_connector(
    entries: &[Receipt],
    connector: &Receipt,
) -> Option<TrustedContextSourceSummary> {
    if connector_payload_scope(connector) == "personal" {
        return None;
    }
    let source_id = connector.receipt_id.clone();
    if connector_item_deleted(entries, &source_id) {
        return None;
    }
    let trust = latest_context_trust_decision(entries, &source_id)?;
    if context_payload_value(trust, "trusted_context_state") != "trusted_for_ai" {
        return None;
    }
    let source_kind = context_payload_value(connector, "source_kind");
    Some(TrustedContextSourceSummary {
        source_id: source_id.clone(),
        artifact_id: source_id.clone(),
        title: context_payload_value(connector, "title"),
        source_kind,
        intake_surface: "connector".to_string(),
        privacy_class: context_payload_value(connector, "data_sensitivity"),
        source_artifact_hash: connector.hash.clone(),
        source_admission_receipt_id: connector.receipt_id.clone(),
        trust_decision_receipt_id: trust.receipt_id.clone(),
        storage_receipt_id: String::new(),
        parse_receipt_id: String::new(),
        grounding_receipt_id: String::new(),
        company_context_source: format!(
            "Connector:{}",
            context_payload_value(connector, "external_ref")
        ),
        allowed_consumers: context_payload_value(trust, "allowed_consumers"),
    })
}

fn latest_context_receipt_for_artifact(
    entries: &[Receipt],
    artifact_id: &str,
    kind: &str,
) -> Option<String> {
    entries
        .iter()
        .rev()
        .find(|receipt| {
            receipt.kind == kind && context_payload_value(receipt, "artifact_id") == artifact_id
        })
        .map(|receipt| receipt.receipt_id.clone())
}

fn latest_context_trust_decision<'a>(
    entries: &'a [Receipt],
    source_id: &str,
) -> Option<&'a Receipt> {
    entries.iter().rev().find(|receipt| {
        receipt.kind == "pages.context_source.trust_decision.recorded"
            && (context_payload_value(receipt, "source_id") == source_id
                || context_payload_value(receipt, "artifact_id") == source_id)
    })
}

fn context_payload_value(receipt: &Receipt, key: &str) -> String {
    receipt.payload.get(key).cloned().unwrap_or_default()
}

fn connector_payload_scope(receipt: &Receipt) -> &str {
    match receipt
        .payload
        .get("scope")
        .map(String::as_str)
        .unwrap_or("")
    {
        "" => "tenant",
        value => value,
    }
}

fn connector_item_deleted(entries: &[Receipt], item_receipt_id: &str) -> bool {
    entries.iter().rev().any(|receipt| {
        receipt.kind == "connector.item.delete_requested"
            && context_payload_value(receipt, "item_receipt_id") == item_receipt_id
    })
}
