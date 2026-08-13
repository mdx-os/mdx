// Intent routing: v1's zero-LLM keyword router, ported. Classifies a
// question to the right specialist in well under a millisecond, on this
// machine, before the send - so "Auto" can pick who answers and the
// switch markers make the routing visible. An explicit pick always wins;
// this never overrides a person's choice.
const ROUTES = [
  {
    id: "twin_coder",
    keywords: [
      "code", "coding", "coder", "implement", "implementation", "build this", "bug",
      "debug", "test", "tests", "unit test", "integration test", "refactor", "repository",
      "repo", "pull request", "pr", "branch", "diff", "compile", "cargo", "npm", "pytest",
      "swift", "typescript", "javascript", "python", "rust", "kotlin", "java", "dotnet",
      "api endpoint", "migration", "forge", "ship this change"
    ]
  },
  {
    id: "twin_coach",
    keywords: [
      // v1's list plus the misses live use surfaced on day one: "coach a
      // teammate" and "performance management" both routed to the
      // advisor default because only "coaching" and "performance review"
      // were listed.
      "feedback", "1:1", "one-on-one", "difficult conversation", "performance review",
      "performance management", "underperform", "team member", "teammate", "career",
      "coach", "coaching", "mentor", "conflict", "morale", "motivation",
      "retention", "hiring", "firing", "skip level", "direct report", "growth conversation",
      "not doing their work", "people side"
    ]
  },
  {
    id: "twin_architect",
    keywords: [
      "technology", "architecture", "build vs buy", "vendor", "platform", "infrastructure",
      "ai", "ml", "llm", "api", "framework", "database", "system design", "technical decision",
      "tech stack", "microservice", "kubernetes", "cloud", "aws", "gcp", "azure"
    ]
  },
  {
    id: "twin_advisor",
    keywords: [
      "org", "organization", "team structure", "domain", "strategy", "roadmap", "planning",
      "okr", "goal", "metric", "process", "workflow", "agile", "scrum", "leadership",
      "vision", "culture"
    ]
  },
  {
    id: "twin_problem_solver",
    keywords: [
      "solve", "problem", "figure out", "diagnose", "optimize", "stuck", "struggling",
      "issue", "trouble", "challenge", "root cause", "first principles", "break down",
      "analyze", "bottleneck", "blocker", "constraint", "simplify", "why is",
      "how do i fix", "what's wrong", "rethink", "improve"
    ]
  },
  {
    id: "twin_compliance",
    keywords: [
      "compliance", "regulation", "regulatory", "osfi", "hipaa", "phipa", "b-13", "e-21",
      "privacy", "security requirement", "audit", "encryption", "phi", "pii",
      "data protection", "breach", "can we store", "are we allowed", "is it compliant",
      "legal requirement", "gdpr", "pipeda", "fintrac", "aml", "kyc"
    ]
  }
];

export const autoCompanionId = "auto";

// Custom companions route by their own words: name and purpose terms
// score lower than the built-in domain keywords, so a custom companion
// wins only when the question clearly speaks its language.
function customScore(persona, message) {
  const words = `${persona.name} ${persona.purpose ?? ""}`
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter((word) => word.length > 3);
  return [...new Set(words)].reduce((score, word) => (message.includes(word) ? score + 1 : score), 0);
}

function scoreAll(text, customPersonas = []) {
  const message = String(text ?? "").toLowerCase();
  const scores = ROUTES.map((route) => ({
    id: route.id,
    score: route.keywords.reduce((sum, keyword) => (message.includes(keyword) ? sum + 2 : sum), 0)
  }));
  for (const persona of customPersonas) {
    scores.push({ id: persona.id, score: customScore(persona, message) });
  }
  return scores.sort((a, b) => b.score - a.score);
}

export function routeCompanion(text, customPersonas = []) {
  const [best] = scoreAll(text, customPersonas);
  // No strong signal: the Strategic Advisor takes general questions,
  // exactly v1's default.
  return {
    id: !best || best.score === 0 ? "twin_advisor" : best.id,
    confidence: Math.min((best?.score ?? 0) / 5, 1)
  };
}

// The panel: when a question genuinely spans two domains (both score
// strongly and the runner-up is close), BOTH specialists take it -
// v1's parallel executor concept without the orchestration LLM. The
// router's own scores already know when a question has two homes.
export function routePanel(text, customPersonas = []) {
  const scores = scoreAll(text, customPersonas);
  const [best, second] = scores;
  if (!best || best.score === 0) {
    return ["twin_advisor"];
  }
  if (second && second.score >= 4 && second.score >= best.score * 0.6) {
    return [best.id, second.id];
  }
  return [best.id];
}
