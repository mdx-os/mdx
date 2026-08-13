// Slice 2 (Track 1): governed autonomy envelopes.
//
// An autonomy policy envelope authorizes bounded autonomous completion. For that
// authorization to mean anything, the envelope itself must be governed: recorded
// as a receipt by an accountable human, not handed to the completion gate as
// inline caller text. This module records an envelope (autonomy.envelope.recorded)
// and reconstructs it from its receipt so the runtime completion path can bind to
// a real, auditable envelope.
//
// Recording an envelope grants no execution, ship, or deploy authority; it only
// writes the standing policy into the ledger. Revocation is a later receipt,
// bound to the recorded envelope and limited to the owner or original recorder.
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutonomyEnvelopeRecordRequest<'a> {
    pub envelope: AutonomyPolicyEnvelope<'a>,
    /// The accountable human recording this envelope.
    pub recorded_by: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutonomyEnvelopeRevokeRequest<'a> {
    /// The autonomy.envelope.recorded receipt being revoked.
    pub envelope_receipt_id: &'a str,
    /// The accountable human revoking this envelope. Must be the envelope owner
    /// or the original recorder.
    pub revoked_by: &'a str,
    pub revocation_reason: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutonomyEnvelopeAmendRequest<'a> {
    /// The active autonomy envelope receipt being superseded.
    pub source_envelope_receipt_id: &'a str,
    /// The re-ratified envelope. Owner must match the source owner.
    pub envelope: AutonomyPolicyEnvelope<'a>,
    /// The accountable human amending this envelope. Must be the envelope owner
    /// or the original recorder.
    pub amended_by: &'a str,
    pub amendment_reason: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutonomyEnvelopeExpirySweepRequest<'a> {
    /// Actor or job id that performed the deterministic sweep.
    pub swept_by: &'a str,
    /// ISO-8601 UTC instant used for the sweep.
    pub observed_at: &'a str,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalAutonomyEnvelopeRegistry;

impl LocalAutonomyEnvelopeRegistry {
    pub fn record<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        request: &AutonomyEnvelopeRecordRequest<'_>,
    ) -> Result<String, String> {
        kernel.record_autonomy_envelope(manifest, request)
    }

    pub fn revoke<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        request: &AutonomyEnvelopeRevokeRequest<'_>,
    ) -> Result<String, String> {
        kernel.revoke_autonomy_envelope(manifest, request)
    }

    pub fn amend<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        request: &AutonomyEnvelopeAmendRequest<'_>,
    ) -> Result<String, String> {
        kernel.amend_autonomy_envelope(manifest, request)
    }

    pub fn sweep_expired<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        request: &AutonomyEnvelopeExpirySweepRequest<'_>,
    ) -> Result<Vec<String>, String> {
        kernel.sweep_expired_autonomy_envelopes(manifest, request)
    }
}

/// The envelope state read back from its receipt chain. The classification
/// consumer left with the deleted autonomous-completion gate; the LIVE
/// consumers (amend, expiry sweep, the authority blocker) need only
/// whether the envelope is still active.
pub(crate) struct ReconstructedEnvelope {
    pub active: bool,
}

fn is_envelope_receipt_kind(kind: &str) -> bool {
    kind == "autonomy.envelope.recorded" || kind == "autonomy.envelope.amended"
}

impl<S: StorageProvider> MdxKernel<S> {
    /// Why a named envelope cannot authorize work right now, or None when
    /// it is active. This is the single authority predicate the run
    /// admission gate and the mid-run revocation poll both consult, so an
    /// envelope's state means the same thing everywhere. Expiry relies on
    /// the deterministic sweep receipt (the kernel takes no ambient
    /// clock), matching the trusted-time doctrine.
    pub fn autonomy_envelope_authority_blocker(&self, envelope_id: &str) -> Option<String> {
        let ledger = self.storage.ledger();
        let recorded = ledger
            .query()
            .by_kind("autonomy.envelope.recorded")
            .iter()
            .chain(ledger.query().by_kind("autonomy.envelope.amended").iter())
            .any(|receipt| {
                receipt.payload.get("envelope_id").map(String::as_str) == Some(envelope_id)
            });
        if !recorded {
            return Some("envelope_not_found".to_string());
        }
        for (kind, blocker) in [
            ("autonomy.envelope.revoked", "envelope_revoked"),
            ("autonomy.envelope.expired", "envelope_expired"),
        ] {
            let hit = ledger.query().by_kind(kind).iter().any(|receipt| {
                receipt.payload.get("envelope_id").map(String::as_str) == Some(envelope_id)
            });
            if hit {
                return Some(blocker.to_string());
            }
        }
        None
    }

    pub fn record_autonomy_envelope(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        request: &AutonomyEnvelopeRecordRequest<'_>,
    ) -> Result<String, String> {
        let envelope = &request.envelope;
        if envelope.envelope_id.trim().is_empty() {
            return Err("missing_envelope_id".to_string());
        }
        if envelope.owner_id.trim().is_empty() {
            return Err("missing_envelope_owner".to_string());
        }
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(manifest.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(manifest.actor_id),
            loop_id: LoopId::new("autonomy_envelope_registry"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("autonomy_envelope");
        let decision = self.decide_with_receipt(&correlation, ActionKind::RecordAutonomyEnvelope);
        let receipt = self.transition_receipt(
            &run_id,
            "AUTONOMY_ENVELOPE_RECORDED",
            &correlation,
            &decision,
            "autonomy.envelope.recorded",
            payload(&[
                ("envelope_id", envelope.envelope_id),
                ("owner_id", envelope.owner_id),
                ("recorded_by", request.recorded_by),
                ("max_risk_class", envelope.max_risk_class.as_str()),
                (
                    "preapproved_work_classes",
                    &envelope.preapproved_work_classes.join(","),
                ),
                (
                    "allowed_path_scopes",
                    &envelope.allowed_path_scopes.join(","),
                ),
                ("self_delivery_scope", envelope.self_delivery_scope.as_str()),
                (
                    "escalation_triggers",
                    &envelope.escalation_triggers.join(","),
                ),
                ("expires_at", envelope.expires_at),
                (
                    "rollback_required",
                    if envelope.rollback_required {
                        "true"
                    } else {
                        "false"
                    },
                ),
                (
                    "eval_required",
                    if envelope.eval_required {
                        "true"
                    } else {
                        "false"
                    },
                ),
                ("active", if envelope.active { "true" } else { "false" }),
                // Recording a policy grants no execution authority.
                ("grants_ship_ratification", "false"),
                ("grants_deployment_authority", "false"),
            ]),
        );
        self.storage.ledger().verify()?;
        Ok(receipt.receipt_id)
    }

    pub fn revoke_autonomy_envelope(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        request: &AutonomyEnvelopeRevokeRequest<'_>,
    ) -> Result<String, String> {
        let envelope_receipt_id = request.envelope_receipt_id.trim();
        if envelope_receipt_id.is_empty() {
            return Err("missing_envelope_receipt_id".to_string());
        }
        if request.revoked_by.trim().is_empty() {
            return Err("missing_revoker".to_string());
        }
        if request.revocation_reason.trim().is_empty() {
            return Err("missing_revocation_reason".to_string());
        }
        let Some(envelope_receipt) = self.storage.ledger().query().by_id(envelope_receipt_id)
        else {
            return Err("missing_envelope_receipt".to_string());
        };
        if envelope_receipt.kind != "autonomy.envelope.recorded" {
            return Err("wrong_envelope_receipt_kind".to_string());
        }
        let owner_id = envelope_receipt
            .payload
            .get("owner_id")
            .map(String::as_str)
            .unwrap_or("");
        let recorded_by = envelope_receipt
            .payload
            .get("recorded_by")
            .map(String::as_str)
            .unwrap_or("");
        let revoked_by = request.revoked_by.trim();
        if revoked_by != owner_id && revoked_by != recorded_by {
            return Err("revoker_not_authorized".to_string());
        }
        let envelope_id = envelope_receipt
            .payload
            .get("envelope_id")
            .cloned()
            .unwrap_or_default();
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(manifest.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(manifest.actor_id),
            loop_id: LoopId::new("autonomy_envelope_registry"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("autonomy_envelope_revoke");
        let decision = self.decide_with_receipt(&correlation, ActionKind::RevokeAutonomyEnvelope);
        let receipt = self.transition_receipt(
            &run_id,
            "AUTONOMY_ENVELOPE_REVOKED",
            &correlation,
            &decision,
            "autonomy.envelope.revoked",
            payload(&[
                ("envelope_id", &envelope_id),
                ("source_envelope_receipt_id", envelope_receipt_id),
                ("revoked_by", revoked_by),
                ("revocation_reason", request.revocation_reason.trim()),
                ("grants_ship_ratification", "false"),
                ("grants_deployment_authority", "false"),
            ]),
        );
        self.storage.ledger().verify()?;
        Ok(receipt.receipt_id)
    }

    pub fn amend_autonomy_envelope(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        request: &AutonomyEnvelopeAmendRequest<'_>,
    ) -> Result<String, String> {
        let source_envelope_receipt_id = request.source_envelope_receipt_id.trim();
        if source_envelope_receipt_id.is_empty() {
            return Err("missing_envelope_receipt_id".to_string());
        }
        if request.amended_by.trim().is_empty() {
            return Err("missing_amender".to_string());
        }
        if request.amendment_reason.trim().is_empty() {
            return Err("missing_amendment_reason".to_string());
        }
        let envelope = &request.envelope;
        if envelope.envelope_id.trim().is_empty() {
            return Err("missing_envelope_id".to_string());
        }
        if envelope.owner_id.trim().is_empty() {
            return Err("missing_envelope_owner".to_string());
        }
        let Some(source_receipt) = self
            .storage
            .ledger()
            .query()
            .by_id(source_envelope_receipt_id)
        else {
            return Err("missing_envelope_receipt".to_string());
        };
        if !is_envelope_receipt_kind(&source_receipt.kind) {
            return Err("wrong_envelope_receipt_kind".to_string());
        }
        if self
            .reconstruct_autonomy_envelope(source_envelope_receipt_id)
            .map(|reconstructed| reconstructed.active)
            != Some(true)
        {
            return Err("source_envelope_inactive".to_string());
        }
        let owner_id = source_receipt
            .payload
            .get("owner_id")
            .cloned()
            .unwrap_or_default();
        let recorded_by = source_receipt
            .payload
            .get("recorded_by")
            .cloned()
            .unwrap_or_default();
        let amended_by = request.amended_by.trim();
        if amended_by != owner_id && amended_by != recorded_by {
            return Err("amender_not_authorized".to_string());
        }
        if envelope.owner_id.trim() != owner_id {
            return Err("amended_owner_mismatch".to_string());
        }
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(manifest.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(manifest.actor_id),
            loop_id: LoopId::new("autonomy_envelope_registry"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("autonomy_envelope_amend");
        let decision = self.decide_with_receipt(&correlation, ActionKind::AmendAutonomyEnvelope);
        let receipt = self.transition_receipt(
            &run_id,
            "AUTONOMY_ENVELOPE_AMENDED",
            &correlation,
            &decision,
            "autonomy.envelope.amended",
            payload(&[
                ("envelope_id", envelope.envelope_id),
                ("source_envelope_receipt_id", source_envelope_receipt_id),
                ("owner_id", envelope.owner_id),
                ("recorded_by", &recorded_by),
                ("amended_by", amended_by),
                ("amendment_reason", request.amendment_reason.trim()),
                ("max_risk_class", envelope.max_risk_class.as_str()),
                (
                    "preapproved_work_classes",
                    &envelope.preapproved_work_classes.join(","),
                ),
                (
                    "allowed_path_scopes",
                    &envelope.allowed_path_scopes.join(","),
                ),
                ("self_delivery_scope", envelope.self_delivery_scope.as_str()),
                (
                    "escalation_triggers",
                    &envelope.escalation_triggers.join(","),
                ),
                ("expires_at", envelope.expires_at),
                (
                    "rollback_required",
                    if envelope.rollback_required {
                        "true"
                    } else {
                        "false"
                    },
                ),
                (
                    "eval_required",
                    if envelope.eval_required {
                        "true"
                    } else {
                        "false"
                    },
                ),
                ("active", if envelope.active { "true" } else { "false" }),
                ("grants_ship_ratification", "false"),
                ("grants_deployment_authority", "false"),
            ]),
        );
        self.storage.ledger().verify()?;
        Ok(receipt.receipt_id)
    }

    pub fn sweep_expired_autonomy_envelopes(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        request: &AutonomyEnvelopeExpirySweepRequest<'_>,
    ) -> Result<Vec<String>, String> {
        let observed_at = request.observed_at.trim();
        if request.swept_by.trim().is_empty() {
            return Err("missing_sweeper".to_string());
        }
        if observed_at.is_empty() {
            return Err("missing_observed_at".to_string());
        }
        let candidates: Vec<Receipt> = self
            .storage
            .ledger()
            .entries()
            .iter()
            .filter(|receipt| is_envelope_receipt_kind(&receipt.kind))
            .cloned()
            .collect();
        let mut expired_receipts = Vec::new();
        for source in candidates {
            let expires_at = source
                .payload
                .get("expires_at")
                .map(String::as_str)
                .unwrap_or("");
            if expires_at.is_empty() || observed_at < expires_at {
                continue;
            }
            if self
                .reconstruct_autonomy_envelope(&source.receipt_id)
                .map(|reconstructed| reconstructed.active)
                != Some(true)
            {
                continue;
            }
            let already_swept = self.storage.ledger().entries().iter().any(|entry| {
                entry.kind == "autonomy.envelope.expired"
                    && entry
                        .payload
                        .get("source_envelope_receipt_id")
                        .map(String::as_str)
                        == Some(source.receipt_id.as_str())
            });
            if already_swept {
                continue;
            }
            let envelope_id = source
                .payload
                .get("envelope_id")
                .cloned()
                .unwrap_or_default();
            let correlation = CorrelationIds {
                tenant_id: TenantId::new(manifest.tenant_id),
                trace_id: TraceId::new(self.ids.next("trace")),
                actor_id: ActorId::new(manifest.actor_id),
                loop_id: LoopId::new("autonomy_envelope_registry"),
                workflow_id: WorkflowId::new(self.ids.next("workflow")),
            };
            let run_id = self.ids.next("autonomy_envelope_expiry_sweep");
            let decision =
                self.decide_with_receipt(&correlation, ActionKind::SweepExpiredAutonomyEnvelope);
            let receipt = self.transition_receipt(
                &run_id,
                "AUTONOMY_ENVELOPE_EXPIRED",
                &correlation,
                &decision,
                "autonomy.envelope.expired",
                payload(&[
                    ("envelope_id", &envelope_id),
                    ("source_envelope_receipt_id", &source.receipt_id),
                    ("swept_by", request.swept_by.trim()),
                    ("observed_at", observed_at),
                    ("expired_at", expires_at),
                    ("grants_ship_ratification", "false"),
                    ("grants_deployment_authority", "false"),
                ]),
            );
            expired_receipts.push(receipt.receipt_id);
        }
        self.storage.ledger().verify()?;
        Ok(expired_receipts)
    }

    // Reconstruct an envelope from its receipt. None when the id is missing or
    // does not point at a real autonomy.envelope.recorded receipt.
    pub(crate) fn reconstruct_autonomy_envelope(
        &self,
        envelope_receipt_id: &str,
    ) -> Option<ReconstructedEnvelope> {
        let receipt = self
            .storage
            .ledger()
            .query()
            .by_id(envelope_receipt_id.trim())
            .cloned()?;
        if !is_envelope_receipt_kind(&receipt.kind) {
            return None;
        }
        let get = |key: &str| receipt.payload.get(key).cloned().unwrap_or_default();
        let deactivated = self.storage.ledger().entries().iter().any(|entry| {
            matches!(
                entry.kind.as_str(),
                "autonomy.envelope.revoked"
                    | "autonomy.envelope.amended"
                    | "autonomy.envelope.expired"
            ) && entry
                .payload
                .get("source_envelope_receipt_id")
                .map(String::as_str)
                == Some(envelope_receipt_id.trim())
        });
        Some(ReconstructedEnvelope {
            active: get("active") == "true" && !deactivated,
        })
    }
}
