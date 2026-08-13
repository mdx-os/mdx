// The beta participant's first ten minutes: a guided, honest, resumable
// activation path, plus a scope-honesty panel that states what is in beta,
// what is gated, and what is coming. The gated/closed claims are read from
// the proof routes' posture flags so the panel never drifts from reality.
// Reads are same-origin and fail soft.
//
// The steps speak to where MDx actually runs for this person: a hosted beta
// member's workspace lives in the cloud and models are already connected,
// while someone on their own machine claims ownership and wires a provider
// themselves. Operator-only surfaces are never offered to non-operators.
import { hostedExperience } from "../../../lib/experienceProfile.server.js";
import { isOperatorSession } from "../../../lib/session.js";

async function readJson(fetchImpl, path) {
  try {
    const response = await fetchImpl(`/api/kernel${path}`, {
      headers: { accept: "application/json" }
    });
    if (!response.ok) return null;
    return await response.json();
  } catch (error) {
    return null;
  }
}

export async function load({ fetch, locals }) {
  const [flywheel, installed, telemetry] = await Promise.all([
    readJson(fetch, "/forge/flywheel-proof.json"),
    readJson(fetch, "/marketplace/installed-capabilities/projection.json"),
    readJson(fetch, "/beta/telemetry-events/projection.json")
  ]);

  const cap = flywheel?.capability_posture ?? {};
  const learn = flywheel?.learning_posture ?? {};
  const local = flywheel?.local_real ?? {};

  // The honest scope, sourced from posture flags, not hand-written claims.
  const scope = {
    inBeta: [
      "Ask Twin and ground answers in your context",
      "Hand Forge a scoped build and approve the plan",
      "Carry decisions and outcomes in Message",
      "Keep trusted docs and decisions in Memory",
      "Add curated capabilities from Marketplace"
    ],
    gated: [
      {
        line: "Builds run for real only in an isolated copy of your repo, and only when you turn it on",
        on: local.live_repo_mutated === false
      },
      {
        line: "Capabilities can be added but never run, touch secrets, or borrow agent permissions",
        on: cap.capability_execution_allowed === false
      },
      {
        line: "Shipping and deploying always stay your decision",
        on: local.deployment_allowed === false
      },
      {
        line: "Lessons are advisory - a human promotes each one; nothing changes behavior on its own",
        on: learn.adaptation_allowed === false
      }
    ],
    coming: [
      "Community-shared capabilities, once scanned and isolated",
      "Live execution at scale, after the cloud activation track",
      "A gated adaptation lane, only after measured improvement is proven"
    ]
  };

  const hosted = hostedExperience();
  const operator = isOperatorSession(locals?.session);

  const arrivalStep = hosted
    ? { id: "install_completed", title: "Install the apps", body: "Get the notarized Mac app and your TestFlight handoff. Use the same invited account on every device.", href: "/download/macos" }
    : { id: "install_completed", title: "You're in", body: "MDx is running on this machine, your data in a local snapshot.", href: "/" };

  const modelStep = {
    id: "model_connected",
    title: hosted && !operator ? "Ask Twin something real" : hosted ? "Check your models" : "Connect a model",
    body: hosted && !operator
      ? "Models are already connected. Ask a work question or bring a decision you are weighing."
      : hosted
        ? "Models are connected for the beta. Confirm they answer, or adjust them."
        : "Confirm or connect a provider so Twin and Forge can really think.",
    href: hosted ? (operator ? "/admin/models" : "/twin") : "/"
  };

  const steps = [
    arrivalStep,
    modelStep,
    { id: "first_page_draft", title: "Write your first Page", body: "Capture a note, decision, or plan. Keep it private until you choose to publish it.", href: "/pages" },
    { id: "repo_connected", title: "Connect a repository", body: "Choose repository access in GitHub. MDx never asks for a personal access token.", href: "/forge" },
    { id: "first_forge_request", title: "Hand Forge one bounded job", body: "Use a safe repository and a small, reviewable request. Forge plans before it touches anything.", href: "/forge" },
    { id: "first_result_seen", title: "Review the result", body: "Read the checks and receipt, then approve, decline, or leave the branch for later.", href: "/forge/review" },
    { id: "first_capability_inspected", title: "Inspect a capability", body: "Open one curated Marketplace capability and see exactly what it can touch.", href: "/marketplace" },
    { id: "first_feedback_submitted", title: "Tell us one thing", body: "Use the feedback affordance on any surface. It only sends safe context.", href: "/" }
  ];
  const actorID = String(locals?.session?.actor_id ?? locals?.session?.user_id ?? "");
  const participantJourney = (Array.isArray(telemetry?.participant_journeys)
    ? telemetry.participant_journeys
    : []).find((journey) => journey?.participant_actor_id === actorID);

  return {
    scope,
    steps,
    completedSteps: Array.isArray(participantJourney?.activation_steps)
      ? participantJourney.activation_steps
      : [],
    postureReadable: flywheel !== null,
    curatedNote:
      installed?.untrusted_capability_execution_requires_stronger_isolation === true
        ? "Marketplace is curated for beta. Community imports wait for scan review and stronger isolation."
        : "Marketplace is curated for beta.",
    boundary:
      "This page only guides and reads safe posture flags. Nothing here runs, ships, or opens authority.",
    safeNext: "Work down the steps in any order. Each one teaches the mental model and is resumable."
  };
}
