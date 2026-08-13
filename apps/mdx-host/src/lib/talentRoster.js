// The Talent roster: the generated populations contract translated into
// people language. Three populations and nothing else - no fourth kind
// of worker can appear here, because the contract is the roster.
// Server-only: generated JSON never ships in client bundles.
import populations from "../../../../generated/agents/mdx-agent-populations.json";
import authBoundary from "../../../../generated/auth/mdx-auth-session-boundary.json";

// The academy register: provider families as schools, said like a
// person would. "Local" is a real school - the deterministic loops
// graduated from this machine and run today.
const SCHOOL_LABELS = {
  anthropic: "Anthropic",
  openai: "OpenAI",
  xai: "xAI",
  google: "Google",
  mistral: "Mistral",
  local: "Local"
};

export function schoolLabel(key) {
  return SCHOOL_LABELS[key] ?? humanName(key);
}

const runnerLines = {
  aegis_scanner_agent: "Watches for threats and records what it saw.",
  charter_attestation_agent: "Keeps the company constitution attested.",
  evals_runner_agent: "Runs the quality checks and saves the verdicts.",
  forge_build_agent: "Carries build intent toward execution.",
  memory_curator_agent: "Curates what the company remembers.",
  talent_onboarding_agent: "Brings new workers in under authority."
};

function humanName(id) {
  return String(id ?? "")
    .replace(/^twin_/, "")
    .replace(/_agent$/, "")
    .replace(/_/g, " ")
    .replace(/\b\w/g, (ch) => ch.toUpperCase());
}

const ceilingLines = {
  READ_ONLY_GROUNDED: "Advises only - grounded in saved proof, never acts."
};

export function talentRoster() {
  return {
    you: authBoundary.local_session
      ? { role: authBoundary.local_session.role ?? "owner" }
      : null,
    companions: (populations.companions ?? []).map((entry) => ({
      id: entry.id,
      name: humanName(entry.id),
      roleTitle: entry.role_title || humanName(entry.role ?? "companion"),
      line: entry.bio || "Answers from saved proof.",
      // The council is consulted, never managed: no badges, no HR fields
      // on display. Provenance stays one tap away in raw.
      cannot: (entry.forbidden_actions ?? []).map((action) => humanName(action).toLowerCase()),
      raw: {
        autonomy_ceiling: entry.autonomy_ceiling ?? "",
        runtime_status: entry.runtime_status ?? "",
        contract_path: entry.path ?? "",
        school: entry.school ?? "",
        background: entry.background ?? "",
        assignment: entry.assignment ?? ""
      }
    })),
    runners: (populations.loop_runners ?? []).map((entry) => ({
      id: entry.id,
      name: humanName(entry.id),
      line: runnerLines[entry.id] ?? "Runs exactly one closed loop.",
      // Loop runners graduated locally: deterministic, running today -
      // truth from the manifest's own mode, never invented.
      school: "local",
      schoolName: schoolLabel("local"),
      background: "deterministic",
      policyChecked: entry.policy_required === true,
      raw: {
        route: entry.route ?? "",
        contract_path: entry.path ?? ""
      }
    })),
    workerTemplates: (populations.ephemeral_workers ?? []).map((entry) => ({
      id: entry.id,
      name: entry.role_title || humanName(entry.id.replace(/_template$/, "")),
      line: entry.bio || "",
      requirements: [
        entry.budget_required ? "a budget" : "",
        entry.expiry_required ? "an expiry" : "",
        entry.scope_required ? "one job" : "",
        entry.tool_allowlist_required ? "a tool allowlist" : "",
        entry.human_sponsor_chain_required ? "a human sponsor" : ""
      ].filter(Boolean),
      raw: {
        runtime_status: entry.runtime_status ?? "",
        contract_path: entry.path ?? ""
      }
    }))
  };
}
