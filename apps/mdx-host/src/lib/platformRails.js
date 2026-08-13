// Platform helpers: substrate statuses and turn-on candidates translated
// into operator language. The kernel's packets are truth; unknown
// statuses degrade to honest neutrality, never to invented liveness.
// Env requirement NAMES and presence booleans are sanctioned by the
// packets themselves (credential_presence_only) - values never exist
// here to leak.

const substrateMeta = {
  postgres: { name: "Database", line: "Where everything the company does is kept." },
  temporal: { name: "Workflows", line: "Durable execution for long-running work." },
  tensorzero: { name: "Model gateway", line: "The door to live AI providers." },
  mem0: { name: "Memory", line: "What the company remembers between conversations." },
  opentelemetry: { name: "Telemetry", line: "How the system watches itself." },
  render: { name: "Deployment", line: "The road from this machine to the cloud." }
};

const candidateNames = {
  tensorzero_openai: "OpenAI, through the gateway",
  tensorzero_anthropic: "Anthropic, through the gateway",
  temporal_workflow: "Temporal workflows",
  mem0_memory: "Mem0 memory",
  opentelemetry_export: "Trace export",
  render_deployment: "Render deployment"
};

export function substratesFrom(statusPacket) {
  const list = Array.isArray(statusPacket?.live_substrate) ? statusPacket.live_substrate : [];
  return list.map((entry) => {
    const meta = substrateMeta[entry.substrate] ?? {
      name: String(entry.substrate ?? "").replace(/\b\w/g, (ch) => ch.toUpperCase()),
      line: ""
    };
    const raw = String(entry.status ?? "");
    const live = raw.startsWith("LIVE");
    return {
      id: entry.substrate,
      name: meta.name,
      line: meta.line,
      live,
      statusLabel: live ? "Live, on this machine" : "Local stand-in today",
      goesLiveWhen: entry.turn_on_signal ?? "",
      raw: {
        status: raw,
        blocking: Array.isArray(entry.blocking_rails) ? entry.blocking_rails.join(", ") : "",
        firstProof: entry.first_local_proof ?? ""
      }
    };
  });
}

export function candidatesFrom(decisionPacket) {
  const list = Array.isArray(decisionPacket?.activation_candidates)
    ? decisionPacket.activation_candidates
    : [];
  return list.map((entry) => {
    const raw = String(entry.status ?? "");
    const ready = entry.env_ready === true;
    return {
      id: entry.id,
      name: candidateNames[entry.id] ?? String(entry.id ?? "").replace(/_/g, " "),
      statusLabel:
        raw === "WAITING_FOR_CREDENTIALS"
          ? "Waiting on its keys"
          : ready
            ? "Keys present - waiting on its observed proof"
            : "Not yet",
      requirements: (entry.env_requirements ?? []).map((req) => ({
        name: req.name ?? "",
        present: req.present === true,
        purpose: req.purpose ?? ""
      })),
      raw: {
        status: raw,
        adapter: entry.live_adapter ?? "",
        countsWhen: entry.turn_on_signal ?? entry.required_evidence ?? "",
        receiptKind: entry.required_receipt_kind ?? ""
      }
    };
  });
}

export function platformPulse(substrates) {
  if (!substrates || substrates.length === 0) {
    return null;
  }
  const live = substrates.filter((entry) => entry.live);
  const waiting = substrates.length - live.length;
  const liveNames = live.map((entry) => entry.name.toLowerCase()).join(", ");
  if (live.length === 0) {
    return `${substrates.length} rails are running on local stand-ins today - each goes live with its own observed proof.`;
  }
  return `The ${liveNames} ${live.length === 1 ? "is" : "are"} live on this machine; ${waiting} ${waiting === 1 ? "rail runs" : "rails run"} on local stand-ins until their live proofs are observed.`;
}
