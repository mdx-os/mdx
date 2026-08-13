// Strategy: the company's direction surface. Translates the kernel's
// ratification packet into human language - what is decided and on the
// record, what is still open, what stays on hold until a call is made,
// and where in the process the company stands right now. All
// token-to-prose translation lives here so the page stays calm. The
// kernel packet is runtime truth read through the proxy; this helper is
// pure translation over whatever the packet contains (or doesn't).

const OPTION_COPY = {
  hold_current_direction: {
    title: "Hold the current direction",
    line: "Keep going as we are. Nothing new is committed; we wait for stronger signal before changing course.",
    weight: "Steady"
  },
  request_more_evidence: {
    title: "Ask for more evidence first",
    line: "Hold the call open and gather more proof from the loops before anyone commits to a direction.",
    weight: "Patient"
  },
  ratify_next_local_strategy_option: {
    title: "Decide on the next move",
    line: "Put a direction on the record so the budget, build, and team paths can open behind it.",
    weight: "Committing"
  }
};

const BLOCKED_COPY = {
  set_company_direction: "Setting the company's direction",
  allocate_live_budget: "Putting real money behind a bet",
  spawn_worker: "Hiring a worker to do the job",
  handoff_to_forge: "Handing a build to Forge",
  production_deploy: "Shipping to production"
};

function titleCaseToken(token) {
  return String(token)
    .replace(/[._-]+/g, " ")
    .replace(/\b\w/g, (ch) => ch.toUpperCase());
}

function humanToken(token) {
  return String(token).replace(/[._-]+/g, " ").trim();
}

// The motion rail: a small, honest sense of where this direction stands in
// the company's own process. It is derived purely from real packet state -
// never a fixed animation. A proposal exists, options are staged; evidence
// is the live step when no receipts back it yet; ratification is the human
// call still ahead; the paths open only once a direction is on the record.
function motionFrom({ hasProposal, hasEvidence, decided }) {
  const stages = [
    {
      key: "staged",
      label: "Options staged",
      cue: "The open question and its choices are on the table.",
      state: hasProposal ? "done" : "ahead"
    },
    {
      key: "evidence",
      label: "Evidence gathering",
      cue: "Proof from the loops is being collected to weigh the choices.",
      state: decided || hasEvidence ? "done" : "active"
    },
    {
      key: "ratified",
      label: "Human ratification",
      cue: "A person decides, on the record. This is the call the company waits for.",
      state: decided ? "done" : hasEvidence ? "active" : "ahead"
    },
    {
      key: "open",
      label: "Paths open",
      cue: "Budget, hiring, builds, and deploys can move once a direction is decided.",
      state: decided ? "active" : "ahead"
    }
  ];
  const activeIndex = stages.findIndex((stage) => stage.state === "active");
  return { stages, activeIndex: activeIndex === -1 ? stages.length - 1 : activeIndex };
}

// proposal: the latest human-authored direction proposal (from the
// direction-proposals projection). When one exists it IS the open
// question - the person's words replace the fixture - and decided is
// judged against that proposal alone, so a fresh proposal reopens the
// room even after an older call was ratified.
export function directionFrom(packet, decisions = null, proposal = null) {
  if (!packet || typeof packet !== "object") {
    return null;
  }

  const optionTokens = Array.isArray(packet.options) ? packet.options : [];
  const options = optionTokens.map((token) => {
    const copy = OPTION_COPY[token];
    return {
      token,
      title: copy ? copy.title : titleCaseToken(token),
      line: copy ? copy.line : null,
      weight: copy ? copy.weight : null
    };
  });

  const blockedTokens = Array.isArray(packet.blocked_actions) ? packet.blocked_actions : [];
  const blocked = blockedTokens.map((token) => ({
    token,
    label: BLOCKED_COPY[token] || titleCaseToken(token)
  }));

  // The packet's receipt_ids are loop evidence (harness runs, gateway
  // observations) - proof that work happened, never proof that a human
  // decided anything. They back the evidence stage only.
  const receiptIds = Array.isArray(packet.receipt_ids)
    ? packet.receipt_ids.filter(Boolean)
    : [];
  const evidenceReceiptIds = Array.isArray(packet.evidence_receipt_ids)
    ? packet.evidence_receipt_ids.filter(Boolean)
    : [];
  const sourceContracts = Array.isArray(packet.source_contracts)
    ? packet.source_contracts.filter(Boolean)
    : [];

  // A direction is "decided, on the record" only when a human ratification
  // decision exists in the decisions projection - and only the committing
  // option counts. "Hold" and "more evidence first" are real calls on the
  // record, but they leave the question open, so the decision room stays up.
  const decisionRecords = Array.isArray(decisions?.decisions)
    ? decisions.decisions.filter(Boolean)
    : [];
  const authoredId =
    typeof proposal?.proposal_id === "string" && proposal.proposal_id.length > 0
      ? proposal.proposal_id
      : null;
  const ratifyingRecords = decisionRecords.filter(
    (record) =>
      record.decision === "ratify_next_local_strategy_option" &&
      (!authoredId || record.proposal_id === authoredId)
  );
  const decided = ratifyingRecords.length > 0;
  const decisionReceiptIds = ratifyingRecords
    .map((record) => record.decision_receipt_id)
    .filter(Boolean);

  const hasProposal =
    authoredId !== null ||
    (typeof packet.proposal_id === "string" && packet.proposal_id.length > 0);
  const ratificationRequired = packet.ratification_required !== false;

  const motion = motionFrom({
    hasProposal,
    hasEvidence: evidenceReceiptIds.length > 0 || receiptIds.length > 0,
    decided
  });

  return {
    proposalId: authoredId ?? (hasProposal ? packet.proposal_id : null),
    authored: authoredId !== null,
    question: authoredId
      ? proposal.direction
      : typeof packet.question === "string"
        ? packet.question
        : null,
    whyNow: authoredId
      ? proposal.why_now || null
      : typeof packet.why_now === "string"
        ? packet.why_now
        : null,
    options,
    blocked,
    receiptIds,
    evidenceReceiptIds,
    decisionReceiptIds,
    sourceContracts,
    decided,
    ratificationRequired,
    motion,
    rawStatus: typeof packet.runtime_status === "string" ? packet.runtime_status : null
  };
}

export { humanToken };
