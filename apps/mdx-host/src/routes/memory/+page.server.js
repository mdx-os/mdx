// Memory home: the one place to see what MDx has learned. It reads three
// receipt-backed rails and never pretends they are one brain - because in
// the kernel they are not yet joined. Everything here is derived from ledger
// projections (activated lessons, consolidation receipts, run-scored models)
// plus the durable record projection, so active private and legacy memories
// stay visible across a restart without being mislabeled as ratified.
// Fail-soft: an unreachable kernel renders honest empty rails.

import { memorySurfaceRecords } from "$lib/memorySurfaceRecords.js";
import { actorDisplayName } from "$lib/actorPresentation.js";

async function readJson(fetchImpl, path) {
  try {
    const response = await fetchImpl(`/api/kernel${path}`, {
      signal: AbortSignal.timeout(1500)
    });
    return response.ok ? await response.json() : null;
  } catch (error) {
    return null;
  }
}

// Shared-scope memory carries a scope, not a surface name. Map each scope to
// the surface a person would recognize it by, so the rail reads in human
// terms instead of kernel enums.
export async function load({ fetch, parent }) {
  const [
    { session },
    activationProjection,
    grantProjection,
    consolidationProjection,
    durableRecords,
    scorecard
  ] = await Promise.all([
    parent(),
    // What a human promoted and activated: learning.memory.activated receipts.
    // The projection already folds supersedes, so retired lessons carry a
    // superseded state instead of vanishing.
    readJson(fetch, "/learning/memory-activations/projection.json"),
    // Which of those lessons a person let steer fleet casting, each a
    // reversible learning.adaptation.granted receipt.
    readJson(fetch, "/learning/adaptation-grants/projection.json"),
    // What each surface wrote down: memory.surface_event.consolidated
    // receipts. Shared scopes land pending until a second person clears them.
    readJson(fetch, "/memory/consolidation-ratifications/projection.json"),
    // Active private memory and records created under the earlier local
    // review contract are durable but are neither pending nor ratified.
    readJson(fetch, "/memory/records.json"),
    // What the runs proved about the models: the per-work-type done rates the
    // planner actually casts on, derived from forge.run.event receipts.
    readJson(fetch, "/forge/model-scorecard.json")
  ]);

  const activeMemories = Array.isArray(activationProjection?.active_memories)
    ? activationProjection.active_memories
    : [];
  const grants = Array.isArray(grantProjection?.grants) ? grantProjection.grants : [];
  const openGrants = grants.filter((grant) => grant.open);

  // Rail 1: lessons a person kept, in the flywheel voice. Retired lessons stay
  // in the record but drop off the "now guiding work" list.
  const activatedLessons = activeMemories
    .filter((memory) => memory.active_memory_state !== "superseded")
    .map((memory) => ({
      receiptId: memory.active_memory_receipt_id,
      summary: String(memory.lesson_summary ?? "").replace(/^candidate lesson:\s*/i, "").trim(),
      targetType: String(memory.target_type ?? ""),
      reviewOwner: String(memory.review_owner ?? ""),
      activationBasis: String(memory.activation_basis ?? ""),
      rollbackPlan: String(memory.rollback_plan ?? ""),
      promotionReceiptId: String(memory.memory_promotion_receipt_id ?? ""),
      judgmentReceiptId: String(memory.judgment_decision_receipt_id ?? ""),
      // The open casting grant for this lesson, if a person opened one. Its
      // presence is the honest signal that this lesson steered casting.
      steersCasting: openGrants.some(
        (grant) => grant.activation_receipt_id === memory.active_memory_receipt_id
      )
    }));
  const retiredLessonCount = activeMemories.filter(
    (memory) => memory.active_memory_state === "superseded"
  ).length;

  // Rail 2: what each surface recorded, grouped by the surface a person knows
  // it by, honest about the pending-review state of shared notes.
  const pending = Array.isArray(consolidationProjection?.pending)
    ? consolidationProjection.pending
    : [];
  const ratifications = Array.isArray(consolidationProjection?.ratifications)
    ? consolidationProjection.ratifications
    : [];
  const records = Array.isArray(durableRecords?.records) ? durableRecords.records : [];
  const surfaceRecords = memorySurfaceRecords({ pending, ratifications, records }).map((group) => ({
    ...group,
    waiting: group.waiting.map((record) => ({
      ...record,
      proposedByLabel: actorDisplayName({
        actorId: record.proposedBy,
        displayName: record.proposedBy,
        selfActorId: session?.user_id,
        actorType: "human"
      })
    }))
  }));
  const pendingReviewCount = pending.length;

  // Rail 3: what the runs proved about the models. by_work_type is the fair
  // read the planner casts on; keep it in that order and say so.
  const byWorkType = Array.isArray(scorecard?.by_work_type) ? scorecard.by_work_type : [];
  const modelProof = byWorkType
    .map((row) => ({
      model: String(row.model ?? ""),
      workType: String(row.work_type ?? ""),
      runs: Number(row.runs ?? 0),
      done: Number(row.done ?? 0),
      doneRate: Number(row.done_rate ?? 0),
      avgTurns: Number(row.avg_turns ?? 0)
    }))
    .sort((a, b) => b.doneRate - a.doneRate || b.runs - a.runs)
    .slice(0, 12);

  return {
    session,
    activatedLessons,
    retiredLessonCount,
    surfaceRecords,
    pendingReviewCount,
    modelProof,
    kernelReachable: Boolean(
      activationProjection || consolidationProjection || durableRecords || scorecard
    )
  };
}
