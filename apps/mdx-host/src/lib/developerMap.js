// The Developer surface's truth sources: generated contracts, slimmed
// server-side for the page. Everything here is the repo describing
// itself - nothing hand-maintained that the generators already own.
// This module is imported by the server load only; generated JSON never
// ships in client bundles.
import architecture from "../../../../generated/architecture/mdx-architecture-map.json";
import loops from "../../../../generated/loops/mdx-local-loops.json";
import routes from "../../../../generated/routes/mdx-local-routes.json";
import workQueue from "../../../../generated/agents/mdx-agent-work-queue.json";
import authBoundary from "../../../../generated/auth/mdx-auth-session-boundary.json";
import verification from "../../../../generated/verification/mdx-local-verification.json";

const loopLines = {
  aegis_scanner_agent: "watches for threats and records what it saw",
  charter_attestation_agent: "keeps the company constitution attested",
  evals_runner_agent: "runs the quality checks and saves verdicts",
  forge_build_agent: "carries build intent toward execution",
  memory_curator_agent: "curates what the company remembers",
  talent_onboarding_agent: "brings new workers in under authority"
};

const checkGroups = [
  {
    id: "local-proof",
    title: "Prove the local stack",
    line: "Use this before trusting a fresh clone or local change.",
    commands: ["make doctor", "make local-smoke", "make agent-preflight-check"]
  },
  {
    id: "ui-surface",
    title: "Change a product surface",
    line: "Use this for host UI, first-use hints, copy, and route-visible behavior.",
    commands: ["make ui-app-check", "make ui-build-check", "make ui-screenshot-proof-check", "make no-fake-green-check"]
  },
  {
    id: "generated-contract",
    title: "Change contracts or generated truth",
    line: "Use this when touching generated packets, source maps, or verification posture.",
    commands: ["make generate", "make generated-clean", "make source-map-check", "make verification-manifest-check"]
  },
  {
    id: "frozen-spine",
    title: "Touch the frozen spine",
    line: "Use this before any kernel, route catalog, receipt, auth, secret, or CI spine edit.",
    commands: ["make frozen-kernel-contract-check", "make tier0-foundation-check", "make change-gate-check"]
  },
  {
    id: "beta-handoff",
    title: "Prepare beta handoff",
    line: "Use this before a maintainer or their agent reviews a beta-bound slice.",
    commands: ["make tier1-flagship-readiness-check", "make tier2-beta-readiness-check", "make agent-context-spine-check"]
  }
];

const developerDocs = [
  {
    title: "DEV-101",
    path: "docs/DEV-101.md",
    line: "Start here when you are new to the repo."
  },
  {
    title: "DEV-201",
    path: "docs/DEV-201.md",
    line: "Use this when you are ready to change MDx or point it at another repo."
  },
  {
    title: "Tier 2 packet",
    path: "docs/TIER-2-BETA-READINESS.md",
    line: "Read this before beta launch review."
  },
  {
    title: "G1 enforcement",
    path: "docs/G1-ENFORCEMENT-CHECKLIST.md",
    line: "Apply this in the target maintainer repo before handoff."
  }
];

function commandState(command) {
  return (verification.commands ?? []).some((entry) => entry.command === command)
    ? "declared"
    : "local";
}

export function developerMap() {
  const crates = Array.isArray(architecture.crates) ? architecture.crates : [];
  const loopList = (loops.loops ?? []).map((loop) => ({
    id: loop.id,
    line: loopLines[loop.id] ?? String(loop.id ?? "").replace(/_/g, " "),
    policyRequired: loop.policy_required === true
  }));

  const groups = new Map();
  for (const route of routes.routes ?? []) {
    const surface = route.surface ?? "other";
    if (!groups.has(surface)) {
      groups.set(surface, []);
    }
    groups.get(surface).push({
      method: route.method,
      path: route.local_path,
      receiptBacked: route.receipt_backed === true
    });
  }
  const apiGroups = [...groups.entries()]
    .map(([surface, list]) => ({
      surface,
      label: surface.replace(/_/g, " "),
      count: list.length,
      writes: list.filter((entry) => entry.method === "POST").length,
      routes: list
    }))
    .sort((a, b) => b.count - a.count);

  const workItems = (workQueue.work_items ?? []).map((item) => ({
    id: item.id,
    priority: item.priority ?? null,
    summary: item.summary ?? "",
    owners: Array.isArray(item.owner_type) ? item.owner_type.join(", ") : ""
  }));

  return {
    crates: crates.map((crate) => String(crate?.name ?? crate)),
    loops: loopList,
    apiGroups,
    routeCount: (routes.routes ?? []).length,
    governedWriteCount: (routes.routes ?? []).filter(
      (route) => route.method === "POST" && route.receipt_backed === true
    ).length,
    workItems,
    workQueueSource: workQueue.source ?? "generated/agents/mdx-agent-work-queue.json",
    checkGroups: checkGroups.map((group) => ({
      ...group,
      commands: group.commands.map((command) => ({
        command,
        state: commandState(command)
      }))
    })),
    developerDocs,
    session: authBoundary.local_session ?? null,
    rules: Array.isArray(architecture.rules) ? architecture.rules : []
  };
}
