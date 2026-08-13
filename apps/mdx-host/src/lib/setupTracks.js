// Setup surface helpers. The alpha-onboarding packet is truth; these
// helpers only translate its track ids and status tokens into human
// language. Raw tokens never reach primary chrome - they stay available
// in each track's disclosure. Unknown statuses degrade to an honest
// "not yet", never to an invented readiness.

export const trackMeta = {
  local_dev: {
    name: "On this machine",
    vendor: "",
    line: "Models, memory, database, workflows - everything runs locally, saved with proof."
  },
  provider_connected_local: {
    name: "Local, with live AI",
    vendor: "",
    line: "The same machine, answering with real model providers once their turn-on is recorded."
  },
  personal_render_supabase: {
    name: "Personal cloud",
    vendor: "Render + Supabase",
    line: "Your own small deployment - the easiest way to take MDx off this machine."
  },
  enterprise_aws_eks_rds: {
    name: "Enterprise cloud",
    vendor: "AWS · EKS + RDS",
    line: "The full stack on your company's AWS, with its own keys and its own proofs."
  },
  gcp_gke_cloudsql: {
    name: "Enterprise cloud",
    vendor: "Google · GKE + Cloud SQL",
    line: "The full stack on Google Cloud, with its own keys and its own proofs."
  },
  self_hosted_kubernetes: {
    name: "Self-hosted",
    vendor: "Your Kubernetes",
    line: "Run it wherever you run everything else, once public readiness is proven."
  }
};

export function humanStatus(status) {
  switch (status) {
    case "READY":
      return { label: "Running today", kind: "ready" };
    case "READY_AFTER_PROVIDER_TURN_ON":
      return { label: "One turn-on away", kind: "soon" };
    case "SCAFFOLD_READY_DEPLOYMENT_BLOCKED":
      return { label: "Scaffolded, waiting on its receipts", kind: "soon" };
    case "PUBLIC_READINESS_REQUIRED":
      return { label: "Further out", kind: "later" };
    default:
      return { label: "Not yet", kind: "later" };
  }
}

export function tracksFrom(packet) {
  const tracks = Array.isArray(packet?.onboarding_tracks) ? packet.onboarding_tracks : [];
  return tracks.map((track) => {
    const meta = trackMeta[track.id] ?? { name: track.id?.replace(/_/g, " ") ?? "Track", vendor: "", line: "" };
    return {
      id: track.id,
      name: meta.name,
      vendor: meta.vendor,
      line: meta.line,
      status: humanStatus(track.status),
      rawStatus: track.status ?? "",
      envTemplate: track.env_template ?? "",
      evidenceRoute: track.evidence_route ?? "",
      proofCommand: track.proof_command ?? "",
      nextAction: track.next_operator_action ?? ""
    };
  });
}

export function pulseFrom(packet) {
  if (!packet) {
    return null;
  }
  const parts = [];
  if (packet.local_install_ready) {
    parts.push("MDx runs fully on this machine today");
  }
  const scaffolded = (packet.onboarding_tracks ?? []).filter((track) =>
    String(track.status ?? "").startsWith("SCAFFOLD_READY")
  ).length;
  if (scaffolded > 0) {
    parts.push(`${scaffolded} cloud ${scaffolded === 1 ? "track is" : "tracks are"} scaffolded and waiting on their proofs`);
  }
  if (parts.length === 0) {
    return null;
  }
  return `${parts.join(", and ")}.`;
}
