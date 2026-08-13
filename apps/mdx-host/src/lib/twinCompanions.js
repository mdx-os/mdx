import grounding from "../../../../generated/companions/twin-text-grounding-contract.json";

// The built-in companions come from the generated grounding contract; the chat
// meta (display name, stance, opener) is the product voice on top of it.
const chatMeta = {
  twin_advisor: {
    name: "Strategic Advisor",
    stance: "Direct, options-first",
    shape: "weigh a decision against what the company has actually done",
    opener: "Should we build or buy this capability?"
  },
  twin_architect: {
    name: "Systems Architect",
    stance: "Structured, tradeoff-led",
    shape: "reason about system shape, boundaries, and failure modes",
    opener: "Walk me through the risks in this design."
  },
  twin_coder: {
    name: "Coder",
    stance: "Practical, proof-first",
    shape: "turn code work into a plan, checks, and a safe Forge handoff",
    opener: "Help me shape this coding task for Forge."
  },
  twin_coach: {
    name: "Leadership Coach",
    stance: "Warm, question-led",
    shape: "prepare for a hard conversation or a team decision",
    opener: "I have a tough feedback conversation coming up."
  },
  twin_compliance: {
    name: "Compliance Specialist",
    stance: "Precise, citation-first",
    shape: "check an obligation or boundary before acting",
    opener: "What are the OSFI B-13 implications for our AI agents?"
  },
  twin_problem_solver: {
    name: "Problem Solver",
    stance: "Calm, step-by-step",
    shape: "untangle a stuck problem into a next move",
    opener: "Help me think through a team restructure."
  }
};

export const companions = grounding.companions.map((companion) => {
  const meta = chatMeta[companion.id] ?? {
    name: companion.role,
    stance: "Grounded",
    shape: "answer from saved proof",
    opener: "What should I look at first?"
  };
  return {
    id: companion.id,
    role: companion.role,
    autonomyCeiling: companion.autonomy_ceiling,
    groundingSources: companion.grounding_sources ?? [],
    forbiddenActions: companion.forbidden_actions ?? [],
    ...meta
  };
});

export const defaultCompanionId = "twin_advisor";

// Each companion gets a stable color identity - the quiet visual answer
// to "who am I talking to right now". The five built-ins have chosen
// hues; custom companions derive one from their id, so the same
// companion always wears the same color, everywhere, on both themes.
const builtinHues = {
  twin_advisor: 222,
  twin_architect: 258,
  twin_coder: 148,
  twin_coach: 36,
  twin_compliance: 172,
  twin_problem_solver: 340
};

export function companionHue(id) {
  if (id in builtinHues) {
    return builtinHues[id];
  }
  let hash = 0;
  for (const char of String(id ?? "")) {
    hash = (hash * 31 + char.charCodeAt(0)) % 360;
  }
  return hash;
}

export function companionColor(id) {
  return `oklch(0.62 0.14 ${companionHue(id)})`;
}
