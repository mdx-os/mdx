export const humanJobs = [
  {
    id: "what_is_happening",
    question: "What is happening?",
    humanNeed: "See the current company state without reading raw logs.",
    primarySurface: "observatory"
  },
  {
    id: "can_i_trust_it",
    question: "Can I trust it?",
    humanNeed: "Understand the proof, blockers, and local-vs-live boundary.",
    primarySurface: "evidence"
  },
  {
    id: "what_needs_me",
    question: "What needs me?",
    humanNeed: "Find exceptions, ratifications, and bounded authority decisions.",
    primarySurface: "observatory"
  },
  {
    id: "what_should_i_do_next",
    question: "What should I do next?",
    humanNeed: "Get a safe next step without giving the system write authority.",
    primarySurface: "concierge"
  }
];

export const appRoles = [
  {
    id: "observatory",
    role: "Shows what MDx is doing.",
    boundary: "Read-only operating view over saved proof.",
    primaryJob: "what_is_happening"
  },
  {
    id: "concierge",
    role: "Answers from saved proof.",
    boundary: "Grounded answers only, no write controls.",
    primaryJob: "what_should_i_do_next"
  },
  {
    id: "twin",
    role: "Helps you reason through the proof.",
    boundary: "Read-only companion guidance, no action-taking.",
    primaryJob: "can_i_trust_it"
  },
  {
    id: "evidence",
    role: "Explains why it can be trusted.",
    boundary: "Saved evidence, routes, contracts, and generated files.",
    primaryJob: "can_i_trust_it"
  }
];

export const appExperienceUpgrade = [
  {
    id: "pages",
    surface: "Pages",
    role: "Knowledge app for builders and operators.",
    currentMode: "World Model projection over saved-proof documents.",
    upgradeNow: "Browse onboarding, doctrine, ADR paths, design guidance, and operating evidence without reading the whole repo first.",
    stillForbidden: "No standalone document store, rich editor, or uncited page mutation.",
    firstSlice: "Developer onboarding library with evidence-backed page cards."
  },
  {
    id: "message",
    surface: "Message",
    role: "Agent-first communication app.",
    currentMode: "Read-only thread and channel projections over ordered events.",
    upgradeNow: "Show agent communication, handoffs, decisions, and crosslinks as human-readable threads.",
    stillForbidden: "No shadow message database, realtime write path, or LLM call on the hot path.",
    firstSlice: "Thread reader with agent/human lanes and Pages crosslinks."
  },
  {
    id: "twin",
    surface: "Twin",
    role: "Grounded companion app.",
    currentMode: "Read-only companions over the same proof packet as Concierge.",
    upgradeNow: "Let humans type, route the question to a companion, and preview grounded guidance locally.",
    stillForbidden: "No tool execution, hidden context change, live answer run, spend, worker start, or deploy.",
    firstSlice: "Read-only conversation preview with companion routing."
  }
];

export const interactionKinds = [
  {
    id: "navigate",
    label: "Move to a surface",
    authority: "read_only"
  },
  {
    id: "inspect",
    label: "Inspect saved proof",
    authority: "read_only"
  },
  {
    id: "ask",
    label: "Ask for a grounded answer",
    authority: "read_only"
  },
  {
    id: "reason",
    label: "Reason with a companion",
    authority: "read_only"
  },
  {
    id: "disclose",
    label: "Reveal evidence in place",
    authority: "read_only"
  }
];

export const primaryActions = [
  {
    label: "Inspect evidence",
    href: "#observatory",
    surface: "observatory",
    kind: "inspect",
    intent: "Inspect the saved proof before making a decision."
  },
  {
    label: "Ask Concierge",
    href: "http://127.0.0.1:5174",
    surface: "concierge",
    kind: "ask",
    intent: "Ask a grounded question without giving write authority."
  },
  {
    label: "Open Twin",
    href: "http://127.0.0.1:5176",
    surface: "twin",
    kind: "reason",
    intent: "Reason through the proof with a read-only companion."
  },
  {
    label: "Review next work",
    href: "#next-work",
    surface: "observatory",
    kind: "inspect",
    intent: "Jump to the queue-backed next safe work item."
  }
];

export const surfaceChrome = {
  themeModes: [
    { id: "system", label: "System", href: "?theme=system" },
    { id: "dark", label: "Dark", href: "?theme=dark" },
    { id: "light", label: "Light", href: "?theme=light" }
  ],
  observatory: {
    brand: "Native Console",
    nav: [
      { label: "Home", href: "#home", meta: "local console" },
      { label: "Observatory", href: "#observatory", meta: "evidence" },
      { label: "Forge", href: "http://127.0.0.1:5177", meta: "build path" },
      { label: "Concierge", href: "http://127.0.0.1:5174", meta: "answers" },
      { label: "Twin", href: "http://127.0.0.1:5176", meta: "companions" },
      { label: "Pages", href: "#pages", meta: "knowledge" },
      { label: "Message", href: "#message", meta: "threads" },
      { label: "Evidence", href: "#evidence", meta: "proof" }
    ]
  },
  concierge: {
    brand: "Native Console",
    nav: [
      { label: "Home", href: "http://127.0.0.1:5175", meta: "local console" },
      { label: "Forge", href: "http://127.0.0.1:5177", meta: "build path" },
      { label: "Concierge", href: "#home", meta: "answers" },
      { label: "Twin", href: "http://127.0.0.1:5176", meta: "companions" },
      { label: "Pages", href: "#pages", meta: "knowledge" },
      { label: "Strategy", href: "#strategy", meta: "direction" },
      { label: "Product", href: "#product", meta: "ratification" },
      { label: "Evidence", href: "#evidence", meta: "receipts" }
    ]
  },
  twin: {
    brand: "Native Console",
    nav: [
      { label: "Home", href: "http://127.0.0.1:5175", meta: "local console" },
      { label: "Forge", href: "http://127.0.0.1:5177", meta: "build path" },
      { label: "Concierge", href: "http://127.0.0.1:5174", meta: "answers" },
      { label: "Twin", href: "#home", meta: "companions" },
      { label: "Companions", href: "#companions", meta: "roles" },
      { label: "Proofs", href: "#proofs", meta: "grounding" },
      { label: "Routes", href: "#routes", meta: "read routes" },
      { label: "Evidence", href: "#evidence", meta: "sources" }
    ]
  },
  forge: {
    brand: "Native Console",
    nav: [
      { label: "Home", href: "http://127.0.0.1:5175", meta: "local console" },
      { label: "Forge", href: "#home", meta: "build path" },
      { label: "Observatory", href: "http://127.0.0.1:5175", meta: "evidence" },
      { label: "Concierge", href: "http://127.0.0.1:5174", meta: "answers" },
      { label: "Twin", href: "http://127.0.0.1:5176", meta: "companions" },
      { label: "Stages", href: "#stages", meta: "path" },
      { label: "Thread", href: "#thread", meta: "work conversation" },
      { label: "Evidence", href: "#evidence", meta: "proof" }
    ]
  }
};

export const operatingStory = {
  eyebrow: "Today in MDx",
  headline: "The local company loop ran, saved proof, and found one thing that still needs judgment.",
  summary:
    "MDx is already useful as a local operating view: it can show what ran, what changed, what is trustworthy, and where a human should step in before live authority expands."
};

export const todayInMdx = [
  {
    step: "What happened",
    title: "Six loops produced a local operating picture.",
    body: "Evals, Aegis, Charter, Forge, Product, and Talent are visible through the same read-only console.",
    signal: "Work is no longer hidden in logs."
  },
  {
    step: "Why it matters",
    title: "Proof is being saved before live work expands.",
    body: "Postgres is working locally, while Temporal, TensorZero, mem0, OpenTelemetry, and Render remain held behind observed proof.",
    signal: "The system is useful without pretending to be fully live."
  },
  {
    step: "Human judgment",
    title: "One local proof item still needs review.",
    body: "The safe move is to inspect the missing proof, ask Concierge for a grounded answer, or use Twin to reason through the decision.",
    signal: "Humans stay at the edge."
  },
  {
    step: "Safe next",
    title: "Read first. Act later.",
    body: "No screen grants write authority. The next experience work should make the proof easier to understand before adding live controls.",
    signal: "Autonomy stays bounded."
  }
];

export const conciergeAskExperience = {
  eyebrow: "Ask Concierge",
  headline: "Ask a grounded question. Get the answer, the caveat, and the next safe move.",
  summary:
    "Concierge is a read-only answer surface over saved proof. It should feel conversational, but it must always show what evidence it used and what it still cannot do.",
  prompt: "What should I look at before expanding live authority?",
  answerContract: [
    "Answer from saved proof",
    "Name what is missing",
    "Offer a safe next step",
    "Keep write authority out of the conversation"
  ]
};

export const suggestedQuestions = [
  {
    question: "Can I trust this local run?",
    answer: "Trust the local proof when the handoff is current, then keep live authority behind observed proof.",
    safeNext: "Open the evidence trail before expanding authority.",
    href: "#evidence"
  },
  {
    question: "What needs a human right now?",
    answer: "The next judgment point is which held-back live connection deserves human approval first.",
    safeNext: "Review the blockers before choosing a live connection path.",
    href: "#dogfood"
  },
  {
    question: "What is still held back?",
    answer: "Temporal, TensorZero, mem0, OpenTelemetry, and Render still require observed live proof.",
    safeNext: "Keep live actions blocked until observed proof is present.",
    href: "#live"
  }
];

export const twinCompanionExperience = {
  eyebrow: "Choose a companion",
  headline: "Five ways to think with the company, all read-only and grounded.",
  summary:
    "Twin should feel like a calm advisory table. Each companion has a clear job, a safe opening question, and the same hard boundary: it can reason with proof while live conversation and action wait for governed rails."
};

export const companionPrompts = [
  {
    id: "twin_advisor",
    name: "Advisor",
    humanUse: "Prioritize the next decision.",
    openingQuestion: "What should I look at first?",
    responseStyle: "Direct, practical, trade-off aware",
    accent: "var(--mdx-focus-blue)"
  },
  {
    id: "twin_architect",
    name: "Architect",
    humanUse: "Stress-test the system shape.",
    openingQuestion: "Where could this design drift?",
    responseStyle: "Structural, concise, evidence-led",
    accent: "var(--mdx-evidence-cyan)"
  },
  {
    id: "twin_coach",
    name: "Coach",
    humanUse: "Keep momentum without rushing judgment.",
    openingQuestion: "What is the calm next move?",
    responseStyle: "Steady, humane, bounded",
    accent: "var(--mdx-success-green)"
  },
  {
    id: "twin_compliance",
    name: "Compliance",
    humanUse: "Check the rails before live authority grows.",
    openingQuestion: "What rule would block this?",
    responseStyle: "Precise, conservative, citation-first",
    accent: "var(--mdx-warning-amber)"
  },
  {
    id: "twin_problem_solver",
    name: "Problem Solver",
    humanUse: "Unstick a blocked operating path.",
    openingQuestion: "What is the smallest unblock?",
    responseStyle: "Diagnostic, pragmatic, stepwise",
    accent: "var(--mdx-danger-red)"
  }
];

export const proofDepthTransitions = {
  observatory: {
    eyebrow: "Proof details below",
    headline: "The first screen is for judgment. The next section is for inspection.",
    body: "Keep scanning if you want the raw operating picture, receipt counts, route proofs, and projection details."
  },
  concierge: {
    eyebrow: "Proof details below",
    headline: "The answer comes first. The citations come next.",
    body: "Use the sections below when you need to check the exact local run, local proof, live boundary, or saved evidence."
  },
  twin: {
    eyebrow: "Proof details below",
    headline: "Choose the companion first. Inspect the grounding second.",
    body: "The panels below show the local run, companion runtime status, routes, and shared evidence that bound Twin's advice."
  }
};

export const consoleSurfaceLauncher = [
  {
    label: "Watch",
    surface: "Observatory",
    promise: "See what the company is doing now.",
    href: "#observatory"
  },
  {
    label: "Ask",
    surface: "Concierge",
    promise: "Get a grounded answer from saved proof.",
    href: "http://127.0.0.1:5174"
  },
  {
    label: "Think",
    surface: "Twin",
    promise: "Reason with a read-only companion.",
    href: "http://127.0.0.1:5176"
  },
  {
    label: "Inspect",
    surface: "Evidence",
    promise: "Open saved evidence and route proofs.",
    href: "#evidence"
  }
];
