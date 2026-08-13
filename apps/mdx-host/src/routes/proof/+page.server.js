// Observatory reads runtime truth through the same-origin kernel proxy,
// server-side and fail-soft: the chain count and receipt ids come from
// the kernel's own answer, the human texture from the same projections
// the Home feed uses. Kernel down means every section invites instead of
// claiming a number it cannot prove.
import {
  chainChapters,
  chainHeadline,
  latestReceipts,
  latestRunLine,
  recentActivity
} from "../../lib/observatoryProof.js";

async function readJson(fetchImpl, path) {
  try {
    const response = await fetchImpl(`/api/kernel${path}`, {
      signal: AbortSignal.timeout(1500)
    });
    if (!response.ok) {
      return null;
    }
    return await response.json();
  } catch (error) {
    return null;
  }
}

export async function load({ fetch }) {
  const [observatory, twin, messages, checkpoints] = await Promise.all([
    readJson(fetch, "/observatory.json"),
    readJson(fetch, "/twin/session-drafts/projection.json"),
    readJson(fetch, "/messages/thread-messages/projection.json"),
    readJson(fetch, "/audit/evidence-checkpoints/projection.json")
  ]);

  const twinDrafts = Array.isArray(twin?.drafts) ? twin.drafts : [];
  const messageItems = Array.isArray(messages?.messages) ? messages.messages : [];

  // The evidence-integrity line: the latest checkpoint, whether it is
  // signed, and whether it has been anchored outside MDx. Null when no
  // checkpoint exists, so the row invites rather than reciting zero.
  const checkpointList = Array.isArray(checkpoints?.checkpoints) ? checkpoints.checkpoints : [];
  const latestCheckpoint = checkpointList[checkpointList.length - 1] ?? null;
  const integrity = latestCheckpoint
    ? {
        coveredThrough: latestCheckpoint.range_end_receipt_id,
        receiptCount: Number(latestCheckpoint.receipt_count ?? 0),
        signed: latestCheckpoint.signed === true,
        anchored: latestCheckpoint.externally_anchored === true,
        checkpointCount: checkpointList.length
      }
    : null;

  return {
    integrity,
    reachable: observatory != null,
    receiptCount: Number(observatory?.receipt_count ?? 0),
    headline: chainHeadline(observatory?.receipt_count),
    runLine: latestRunLine(observatory?.latest_run),
    chapters: chainChapters(observatory),
    latest: latestReceipts(observatory),
    activity: recentActivity({ twinDrafts, messages: messageItems }),
    boundary:
      "the chain is hash-linked and read-only here; nothing on this page changes the record, and every receipt counts only because it can be proven",
    safeNext: "open Twin and ask what happened, or take any action elsewhere and watch its receipt land here"
  };
}
