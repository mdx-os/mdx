import agentWorkQueue from "../../generated/agents/mdx-agent-work-queue.json";
import harnessRunway from "../../generated/architecture/harness-runway.json";

const heldBackStatuses = new Set(["DEFERRED", "PENDING-LIVE-RUN"]);
const localGates = harnessRunway.gate_order.filter((gate) => !heldBackStatuses.has(gate.status));
const heldBackGates = harnessRunway.gate_order.filter((gate) => heldBackStatuses.has(gate.status));
const preferredWorkItem =
  agentWorkQueue.work_items.find((item) => item.id === harnessRunway.preferred_next_work_item) ??
  agentWorkQueue.work_items[0];
const externalMachineLocal = harnessRunway.current_truth.external_execution_machines === "LIVE-LOCAL-FAIL-CLOSED";

export const harnessRunwayOverview = {
  title: "The harness is local, proven, and still deliberately narrow.",
  summary:
    `${localGates.length} gates work locally. Outside-agent requests now fail closed; ${heldBackGates.length} gates still need observed proof.`,
  source: harnessRunway.source,
  metrics: [
    {
      label: "Local gates",
      value: String(localGates.length),
      detail: "proven or fail-closed locally",
      state: "local"
    },
    {
      label: "Held back",
      value: String(heldBackGates.length),
      detail: "await observed proof",
      state: "held-back"
    },
    {
      label: "Outside agents",
      value: externalMachineLocal ? "Denied locally" : "Still closed",
      detail: externalMachineLocal ? "request recorded before any action" : "denial proof still required",
      state: externalMachineLocal ? "local" : "held-back"
    }
  ],
  lanes: [
    {
      label: "Foundation",
      state: "local",
      body: "The north star, manifest, plan-only runner, transcript replay, and inspection path are in place.",
      proof: "Checked by the harness runway, manifest, local plan runner, transcript, and inspection gates."
    },
    {
      label: "Safe tools",
      state: "local",
      body: "Read, search, list, and patch mediation stay scoped, capped, and tied to saved proof.",
      proof: "Checked by the safe tool-plane gate."
    },
    {
      label: "Build path",
      state: "local",
      body: "Model routing, plan execution, Forge, the Talent bridge, and outside-agent requests fail closed before live authority grows.",
      proof: "Checked by provider, plan-execute, Forge-on-harness, Talent bridge, and external-machine gates."
    },
    {
      label: "Held back",
      state: "held-back",
      body: "Durable workers and live connections wait for observed proof. Outside-agent adapters are denied locally.",
      proof: "No spend, deploy, worker start, or state change is granted by these screens."
    }
  ],
  callouts: [
    {
      label: "Outside agents",
      state: externalMachineLocal ? "local" : "held-back",
      title: externalMachineLocal ? "Recorded, denied, and kept out of decisions." : "Still waiting for local denial proof.",
      body: externalMachineLocal
        ? "The harness can admit a request envelope, block the adapter, and hold imported evidence behind MDx judgment."
        : "Every external adapter stays closed until the denial proof exists.",
      proof: "Checked by the external-machine gate."
    },
    {
      label: "Next judgment",
      state: "next",
      title: "Make this easier to trust.",
      body: "The next slice improves the first read before durable workers or live connections expand.",
      proof: preferredWorkItem.required_checks.slice(0, 2).join(", ")
    }
  ],
  next: {
    label: "Next safe slice",
    title: "Make the proof easier to trust before expanding authority.",
    body: preferredWorkItem.summary,
    work_item_id: preferredWorkItem.id,
    checks: preferredWorkItem.required_checks
  },
  evidence: [harnessRunway.source, agentWorkQueue.source]
};
