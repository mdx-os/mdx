// Operating a long-horizon mission, write-side. The web can shape and launch a
// mission; these helpers let the operator also drive it: pause it, resume it,
// steer the crew with a note, and mark a milestone started, passed, or blocked.
// Every verb posts one checkpoint to /forge/long-horizon-mission-checkpoints.json;
// the kernel is fail-closed, so a refusal comes back as a refusal and we surface
// it honestly. Nothing here invents state - each label is a function of what the
// checkpoint receipts say.

// The checkpoint events the kernel accepts. Milestone events carry a milestone;
// mission events act on the whole mission. Kept in one place so the UI and the
// payload builder never drift from the kernel's allow-list.
export const CHECKPOINT_EVENTS = {
  milestone_started: { requiresMilestone: true },
  milestone_completed: { requiresMilestone: true },
  milestone_blocked: { requiresMilestone: true },
  mission_paused: { requiresMilestone: false },
  mission_resumed: { requiresMilestone: false },
  mission_steered: { requiresMilestone: false }
};

// Build the POST body for one checkpoint. A milestone event without a milestone
// id would be refused by the kernel, so we surface that as an error instead of
// sending a doomed request. A steering note rides both `summary` (the human
// line every checkpoint records) and `steering_note` (what the running loop
// reads between turns).
export function buildCheckpointPayload({ missionId, event, milestoneId = "", note = "", actor = "local_user" }) {
  const spec = CHECKPOINT_EVENTS[event];
  if (!spec) return { error: `Unknown checkpoint: ${event}` };
  const mission = String(missionId ?? "").trim();
  if (!mission) return { error: "This mission has no id to check point yet." };
  const milestone = String(milestoneId ?? "").trim();
  if (spec.requiresMilestone && !milestone) {
    return { error: "Pick the milestone this applies to first." };
  }
  const trimmedNote = String(note ?? "").trim();
  if (event === "mission_steered" && !trimmedNote) {
    return { error: "Leave a note for the crew before you steer." };
  }
  const body = { mission_id: mission, checkpoint_event: event, actor_id: actor };
  if (milestone) body.milestone_id = milestone;
  if (trimmedNote) {
    body.summary = trimmedNote;
    if (event === "mission_steered") body.steering_note = trimmedNote;
  }
  return { body };
}

// A checkpoint response is a success unless the kernel refused it. The route
// returns event-specific statuses (MISSION_STEERED, MISSION_PAUSED_FOR_OPERATOR,
// MISSION_MILESTONE_COMPLETED, ...) rather than a single RECORDED, so we treat
// any non-REFUSED status carrying a receipt as landed.
export function checkpointLanded(packet) {
  if (!packet) return false;
  const status = String(packet.status ?? "");
  if (!status || status === "REFUSED") return false;
  return Boolean(packet.checkpoint_receipt_id) || status.startsWith("MISSION_");
}

// The mission's overall state as one calm human label, from the packet's
// mission_state. Unknown states fall back to a readable form of the raw token.
const MISSION_STATE = {
  ADMITTED_WAITING_FOR_LOCAL_EXECUTION: { label: "Ready to run", tone: "shaping" },
  IN_PROGRESS_LOCAL_CHECKPOINTS: { label: "In progress", tone: "active" },
  PAUSED_FOR_OPERATOR: { label: "Paused", tone: "warn" },
  BLOCKED_NEEDS_OPERATOR: { label: "Needs you", tone: "warn" },
  COMPLETED_LOCAL_CHECKPOINTS: { label: "Done", tone: "ok" }
};

export function missionState(state) {
  const key = String(state ?? "").trim();
  if (MISSION_STATE[key]) return MISSION_STATE[key];
  if (!key) return { label: "", tone: "" };
  return { label: key.toLowerCase().replace(/_/g, " "), tone: "" };
}

// Whether the mission is paused right now, from its latest checkpoint. Drives
// which of pause / resume the header offers.
export function isMissionPaused(mission) {
  return (
    String(mission?.missionState ?? "") === "PAUSED_FOR_OPERATOR" ||
    String(mission?.latestCheckpointEvent ?? "") === "mission_paused"
  );
}

// The latest steering or checkpoint note, phrased as a short human line for the
// header. Empty when nothing has been recorded yet.
export function latestCheckpointLine(mission) {
  const summary = String(mission?.latestCheckpointSummary ?? "").trim();
  const event = String(mission?.latestCheckpointEvent ?? "").trim();
  if (!summary && !event) return "";
  const verb = {
    mission_steered: "Last note to the crew",
    mission_paused: "Paused",
    mission_resumed: "Resumed",
    milestone_started: "Started a milestone",
    milestone_completed: "Marked a milestone passed",
    milestone_blocked: "Flagged a milestone"
  }[event] ?? "Last checkpoint";
  return summary ? `${verb}: ${summary}` : verb;
}
