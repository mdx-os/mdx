import { adminGate } from "../../../lib/adminGate.js";

export async function load() {
  return {
    gate: adminGate(),
    boundary:
      "gate is read-only; it maps write standing, receipt coverage, and blocked authority without approving any action",
    safeNext:
      "use Gate to decide which existing governed write or authority contract needs a real approval workflow next"
  };
}
