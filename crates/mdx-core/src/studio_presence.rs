use crate::*;

// Multiplayer presence on a Studio run: who is working a run right now. Like
// Message presence, this is receipt-derived and realtime is not promised - the
// roster is read from the ledger in order, never a live socket. Presence is NOT
// co-edit: it names who is on the run, it does not open a second write path to
// the document body, so the steering contract's live_co_edit_allowed=false holds.
//
// The roster comes from two honest signals tied to a studio_run_id:
//   - lifecycle receipts the operator already produced (opening, pausing,
//     steering, resuming, landing) carry the actor, so anyone who has worked the
//     run appears without a new write;
//   - an explicit studio.presence.marked receipt lets a second operator signal
//     "I'm on this run" without having to act on it yet.
// The most recent actors are marked active; the rest are around.

/// How many of the most recently seen actors count as active now.
pub const STUDIO_PRESENCE_ACTIVE_RECENT: usize = 3;
/// Bound on the roster so a long-lived run cannot grow an unbounded list.
pub const STUDIO_PRESENCE_ROSTER_LIMIT: usize = 16;

/// An explicit presence signal recorded against a run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudioPresenceMark<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub run_id: &'a str,
    pub operator_id: &'a str,
    /// A free label for why they are here (reviewing, drafting, watching). Empty
    /// is fine; it is recorded as-is for the record, never interpreted.
    pub presence_scope: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudioPresenceReport {
    pub status: &'static str,
    pub run_id: String,
    pub receipt_id: String,
    pub present_count: usize,
}

/// One actor on a run, with whether they are active now (recently seen).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudioPresenceEntry {
    pub actor_id: String,
    pub active: bool,
}

impl<S: StorageProvider> MdxKernel<S> {
    /// Record that an operator is present on a run. Refused if the run is not
    /// open, so presence never accretes against a run that does not exist.
    pub fn mark_studio_presence(
        &mut self,
        mark: StudioPresenceMark<'_>,
    ) -> Result<StudioPresenceReport, StudioRunError> {
        for (field, value) in [
            ("tenant_id", mark.tenant_id),
            ("actor_id", mark.actor_id),
            ("run_id", mark.run_id),
            ("operator_id", mark.operator_id),
        ] {
            if value.trim().is_empty() {
                return Err(StudioRunError::Missing(field));
            }
        }
        if self.studio_run_state(mark.run_id) == StudioRunState::Unknown {
            return Err(StudioRunError::UnknownRun(mark.run_id.to_string()));
        }
        self.studio_admit(mark.tenant_id, mark.actor_id, mark.run_id)?;
        let receipt = self.emit_studio_control_receipt(
            mark.tenant_id,
            mark.actor_id,
            "STUDIO_PRESENCE_MARKED",
            "studio.presence.marked",
            payload(&[
                ("studio_run_id", mark.run_id),
                ("operator_id", mark.operator_id),
                ("presence_scope", mark.presence_scope),
            ]),
        );
        let present_count = self.studio_run_presence(mark.run_id).len();
        Ok(StudioPresenceReport {
            status: "STUDIO_PRESENCE_MARKED",
            run_id: mark.run_id.to_string(),
            receipt_id: receipt.receipt_id,
            present_count,
        })
    }

    /// The roster of actors on a run, most-recent first, derived from the run's
    /// receipts in ledger order. The first STUDIO_PRESENCE_ACTIVE_RECENT are
    /// marked active; the roster is bounded by STUDIO_PRESENCE_ROSTER_LIMIT.
    pub fn studio_run_presence(&self, run_id: &str) -> Vec<StudioPresenceEntry> {
        // actor_id -> the latest ledger index at which we saw them on this run.
        let mut latest: Vec<(String, usize)> = Vec::new();
        for (index, receipt) in self.ledger().entries().iter().enumerate() {
            if receipt.payload.get("studio_run_id").map(String::as_str) != Some(run_id) {
                continue;
            }
            let actor = receipt
                .payload
                .get("operator_id")
                .or_else(|| receipt.payload.get("identity_subject_actor_id"))
                .map(String::as_str)
                .filter(|value| !value.is_empty());
            let Some(actor) = actor else { continue };
            match latest.iter_mut().find(|(existing, _)| existing == actor) {
                Some(entry) => entry.1 = index,
                None => latest.push((actor.to_string(), index)),
            }
        }
        // Most recently seen first.
        latest.sort_by_key(|(_, index)| std::cmp::Reverse(*index));
        latest
            .into_iter()
            .take(STUDIO_PRESENCE_ROSTER_LIMIT)
            .enumerate()
            .map(|(rank, (actor_id, _))| StudioPresenceEntry {
                actor_id,
                active: rank < STUDIO_PRESENCE_ACTIVE_RECENT,
            })
            .collect()
    }
}
