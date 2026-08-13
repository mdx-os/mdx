import test from "node:test";
import assert from "node:assert/strict";
import { load } from "../src/routes/welcome/beta/+page.server.js";

test("beta onboarding restores receipt-backed steps for the signed-in participant", async () => {
  const responseByPath = new Map([
    ["/api/kernel/forge/flywheel-proof.json", { local_real: {}, capability_posture: {}, learning_posture: {} }],
    ["/api/kernel/marketplace/installed-capabilities/projection.json", {}],
    ["/api/kernel/beta/telemetry-events/projection.json", {
      participant_journeys: [
        { participant_actor_id: "participant_a", activation_steps: ["install_completed"] },
        { participant_actor_id: "participant_b", activation_steps: ["first_result_seen"] }
      ]
    }]
  ]);
  const data = await load({
    locals: { session: { actor_id: "participant_b", role: "member" } },
    fetch: async (path) => Response.json(responseByPath.get(path) ?? {}, { status: responseByPath.has(path) ? 200 : 404 })
  });
  assert.deepEqual(data.completedSteps, ["first_result_seen"]);
});

test("hosted beta onboarding covers install, Twin, Pages, GitHub, and Forge", async () => {
  const previousProfile = process.env.MDX_EXPERIENCE_PROFILE;
  process.env.MDX_EXPERIENCE_PROFILE = "public-beta";
  try {
    const data = await load({
      locals: { session: { actor_id: "participant_a", role: "member" } },
      fetch: async () => Response.json({}, { status: 404 })
    });
    const byID = new Map(data.steps.map((step) => [step.id, step]));
    assert.equal(data.steps.length, 8);
    assert.equal(byID.get("install_completed")?.href, "/download/macos");
    assert.match(byID.get("model_connected")?.title ?? "", /Twin/);
    assert.equal(byID.get("model_connected")?.href, "/twin");
    assert.equal(byID.get("first_page_draft")?.href, "/pages");
    assert.match(byID.get("repo_connected")?.body ?? "", /never asks for a personal access token/i);
    assert.equal(byID.get("first_forge_request")?.href, "/forge");
  } finally {
    if (previousProfile === undefined) delete process.env.MDX_EXPERIENCE_PROFILE;
    else process.env.MDX_EXPERIENCE_PROFILE = previousProfile;
  }
});
