// Aegis (admin): the declared posture is generated truth and always renders.
// The locked-by-design doors and the scan evidence are runtime truth, read
// through the kernel proxy, fail-soft to null so the surface invites instead
// of inventing a number when the local server is not running.
import { hardStops, threats, watchers, postureSource } from "../../../lib/aegisPosture.js";

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

// The v1 security cockpit carries a set of "<capability>_allowed" booleans.
// Every false is a door the install keeps shut on purpose. We translate the
// shut ones into human sentences and keep the exact flag for the disclosure.
const DOOR_LINES = {
  writes_allowed: "Nothing here can change your data.",
  execution_allowed: "Nothing here can run code on your behalf.",
  live_read_allowed: "Live production data is not read from this surface.",
  row_payload_allowed: "Raw rows are never exported out.",
  service_role_runtime_allowed: "The all-powerful service role can't run here.",
  production_write_allowed: "Production stays read-only from here.",
  production_cutover_allowed: "No switch flips this install onto production.",
  provider_call_allowed: "Outside providers aren't called from here.",
  security_remediation_write_allowed: "Even fixing security findings happens elsewhere, on the record.",
  storage_write_allowed: "Stored files can't be written from this surface.",
  schema_green_allowed: "The schema can't be declared healthy until its proof exists."
};

function lockedDoors(cockpit) {
  if (!cockpit) return null;
  return Object.keys(DOOR_LINES)
    .filter((flag) => cockpit[flag] === false)
    .map((flag) => ({ line: DOOR_LINES[flag], raw: flag }));
}

function scanEvidence(receipts) {
  if (!receipts || !Array.isArray(receipts.receipt_ids)) return null;
  return receipts.receipt_ids
    .filter((id) => String(id).includes("aegis"))
    .slice(-8)
    .reverse();
}

export async function load({ fetch }) {
  const [cockpit, receipts] = await Promise.all([
    readJson(fetch, "/v1/security-schema-replacement-cockpit.json"),
    readJson(fetch, "/receipts.json")
  ]);

  return {
    watchers: watchers(),
    threats: threats(),
    hardStops: hardStops(),
    doors: lockedDoors(cockpit),
    scans: scanEvidence(receipts),
    source: postureSource,
    boundary:
      "this surface only reads - it can't change data, run code, touch production, or fix a finding; scans run as the aegis loop under policy, so there is no start button here, and missing evidence reads as unknown, never as safe",
    safeNext:
      "start the local server to see the locked doors and any scan receipts fill in, or open Platform to follow a rail toward live"
  };
}
