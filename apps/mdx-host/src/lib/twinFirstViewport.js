import appReadiness from "../../../../generated/apps/mdx-app-readiness.json";
import authBoundary from "../../../../generated/auth/mdx-auth-session-boundary.json";
import groundingContract from "../../../../generated/companions/twin-text-grounding-contract.json";
import groundingFixtures from "../../../../generated/companions/twin-grounding-fixtures.json";
import personaContract from "../../../../generated/companions/twin-session-persona-contract.json";

const twinReadiness =
  appReadiness.apps.find((app) => app.legacy_app === "Twin") ?? {
    readiness: "unknown",
    blocked_by: "missing Twin readiness entry",
    next_contract: "generated/apps/mdx-app-readiness.json",
    first_local_proof: "make twin-grounding-check"
  };

const admittedFixture = personaContract.admitted_fixture;
const architect =
  groundingContract.companions.find((companion) => companion.id === "twin_architect") ??
  groundingContract.companions[0];

export const twinFirstViewport = {
  source: "generated/companions/twin-session-persona-contract.json",
  currentSignal: `${twinReadiness.readiness}: ${personaContract.allowed_companions.length} companions are admitted only through local session persona evidence.`,
  judgment:
    "Prove Twin as a route-owned companion viewport before moving composer taste, voice, and subtle UX from the current product shell.",
  safeNext: personaContract.next_runtime_step,
  boundary: [
    admittedFixture.provider_call_allowed ? "provider calls allowed" : "provider calls blocked",
    admittedFixture.memory_write_allowed ? "memory writes allowed" : "memory writes blocked",
    admittedFixture.worker_spawn_allowed ? "worker spawn allowed" : "worker spawn blocked",
    authBoundary.actor_admission.production_write_allowed ? "production writes allowed" : "production writes blocked"
  ],
  readiness: {
    status: twinReadiness.readiness,
    blockedBy: twinReadiness.blocked_by,
    nextContract: twinReadiness.next_contract,
    firstLocalProof: twinReadiness.first_local_proof
  },
  admission: {
    result: admittedFixture.admission_result,
    receiptKind: admittedFixture.draft_receipt_kind,
    companionId: admittedFixture.companion_id,
    stance: admittedFixture.companion_stance,
    promptShape: admittedFixture.prompt_shape,
    requiredFields: personaContract.admission_contract.required_fields,
    forbiddenCapabilities: personaContract.admission_contract.forbidden_capabilities
  },
  grounding: {
    runtimeStatus: groundingContract.runtime_status,
    responseEvidence: groundingContract.required_response_evidence,
    architectSources: architect.grounding_sources,
    fixtureCompanionCount: groundingFixtures.companions.length
  },
  evidence: admittedFixture.source_contracts
};
