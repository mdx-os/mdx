// Slice G: the Forge operator decision packet (backend, data-only).
//
// An operator looking at a build needs the same five things the first-viewport
// contract asks for: the current signal, the human judgment to make, the safe
// next move, the boundary, and the evidence to disclose. This assembles that
// packet from the build's terminal receipt - the autonomous completion, the
// escalation, or the ship readiness record - in human-first language, sourced
// only from receipts. It is data only: it renders no UI, and it changes no
// state. The packet itself never ratifies or deploys; it points at the door, it
// does not open it.
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForgeOperatorPacketRequest<'a> {
    pub worker_run_id: &'a str,
    /// The build's terminal receipt: harness.autonomous.completion.recorded,
    /// harness.autonomous.escalated, or harness.ship.readiness.assembled (and the
    /// blocked variants). The packet reads its kind and presence-only fields.
    pub terminal_receipt_id: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeOperatorDecisionPacket {
    pub status: &'static str,
    pub denied_reason: Option<String>,
    /// A human-first one-liner: what is true right now.
    pub current_signal: String,
    /// The terminal disposition the signal came from.
    pub disposition: String,
    /// The safe next move for the operator, in plain language.
    pub safe_next_move: String,
    /// The line the operator cannot cross from here.
    pub boundary: String,
    /// Whether a human decision is required next.
    pub needs_human: bool,
    /// Receipt ids the operator can open for the full chain (disclosure).
    pub evidence_receipt_ids: Vec<String>,
    pub packet_receipt_id: String,
    // The packet is evidence and a recommendation. It grants nothing.
    pub operator_can_ratify_here: bool,
    pub deployment_authority_granted: bool,
    pub receipt_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalForgeOperatorPacket;

impl LocalForgeOperatorPacket {
    pub fn assemble<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        request: &ForgeOperatorPacketRequest<'_>,
    ) -> Result<ForgeOperatorDecisionPacket, String> {
        kernel.assemble_forge_operator_packet(manifest, request)
    }
}

// The operator-facing reading of a terminal receipt: a signal, a next move,
// whether a human is needed, and which receipts to disclose.
struct OperatorReading {
    current_signal: &'static str,
    safe_next_move: &'static str,
    needs_human: bool,
    disposition: String,
    evidence_receipt_ids: Vec<String>,
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn assemble_forge_operator_packet(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        request: &ForgeOperatorPacketRequest<'_>,
    ) -> Result<ForgeOperatorDecisionPacket, String> {
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(manifest.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(manifest.actor_id),
            loop_id: LoopId::new("forge_operator_packet"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("operator_packet");
        let boundary =
            "this packet is evidence and a recommended next move; it does not ratify, deploy, or change state"
                .to_string();

        let reading = self.read_terminal_receipt(request.terminal_receipt_id.trim());
        let Some(reading) = reading else {
            let decision = self.decide_with_receipt(
                &correlation,
                ActionKind::AssembleForgeOperatorDecisionPacket,
            );
            let receipt = self.transition_receipt(
                &run_id,
                "OPERATOR_PACKET_BLOCKED",
                &correlation,
                &decision,
                "harness.operator.decision.packet.blocked",
                payload(&[
                    ("worker_run_id", request.worker_run_id),
                    ("denied_reason", "unrecognized_terminal_receipt"),
                ]),
            );
            self.storage.ledger().verify()?;
            return Ok(ForgeOperatorDecisionPacket {
                status: "OPERATOR_PACKET_BLOCKED",
                denied_reason: Some("unrecognized_terminal_receipt".to_string()),
                current_signal: "This build has no recognized terminal record yet".to_string(),
                disposition: String::new(),
                safe_next_move: "Wait for the build to reach a terminal state".to_string(),
                boundary,
                needs_human: true,
                evidence_receipt_ids: Vec::new(),
                packet_receipt_id: receipt.receipt_id.clone(),
                operator_can_ratify_here: false,
                deployment_authority_granted: false,
                receipt_ids: vec![receipt.receipt_id],
            });
        };

        let evidence = reading.evidence_receipt_ids.join(",");
        let decision = self.decide_with_receipt(
            &correlation,
            ActionKind::AssembleForgeOperatorDecisionPacket,
        );
        let receipt = self.transition_receipt(
            &run_id,
            "OPERATOR_PACKET_ASSEMBLED",
            &correlation,
            &decision,
            "harness.operator.decision.packet.assembled",
            payload(&[
                ("worker_run_id", request.worker_run_id),
                ("terminal_receipt_id", request.terminal_receipt_id),
                ("disposition", &reading.disposition),
                ("current_signal", reading.current_signal),
                ("safe_next_move", reading.safe_next_move),
                ("boundary", &boundary),
                (
                    "needs_human",
                    if reading.needs_human { "true" } else { "false" },
                ),
                ("evidence_receipt_ids", &evidence),
                // Data only: the packet recommends, it never acts.
                ("operator_can_ratify_here", "false"),
                ("deployment_authority_granted", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.storage.ledger().verify()?;
        Ok(ForgeOperatorDecisionPacket {
            status: "OPERATOR_PACKET_ASSEMBLED",
            denied_reason: None,
            current_signal: reading.current_signal.to_string(),
            disposition: reading.disposition,
            safe_next_move: reading.safe_next_move.to_string(),
            boundary,
            needs_human: reading.needs_human,
            evidence_receipt_ids: reading.evidence_receipt_ids,
            packet_receipt_id: receipt.receipt_id.clone(),
            operator_can_ratify_here: false,
            deployment_authority_granted: false,
            receipt_ids: vec![receipt.receipt_id],
        })
    }

    // Map a terminal receipt to an operator reading. Unknown or missing receipts
    // return None so the packet blocks rather than inventing a signal.
    fn read_terminal_receipt(&self, terminal_receipt_id: &str) -> Option<OperatorReading> {
        let receipt = self
            .storage
            .ledger()
            .query()
            .by_id(terminal_receipt_id)
            .cloned()?;
        let disposition = receipt
            .payload
            .get("terminal_state")
            .cloned()
            .unwrap_or_else(|| receipt.kind.clone());
        // Disclose whatever evidence the terminal receipt references, plus itself.
        let mut evidence = vec![terminal_receipt_id.to_string()];
        for key in [
            "review_packet_receipt_id",
            "ci_evidence_receipt_id",
            "worker_build_receipt_id",
        ] {
            if let Some(id) = receipt.payload.get(key)
                && !id.trim().is_empty()
            {
                evidence.push(id.clone());
            }
        }

        let (current_signal, safe_next_move, needs_human) = match receipt.kind.as_str() {
            "harness.autonomous.completion.recorded" => (
                "This change completed under your team's standing authorization",
                "Review the completed change when convenient; deployment, if any, is a separate governed step",
                false,
            ),
            "harness.autonomous.escalated" => (
                "A change is waiting for your decision",
                "Open the evidence, then ratify or reject the change at the ship door",
                true,
            ),
            "harness.ship.readiness.assembled" => (
                "A change is ready for your ratification",
                "Review the evidence, then ratify or reject at the ship door",
                true,
            ),
            "harness.autonomous.completion.blocked"
            | "harness.review.packet.blocked"
            | "harness.ship.readiness.blocked" => (
                "A change is blocked on its evidence",
                "Resolve the blocked build evidence, then re-run the gate",
                true,
            ),
            _ => return None,
        };
        Some(OperatorReading {
            current_signal,
            safe_next_move,
            needs_human,
            disposition,
            evidence_receipt_ids: evidence,
        })
    }
}
