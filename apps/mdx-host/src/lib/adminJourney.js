// The admin journey: one guided spine computed entirely from the kernel's
// own packets. Six steps, each "done", "current", or "later" by packet
// fields alone - never by hope, never by a flag this surface invents.
//
// The current step is the first one not yet done. Steps before it carry a
// quiet check. Steps after it dim to a one-line "what's coming". When the
// kernel is not answering, every packet is null: nothing is done, step one
// becomes an invitation, and zero readiness is claimed.
//
// Commands are taken verbatim from the packets where the packet provides
// them (proof_check), so the line a human runs is the line the kernel
// vouches for. Links point at existing admin surfaces only.

function bool(value) {
  return value === true;
}

// Step 1 - Prove it on this machine.
// Done when the readiness packet reports the local install ready. The
// command is the packet's own proof_check, verbatim.
function step1(readiness) {
  const done = bool(readiness?.local_install_ready);
  return {
    id: "prove_local",
    title: "Prove it on this machine",
    done,
    get:
      "A full MDx stack running locally - models, memory, database, and workflows - with saved evidence you can re-run any time.",
    command: readiness?.proof_check ?? "make v2-fresh-install-proof",
    doneLine: "This install proved itself on this machine.",
    proves:
      "Run the command and it writes its own evidence file. Green means the local stack stood up for real.",
    coming: "It starts here: one command brings the whole stack up locally.",
    rawState: readiness?.status ?? ""
  };
}

// Step 2 - Choose your track.
// Six ways to run MDx arrive with the onboarding packet; the local track
// is ready today. Done once the tracks exist and the local one is ready,
// which is what makes choosing a real choice rather than a wish.
function step2(onboarding) {
  const tracks = Array.isArray(onboarding?.onboarding_tracks) ? onboarding.onboarding_tracks : [];
  const localReady = tracks.some((track) => track?.id === "local_dev" && track?.status === "READY");
  const done = tracks.length > 0 && localReady;
  return {
    id: "choose_track",
    title: "Choose your track",
    done,
    get:
      "A clear path from this machine to your company's cloud - six tracks, each saying honestly where it stands and exactly what opens it.",
    link: "/admin/setup",
    linkLabel: "Open Setup",
    doneLine: "Your tracks are laid out, with the local one ready today.",
    proves:
      "Setup reads the same onboarding packet and shows every track's real state - nothing marked ready that the kernel did not report.",
    coming: "Then pick how far you want to take it, from this machine to the cloud.",
    rawState: onboarding?.status ?? ""
  };
}

// Step 3 - Add your keys.
// Provider profiles wait for their environment. Done the moment any
// candidate reports env_ready, which is the kernel's signal that a
// provider's keys are present (presence only - values never reach here).
function step3(decision) {
  const candidates = Array.isArray(decision?.activation_candidates) ? decision.activation_candidates : [];
  const anyEnvReady = candidates.some((candidate) => bool(candidate?.env_ready));
  const readyCount = Number(decision?.env_ready_candidate_count ?? 0);
  const done = anyEnvReady || readyCount > 0;
  return {
    id: "add_keys",
    title: "Add your keys",
    done,
    get:
      "Live AI on this same machine - the moment a provider's keys are in your shell, its profile shows ready for turn-on.",
    link: "/admin/platform",
    linkLabel: "Open Platform",
    doneLine: "At least one provider's keys are in place and seen.",
    proves:
      "Keys live in your shell, never in MDx. Platform reads each profile and reports the ones whose environment is present.",
    coming: "Drop a provider's keys into your shell to light up live AI locally.",
    rawState: `${readyCount} of ${Number(decision?.candidate_count ?? candidates.length)} ready`
  };
}

// Step 4 - Turn it on.
// A candidate is turned on once it moves past WAITING_FOR_CREDENTIALS with
// its approval on the record. Done when any candidate is in that state -
// the observed-proof story for a provider answering for real.
function step4(decision) {
  const candidates = Array.isArray(decision?.activation_candidates) ? decision.activation_candidates : [];
  const movedOn = candidates.filter(
    (candidate) => candidate?.status && candidate.status !== "WAITING_FOR_CREDENTIALS"
  );
  const liveAllowed = bool(decision?.live_provider_call_allowed_now);
  const done = movedOn.length > 0 || liveAllowed;
  return {
    id: "turn_it_on",
    title: "Turn it on",
    done,
    get:
      "A provider actually answering - the turn-on recorded, the first live call proven, the receipt saved.",
    command: decision?.provider_turn_on_check ?? "make live-turn-on-check",
    doneLine: "A provider is past waiting and turned on, on the record.",
    proves:
      "Turn-on is a recorded approval plus a proven first call. The drill writes both before anything calls out for real.",
    coming: "Record the turn-on and watch the first real call go through.",
    rawState: `${movedOn.length} past waiting`
  };
}

// Step 5 - Take it to the cloud.
// The onboarding packet decides when deployment is allowed; nothing here
// front-runs it. Done only when cloud_deployment_allowed_now is true.
function step5(onboarding) {
  const done = bool(onboarding?.cloud_deployment_allowed_now);
  return {
    id: "to_the_cloud",
    title: "Take it to the cloud",
    done,
    get:
      "MDx running off this machine - your own deployment, with its own keys and its own proofs, when its receipts are in place.",
    link: "/admin/setup",
    linkLabel: "See the cloud tracks",
    doneLine: "Cloud deployment is open for this install.",
    proves:
      "Deployment opens only when the kernel says its plan and receipts are recorded - this step reads that decision, it does not make it.",
    coming: "When you are ready, take the proven stack to your cloud.",
    rawState: bool(onboarding?.cloud_deployment_allowed_now) ? "allowed" : "not yet"
  };
}

// Step 6 - Invite the team.
// Real sign-in is the last door. Done only when real_login_allowed_now is
// true - until then every seat is the owner's, and this step says so.
function step6(onboarding) {
  const done = bool(onboarding?.real_login_allowed_now);
  return {
    id: "invite_team",
    title: "Invite the team",
    done,
    get:
      "Real sign-in and seats for your team - roles and invites, each with its own recorded turn-on.",
    doneLine: "Sign-in is live, so the team can come aboard.",
    proves:
      "Login opens only when the kernel reports it ready. Until then every seat is the owner's, and nothing here pretends a seat exists.",
    coming: "Last, open sign-in and bring your team aboard.",
    rawState: bool(onboarding?.real_login_allowed_now) ? "allowed" : "not yet"
  };
}

export function buildJourney({ readiness, onboarding, decision }) {
  const kernelUp = Boolean(readiness || onboarding || decision);

  const steps = [
    step1(readiness),
    step2(onboarding),
    step3(decision),
    step4(decision),
    step5(onboarding),
    step6(onboarding)
  ];

  // Current = first step not yet done. If the kernel is down, step one is
  // the current invitation. If everything is done, the last step holds.
  let currentIndex = steps.findIndex((step) => !step.done);
  if (!kernelUp) {
    currentIndex = 0;
  } else if (currentIndex === -1) {
    currentIndex = steps.length - 1;
  }

  const doneCount = steps.filter((step) => step.done).length;

  return steps.map((step, index) => {
    let state;
    if (!kernelUp) {
      state = index === 0 ? "current" : "later";
    } else if (step.done) {
      state = "done";
    } else if (index === currentIndex) {
      state = "current";
    } else {
      state = "later";
    }
    return { ...step, number: index + 1, state };
  }).reduce(
    (acc, step) => {
      acc.steps.push(step);
      return acc;
    },
    { steps: [], kernelUp, doneCount, total: steps.length, currentIndex }
  );
}

// A one-line human read of where the journey stands, for the page header.
export function journeyPulse({ kernelUp, doneCount, total, steps, currentIndex }) {
  if (!kernelUp) {
    return null;
  }
  const current = steps[currentIndex];
  if (doneCount >= total) {
    return "Every step is done - this install is all the way through.";
  }
  if (doneCount === 0) {
    return `You're at the start: ${current.title.toLowerCase()}.`;
  }
  return `${doneCount} of ${total} done. You're here: ${current.title.toLowerCase()}.`;
}
