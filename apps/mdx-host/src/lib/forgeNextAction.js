// Forge's single operator-facing next move. This module only interprets
// projected truth: it never grants authority, invents readiness, or performs a
// write. Surfaces can share the result without growing their own status trees.

function encodedRunHref(run, surface = "runs") {
  const id = String(run?.runId ?? "").trim();
  return id ? `/forge/${surface}?run_id=${encodeURIComponent(id)}` : `/forge/${surface}`;
}

function firstRun(runs, predicate) {
  return (Array.isArray(runs) ? runs : []).find(predicate) ?? null;
}

export function forgeNextAction({ decisions = [], runs = [] } = {}) {
  const waitingDecisions = (Array.isArray(decisions) ? decisions : [])
    .filter((item) => item?.href)
    .slice()
    .sort((left, right) => Number(left?.priority ?? 999) - Number(right?.priority ?? 999));
  const waiting = waitingDecisions[0] ?? null;
  if (waiting) {
    const selectedCount = Math.max(1, Number(waiting.count) || 1);
    const totalWaiting = waitingDecisions.reduce(
      (total, item) => total + Math.max(1, Number(item?.count) || 1),
      0
    );
    const additionalWaiting = Math.max(0, totalWaiting - selectedCount);
    return {
      kind: "decision",
      eyebrow: "Needs you",
      label: String(waiting.label || "Make the waiting decision"),
      line: String(waiting.line || "Forge is holding at a boundary only you can cross."),
      href: String(waiting.href),
      cta: String(waiting.cta || "Open decision"),
      additionalWaiting,
      queueHref: "/needs-you",
      actionState: String(waiting.actionState || "review_required"),
      blockerCodes: Array.isArray(waiting.blockerCodes) ? waiting.blockerCodes : [],
      primary: true
    };
  }

  // Finished work gets the bounded human judgment before ordinary recovery.
  // A future projected urgency can still promote a critical intervention.
  const review = firstRun(runs, (run) =>
    run?.operatorStatus === "ready_for_review" ||
    (["done", "finished"].includes(run?.status) && Boolean(run?.branch))
  );
  if (review) {
    return {
      kind: "review",
      eyebrow: "Ready to review",
      label: String(review.runTitle || "A branch is ready for your call"),
      line: "Read the change and its proof, then decide whether it ships or goes back for revision.",
      href: encodedRunHref(review, "review"),
      cta: "Review branch",
      primary: true
    };
  }

  const attention = firstRun(runs, (run) =>
    run?.operatorStatus === "needs_you" ||
    ["cannot_proceed", "budget_exhausted", "error", "stopped"].includes(run?.status)
  );
  if (attention) {
    return {
      kind: "recovery",
      eyebrow: "Needs direction",
      label: String(attention.runTitle || "A build stopped short"),
      line: "See where it stopped, then narrow the work or ask for a focused revision.",
      href: encodedRunHref(attention),
      cta: "Pick it back up",
      primary: true
    };
  }

  const working = firstRun(runs, (run) =>
    run?.operatorStatus === "working" || run?.status === "running"
  );
  if (working) {
    return {
      kind: "working",
      eyebrow: "Working now",
      label: String(working.runTitle || "Forge is building"),
      line: "Follow the translated trail while the branch and proof take shape.",
      href: encodedRunHref(working),
      cta: "Follow the work",
      primary: true
    };
  }

  return {
    kind: "start",
    eyebrow: "Ready",
    label: "Hand Forge a scoped change",
    line: "Describe the outcome and the check that proves it. Forge works in isolation and brings back a branch.",
    href: "#start-build",
    cta: "Describe the work",
    primary: false
  };
}

export function isComposerPrimary(action) {
  return !action || action.kind === "start";
}
