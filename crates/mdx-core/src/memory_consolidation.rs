//! Deterministic memory adjudication and distinct-actor ratification.
//!
//! Adjudication compares an incoming candidate fact against the records
//! already held in the same tenant and scope and answers ADD, SUPERSEDE, or
//! NOOP with the cited record on the receipt - the auditable
//! extract-then-consolidate decision grammar, computed without a model so
//! the same candidate always adjudicates the same way.
//!
//! Ratification retires the self-approving review stub for shared scopes:
//! a shared-scope consolidation is stored pending and only a DISTINCT actor
//! can activate (or decline) it through the governed
//! memory.consolidation.ratified write.

use super::*;

/// Token overlap at or above this percent is the same knowledge: NOOP.
const MEMORY_DUPLICATE_OVERLAP_PERCENT: usize = 90;
/// Subject-token overlap at or above this percent is the same subject: a
/// newer trusted-time candidate SUPERSEDEs the older record.
const MEMORY_SUBJECT_OVERLAP_PERCENT: usize = 60;

const MAX_RATIFICATION_NOTE_CHARS: usize = 2000;

// Actor ids carry a type prefix at some entry points and arrive as the bare
// authenticated subject at others. Distinct-person governance compares the
// principal, not the serialization used by a route, so `human:local_user`
// and `local_user` must be treated as the same actor.
fn normalized_actor_principal(actor_id: &str) -> &str {
    actor_id
        .trim()
        .trim_start_matches("human:")
        .trim_start_matches("agent:")
        .trim_start_matches("system:")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryAdjudication {
    pub decision: ConsolidationDecision,
    /// The existing record cited on NOOP and SUPERSEDE; empty on ADD.
    pub cited_memory_id: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryConsolidationOutcome {
    pub decision: &'static str,
    /// Empty when the decision wrote no record (NOOP).
    pub memory_id: String,
    pub cited_memory_id: String,
    pub consolidation_state: &'static str,
    pub gate_receipt_id: String,
}

#[derive(Clone, Copy, Debug)]
pub struct MemoryConsolidationRatification<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub memory_id: &'a str,
    /// "approve" or "decline".
    pub decision: &'a str,
    pub note: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryRatificationReport {
    pub memory_id: String,
    pub ratification_decision: String,
    pub consolidation_state: &'static str,
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub proposed_by: String,
    pub ratified_by: String,
}

/// Whether two memory contents are about the same subject: overlap of
/// subject tokens (negation and auxiliary words stripped, plural-stemmed)
/// above the subject threshold. "User prefers concise planning" and "User
/// does not prefer concise planning" are the same subject; a novel fact is
/// not.
pub(crate) fn memory_same_subject(a: &str, b: &str) -> bool {
    token_overlap_percent(&memory_subject_tokens(a), &memory_subject_tokens(b))
        >= MEMORY_SUBJECT_OVERLAP_PERCENT
}

fn memory_subject_tokens(content: &str) -> Vec<String> {
    normalized_tokens(content)
        .into_iter()
        .filter(|token| {
            !matches!(
                token.as_str(),
                "a" | "an"
                    | "the"
                    | "is"
                    | "are"
                    | "was"
                    | "were"
                    | "does"
                    | "do"
                    | "did"
                    | "not"
                    | "no"
                    | "never"
                    | "dont"
                    | "doesnt"
                    | "avoid"
                    | "longer"
                    | "anymore"
                    | "now"
            )
        })
        .map(|token| {
            // Cheap plural/verb-s stem so "prefers" and "prefer" agree.
            if token.len() > 3 && token.ends_with('s') && !token.ends_with("ss") {
                token[..token.len() - 1].to_string()
            } else {
                token
            }
        })
        .collect()
}

/// Jaccard overlap of two token lists, in whole percent.
fn token_overlap_percent(a: &[String], b: &[String]) -> usize {
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    let left: std::collections::BTreeSet<&str> = a.iter().map(String::as_str).collect();
    let right: std::collections::BTreeSet<&str> = b.iter().map(String::as_str).collect();
    let intersection = left.intersection(&right).count();
    let union = left.union(&right).count();
    (intersection * 100).checked_div(union).unwrap_or(0)
}

impl<S: StorageProvider> MdxKernel<S> {
    /// Deterministic adjudication of one candidate fact against the records
    /// held in the same tenant and scope. Exact or near-duplicate content is
    /// NOOP citing the existing record; the same subject with newer trusted
    /// time is SUPERSEDE citing the record it closes; anything else is ADD.
    pub fn adjudicate_memory_candidate(
        &self,
        tenant_id: &str,
        memory_scope: &str,
        content: &str,
        candidate_valid_from: &str,
    ) -> MemoryAdjudication {
        let candidate_tokens = normalized_tokens(content);
        let candidate_subject = memory_subject_tokens(content);
        let mut best_subject: Option<(usize, &MemoryRecord)> = None;
        for record in self.storage.memory_records() {
            if record.tenant_id.as_str() != tenant_id
                || record.memory_scope != memory_scope
                || record.consolidation_state == MEMORY_CONSOLIDATION_DECLINED
                || !record.valid_until_receipt_timestamp.is_empty()
                || !self.memory_lifecycle_active(&record.memory_id)
            {
                continue;
            }
            let duplicate_overlap =
                token_overlap_percent(&candidate_tokens, &normalized_tokens(&record.content));
            if record.content.trim() == content.trim()
                || duplicate_overlap >= MEMORY_DUPLICATE_OVERLAP_PERCENT
            {
                return MemoryAdjudication {
                    decision: ConsolidationDecision::Noop,
                    cited_memory_id: record.memory_id.clone(),
                    reason: format!(
                        "candidate duplicates memory {} at {duplicate_overlap} percent token overlap",
                        record.memory_id
                    ),
                };
            }
            let subject_overlap =
                token_overlap_percent(&candidate_subject, &memory_subject_tokens(&record.content));
            if subject_overlap >= MEMORY_SUBJECT_OVERLAP_PERCENT
                && best_subject
                    .as_ref()
                    .map(|(best, _)| subject_overlap > *best)
                    .unwrap_or(true)
            {
                best_subject = Some((subject_overlap, record));
            }
        }
        if let Some((overlap, existing)) = best_subject {
            if candidate_valid_from > existing.valid_from_receipt_timestamp.as_str() {
                return MemoryAdjudication {
                    decision: ConsolidationDecision::Supersede,
                    cited_memory_id: existing.memory_id.clone(),
                    reason: format!(
                        "candidate carries newer trusted time on the subject of memory {} ({overlap} percent subject overlap)",
                        existing.memory_id
                    ),
                };
            }
            return MemoryAdjudication {
                decision: ConsolidationDecision::Noop,
                cited_memory_id: existing.memory_id.clone(),
                reason: format!(
                    "memory {} already holds newer or equal trusted time on this subject",
                    existing.memory_id
                ),
            };
        }
        MemoryAdjudication {
            decision: ConsolidationDecision::Add,
            cited_memory_id: String::new(),
            reason: "no held record duplicates or shares this subject".to_string(),
        }
    }

    fn memory_lifecycle_active(&self, memory_id: &str) -> bool {
        self.storage
            .memory_lifecycle_events()
            .iter()
            .rev()
            .find(|event| event.memory_id == memory_id)
            .map(|event| event.lifecycle_state == "active")
            .unwrap_or(true)
    }

    /// The governed clear for a pending shared-scope consolidation. Requires
    /// an actor DISTINCT from the proposer on the gate receipt: the write
    /// ceremony can no longer review itself. Approval activates the record
    /// and seeds its recall ranking; decline suppresses it while keeping the
    /// stored content and every receipt.
    pub fn ratify_memory_consolidation(
        &mut self,
        request: MemoryConsolidationRatification<'_>,
    ) -> Result<MemoryRatificationReport, String> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("memory_id", request.memory_id),
            ("decision", request.decision),
        ] {
            if value.trim().is_empty() {
                return Err(format!(
                    "memory consolidation ratification: missing {field}"
                ));
            }
        }
        if request.note.chars().count() > MAX_RATIFICATION_NOTE_CHARS {
            return Err(format!(
                "the note is {} characters; the limit is {MAX_RATIFICATION_NOTE_CHARS}",
                request.note.chars().count()
            ));
        }
        let decision = request.decision.trim();
        if decision != "approve" && decision != "decline" {
            return Err(format!(
                "memory consolidation ratification decision must be approve or decline, not {decision}"
            ));
        }
        let record = self
            .storage
            .memory_records()
            .iter()
            .find(|record| {
                record.memory_id == request.memory_id.trim()
                    && record.tenant_id.as_str() == request.tenant_id
            })
            .cloned()
            .ok_or_else(|| format!("memory {} missing", request.memory_id.trim()))?;
        if record.consolidation_state != MEMORY_CONSOLIDATION_PENDING {
            return Err(format!(
                "memory {} is {}, not pending ratification",
                record.memory_id, record.consolidation_state
            ));
        }
        let proposed_by = self
            .storage
            .ledger()
            .query()
            .by_id(&record.provenance.gate_receipt_id)
            .map(|receipt| receipt.actor_id.as_str().to_string())
            .unwrap_or_default();
        if normalized_actor_principal(&proposed_by) == normalized_actor_principal(request.actor_id)
        {
            return Err(format!(
                "memory consolidation ratification requires a distinct actor: {} proposed this memory and cannot ratify it",
                request.actor_id.trim()
            ));
        }
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("memory_ratification"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let approved = decision == "approve";
        let consolidation_state = if approved {
            MEMORY_CONSOLIDATION_ACTIVE
        } else {
            MEMORY_CONSOLIDATION_DECLINED
        };
        let policy = self.decide_with_receipt(&correlation, ActionKind::WriteMemory);
        let receipt = self.storage.append_receipt(
            &mut self.ids,
            &correlation,
            "memory.consolidation.ratified",
            Some(policy.policy_decision_id.clone()),
            payload(&[
                ("memory_id", &record.memory_id),
                ("ratification_decision", decision),
                ("consolidation_state", consolidation_state),
                ("proposed_by", &proposed_by),
                ("ratified_by", request.actor_id.trim()),
                (
                    "proposal_gate_receipt_id",
                    &record.provenance.gate_receipt_id,
                ),
                ("memory_scope", record.memory_scope),
                ("note", request.note.trim()),
                ("distinct_actor_required", "true"),
                ("adaptation_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.storage
            .set_memory_consolidation_state(&record.memory_id, consolidation_state);
        if approved {
            self.storage
                .push_memory_lifecycle_event(MemoryLifecycleEvent {
                    event_id: self.ids.next("memory_lifecycle"),
                    tenant_id: correlation.tenant_id.clone(),
                    memory_id: record.memory_id.clone(),
                    action: "retain",
                    lifecycle_state: "active",
                    reason: "distinct-actor ratification approved this shared-scope memory"
                        .to_string(),
                    source_receipt_id: record.source_receipt_id.clone(),
                    valid_from_receipt_timestamp: receipt.receipt_timestamp.clone(),
                    receipt_id: receipt.receipt_id.clone(),
                });
            self.record_memory_recall_ranking(
                &correlation,
                surface_for_memory_scope(record.memory_scope),
                &record.content,
                record.memory_scope,
                &receipt.receipt_id,
            );
        } else {
            self.storage
                .push_memory_lifecycle_event(MemoryLifecycleEvent {
                    event_id: self.ids.next("memory_lifecycle"),
                    tenant_id: correlation.tenant_id.clone(),
                    memory_id: record.memory_id.clone(),
                    action: "suppress",
                    lifecycle_state: "suppressed",
                    reason: "distinct-actor ratification declined this shared-scope memory"
                        .to_string(),
                    source_receipt_id: record.source_receipt_id.clone(),
                    valid_from_receipt_timestamp: receipt.receipt_timestamp.clone(),
                    receipt_id: receipt.receipt_id.clone(),
                });
            self.storage
                .set_memory_lifecycle_state(&record.memory_id, "suppressed");
        }
        Ok(MemoryRatificationReport {
            memory_id: record.memory_id,
            ratification_decision: decision.to_string(),
            consolidation_state,
            receipt_id: receipt.receipt_id.clone(),
            policy_decision_id: policy.policy_decision_id,
            proposed_by,
            ratified_by: request.actor_id.trim().to_string(),
        })
    }

    /// The pending shared-scope consolidations awaiting a distinct actor,
    /// oldest first.
    pub fn pending_memory_consolidations(&self) -> Vec<MemoryRecord> {
        self.storage
            .memory_records()
            .iter()
            .filter(|record| record.consolidation_state == MEMORY_CONSOLIDATION_PENDING)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kernel_with_source() -> (MdxKernel, String) {
        let mut kernel = MdxKernel::boot_local();
        kernel.run_evals_runner_agent().expect("loop run");
        let source_receipt_id = kernel
            .ledger()
            .entries()
            .last()
            .expect("source receipt")
            .receipt_id
            .clone();
        (kernel, source_receipt_id)
    }

    fn consolidate(
        kernel: &mut MdxKernel,
        scope: &'static str,
        surface: &'static str,
        content: &str,
    ) -> MemoryConsolidationOutcome {
        let source_receipt_id = kernel
            .ledger()
            .entries()
            .last()
            .expect("source receipt")
            .receipt_id
            .clone();
        kernel
            .consolidate_surface_memory(
                "local_tenant",
                "human:proposer",
                surface,
                scope,
                &source_receipt_id,
                content,
                "adjudicated_fact_atom",
            )
            .expect("consolidation")
    }

    #[test]
    fn duplicate_candidate_adjudicates_noop_citing_the_existing_record() {
        let (mut kernel, _) = kernel_with_source();
        let first = consolidate(
            &mut kernel,
            "private_user_memory",
            "twin",
            "User prefers concise planning summaries",
        );
        assert_eq!(first.decision, "ADD");
        let before = kernel.memory_records().len();
        let second = consolidate(
            &mut kernel,
            "private_user_memory",
            "twin",
            "User prefers concise planning summaries",
        );
        assert_eq!(second.decision, "NOOP");
        assert_eq!(second.cited_memory_id, first.memory_id);
        assert!(second.memory_id.is_empty());
        assert_eq!(
            kernel.memory_records().len(),
            before,
            "NOOP writes no record"
        );
        let gate = kernel
            .ledger()
            .query()
            .by_id(&second.gate_receipt_id)
            .expect("gate receipt");
        assert_eq!(
            gate.payload.get("decision").map(String::as_str),
            Some("NOOP")
        );
        assert_eq!(
            gate.payload
                .get("adjudicated_against_memory_id")
                .map(String::as_str),
            Some(first.memory_id.as_str())
        );
    }

    #[test]
    fn newer_same_subject_candidate_supersedes_and_stamps_the_validity_window() {
        let (mut kernel, _) = kernel_with_source();
        let first = consolidate(
            &mut kernel,
            "private_user_memory",
            "twin",
            "User prefers dark mode in the editor",
        );
        let second = consolidate(
            &mut kernel,
            "private_user_memory",
            "twin",
            "User prefers light mode in the editor",
        );
        assert_eq!(second.decision, "SUPERSEDE");
        assert_eq!(second.cited_memory_id, first.memory_id);
        let old = kernel
            .memory_records()
            .iter()
            .find(|record| record.memory_id == first.memory_id)
            .expect("superseded record");
        assert!(!old.valid_until_receipt_timestamp.is_empty());
        assert_eq!(old.invalidated_by_receipt_id, second.gate_receipt_id);
        assert!(
            kernel
                .memory_graph_edges()
                .iter()
                .any(|edge| edge.edge_kind == "SUPERSEDES"
                    && edge.source_receipt_id == second.gate_receipt_id)
        );
        assert!(
            kernel
                .memory_lifecycle_events()
                .iter()
                .any(|event| event.memory_id == first.memory_id
                    && event.lifecycle_state == "superseded")
        );
        // The new record's own window is open.
        let new = kernel
            .memory_records()
            .iter()
            .find(|record| record.memory_id == second.memory_id)
            .expect("superseding record");
        assert!(new.valid_until_receipt_timestamp.is_empty());
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn novel_candidate_adjudicates_add() {
        let (mut kernel, _) = kernel_with_source();
        consolidate(
            &mut kernel,
            "private_user_memory",
            "twin",
            "User prefers concise planning summaries",
        );
        let novel = consolidate(
            &mut kernel,
            "private_user_memory",
            "twin",
            "Forge runs on this repo must prove cargo test before finishing",
        );
        assert_eq!(novel.decision, "ADD");
        assert!(novel.cited_memory_id.is_empty());
        assert!(!novel.memory_id.is_empty());
    }

    #[test]
    fn shared_scope_consolidation_lands_pending_and_stays_out_of_recall() {
        let (mut kernel, source_receipt_id) = kernel_with_source();
        let outcome = kernel
            .consolidate_surface_memory(
                "local_tenant",
                "human:proposer",
                "messages",
                "team_memory",
                &source_receipt_id,
                "Team decided to ship the beta gate first",
                "unadjudicated_verbatim",
            )
            .expect("pending consolidation");
        assert_eq!(outcome.consolidation_state, MEMORY_CONSOLIDATION_PENDING);
        let record = kernel
            .memory_records()
            .iter()
            .find(|record| record.memory_id == outcome.memory_id)
            .expect("stored pending record");
        assert_eq!(record.consolidation_state, MEMORY_CONSOLIDATION_PENDING);
        // Stored but excluded from recall rankings.
        let correlation = CorrelationIds {
            tenant_id: TenantId::new("local_tenant"),
            trace_id: TraceId::new("trace_pending_recall"),
            actor_id: ActorId::new("human:reader"),
            loop_id: LoopId::new("test"),
            workflow_id: WorkflowId::new("wf_test"),
        };
        let rankings = kernel.record_memory_recall_ranking(
            &correlation,
            "messages",
            "Team decided to ship the beta gate first",
            "team_memory",
            &source_receipt_id,
        );
        assert!(
            rankings
                .iter()
                .all(|ranking| ranking.memory_id != outcome.memory_id),
            "pending memory must not be recalled"
        );
        assert!(
            kernel
                .pending_memory_consolidations()
                .iter()
                .any(|pending| pending.memory_id == outcome.memory_id)
        );
    }

    #[test]
    fn distinct_actor_ratification_activates_and_self_ratification_is_refused() {
        let (mut kernel, source_receipt_id) = kernel_with_source();
        let outcome = kernel
            .consolidate_surface_memory(
                "local_tenant",
                "human:proposer",
                "messages",
                "team_memory",
                &source_receipt_id,
                "Team decided to ship the beta gate first",
                "unadjudicated_verbatim",
            )
            .expect("pending consolidation");
        let refused = kernel.ratify_memory_consolidation(MemoryConsolidationRatification {
            tenant_id: "local_tenant",
            actor_id: "human:proposer",
            memory_id: &outcome.memory_id,
            decision: "approve",
            note: "approving my own proposal",
        });
        assert!(refused.is_err(), "self-ratification must refuse");
        assert!(refused.unwrap_err().contains("distinct actor"));

        let report = kernel
            .ratify_memory_consolidation(MemoryConsolidationRatification {
                tenant_id: "local_tenant",
                actor_id: "human:reviewer",
                memory_id: &outcome.memory_id,
                decision: "approve",
                note: "the team record matches the thread",
            })
            .expect("distinct-actor ratification");
        assert_eq!(report.consolidation_state, MEMORY_CONSOLIDATION_ACTIVE);
        assert_eq!(report.proposed_by, "human:proposer");
        assert_eq!(report.ratified_by, "human:reviewer");
        let record = kernel
            .memory_records()
            .iter()
            .find(|record| record.memory_id == outcome.memory_id)
            .expect("ratified record");
        assert_eq!(record.consolidation_state, MEMORY_CONSOLIDATION_ACTIVE);
        // Now recallable.
        let correlation = CorrelationIds {
            tenant_id: TenantId::new("local_tenant"),
            trace_id: TraceId::new("trace_ratified_recall"),
            actor_id: ActorId::new("human:reader"),
            loop_id: LoopId::new("test"),
            workflow_id: WorkflowId::new("wf_test"),
        };
        let rankings = kernel.record_memory_recall_ranking(
            &correlation,
            "messages",
            "Team decided to ship the beta gate first",
            "team_memory",
            &source_receipt_id,
        );
        assert!(
            rankings
                .iter()
                .any(|ranking| ranking.memory_id == outcome.memory_id),
            "ratified memory must be recallable"
        );
        // A second ratification of the same record refuses: no longer pending.
        let again = kernel.ratify_memory_consolidation(MemoryConsolidationRatification {
            tenant_id: "local_tenant",
            actor_id: "human:reviewer",
            memory_id: &outcome.memory_id,
            decision: "approve",
            note: "",
        });
        assert!(again.is_err());
    }

    #[test]
    fn actor_prefix_does_not_bypass_distinct_person_ratification() {
        let (mut kernel, source_receipt_id) = kernel_with_source();
        let outcome = kernel
            .consolidate_surface_memory(
                "local_tenant",
                "human:local_user",
                "messages",
                "team_memory",
                &source_receipt_id,
                "The launch review happens on Friday",
                "unadjudicated_verbatim",
            )
            .expect("pending consolidation");

        let refused = kernel.ratify_memory_consolidation(MemoryConsolidationRatification {
            tenant_id: "local_tenant",
            actor_id: "local_user",
            memory_id: &outcome.memory_id,
            decision: "approve",
            note: "same authenticated person without the actor prefix",
        });

        assert!(
            refused.is_err(),
            "an actor prefix must not create a second person"
        );
        assert!(refused.unwrap_err().contains("distinct actor"));
    }

    #[test]
    fn declined_ratification_suppresses_but_keeps_the_stored_record() {
        let (mut kernel, source_receipt_id) = kernel_with_source();
        let outcome = kernel
            .consolidate_surface_memory(
                "local_tenant",
                "human:proposer",
                "pages",
                "company_memory",
                &source_receipt_id,
                "Published page announcing the beta window",
                "unadjudicated_verbatim",
            )
            .expect("pending consolidation");
        let report = kernel
            .ratify_memory_consolidation(MemoryConsolidationRatification {
                tenant_id: "local_tenant",
                actor_id: "human:reviewer",
                memory_id: &outcome.memory_id,
                decision: "decline",
                note: "not a durable company fact",
            })
            .expect("decline ratification");
        assert_eq!(report.consolidation_state, MEMORY_CONSOLIDATION_DECLINED);
        let record = kernel
            .memory_records()
            .iter()
            .find(|record| record.memory_id == outcome.memory_id)
            .expect("declined record stays stored");
        assert_eq!(record.consolidation_state, MEMORY_CONSOLIDATION_DECLINED);
        assert!(
            kernel
                .memory_lifecycle_events()
                .iter()
                .any(|event| event.memory_id == outcome.memory_id
                    && event.lifecycle_state == "suppressed")
        );
    }

    #[test]
    fn private_user_scope_stays_auto_approved() {
        let (mut kernel, source_receipt_id) = kernel_with_source();
        let outcome = kernel
            .consolidate_surface_memory(
                "local_tenant",
                "human:proposer",
                "twin",
                "private_user_memory",
                &source_receipt_id,
                "User keeps Twin answers short",
                "unadjudicated_verbatim",
            )
            .expect("private consolidation");
        assert_eq!(outcome.consolidation_state, MEMORY_CONSOLIDATION_ACTIVE);
    }
}
