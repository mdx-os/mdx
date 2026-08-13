// Charter: the constitution itself is generated truth (complete without
// the kernel). What changes at runtime is the record - how many
// attestations have actually landed on this machine. We read the receipt
// chain through the proxy, fail-soft to null so the page invites instead
// of claiming a number it cannot prove.
import {
  charterAttestationGuarantees,
  charterLines,
  charterPrinciples
} from "../../../lib/charterRules.js";

async function readJson(fetchImpl, path) {
  try {
    const response = await fetchImpl(`/api/kernel${path}`, {
      signal: AbortSignal.timeout(1500)
    });
    return response.ok ? await response.json() : null;
  } catch (error) {
    return null;
  }
}

export async function load({ fetch }) {
  const receipts = await readJson(fetch, "/receipts.json");

  // The attestation count is runtime truth: receipts on this machine that
  // belong to the charter loop. Null means the kernel was unreachable;
  // zero means honestly no attestations yet.
  let attestationCount = null;
  let recordCount = null;
  if (receipts && Array.isArray(receipts.receipt_ids)) {
    recordCount = receipts.receipt_ids.length;
    attestationCount = receipts.receipt_ids.filter(
      (id) => id.includes("charter") || id.includes("attest")
    ).length;
  }

  const attestation = charterAttestationGuarantees();

  return {
    principles: charterPrinciples(),
    lines: charterLines(),
    attestation,
    attestationCount,
    recordCount,
    boundary:
      "this page states the company's rules and reads the attestation record; it never edits a rule or signs an attestation - those run as the charter_attestation loop under policy, on the record",
    safeNext:
      "run make local-smoke to refresh the local record, or open any principle to see the exact check that keeps it honest"
  };
}
