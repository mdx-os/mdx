// The "what's moving" panels for Twin's home, fetched by the client after
// first paint so the greeting and composer never wait on this ten-way
// projection fan-out. Kept server-side (not inlined in the page load) so the
// cross-surface digest logic stays here and the page ships instantly; a plain
// same-origin fetch is also CSP-safe, where a streamed deferred promise is not.
import { json } from "@sveltejs/kit";
import { crossMdxDigest } from "../../../lib/crossMdx.js";

async function readJson(fetchImpl, path) {
  try {
    const response = await fetchImpl(`/api/kernel${path}`, { signal: AbortSignal.timeout(1500) });
    return response.ok ? await response.json() : null;
  } catch (error) {
    return null;
  }
}

export async function GET({ fetch }) {
  const [
    messages,
    pages,
    observatory,
    proposals,
    productBets,
    needsYou,
    forgeRuns,
    pagesLifecycle,
    installedCapabilities,
    teamShelf,
    activeMemory,
    twinRuntime,
    twinReadiness,
    twinArtifacts,
    modelConnection,
    ratifiedMemory
  ] = await Promise.all([
    readJson(fetch, "/messages/thread-messages/projection.json"),
    readJson(fetch, "/pages.json"),
    readJson(fetch, "/observatory.json"),
    readJson(fetch, "/strategy/direction-proposals/projection.json"),
    readJson(fetch, "/product/bet-drafts/projection.json"),
    readJson(fetch, "/product/needs-you/projection.json"),
    readJson(fetch, "/forge/runs/projection.json"),
    readJson(fetch, "/pages/lifecycle/projection.json"),
    readJson(fetch, "/marketplace/installed-capabilities/projection.json"),
    readJson(fetch, "/marketplace/team-shelf.json"),
    readJson(fetch, "/learning/memory-activations/projection.json"),
    readJson(fetch, "/twin/agent-runtime.json"),
    readJson(fetch, "/twin/intelligence-readiness.json"),
    readJson(fetch, "/twin/artifact-runtime.json"),
    readJson(fetch, "/models/connections.json"),
    readJson(fetch, "/memory/consolidation-ratifications/projection.json")
  ]);

  const acrossMdx = crossMdxDigest({
    needsYou,
    forgeRuns,
    messages,
    pages: {
      ...pagesLifecycle,
      document_count: Number(pages?.count ?? pagesLifecycle?.document_count ?? 0),
      latest_document: Array.isArray(pages?.documents) ? pages.documents[pages.documents.length - 1] : null
    },
    strategyProposals: proposals,
    productBets,
    twinRuntime,
    twinReadiness,
    twinArtifacts,
    modelConnection,
    ratifiedMemory
  });

  const latestMessage = Array.isArray(messages?.messages)
    ? messages.messages[messages.messages.length - 1]
    : null;

  const installedList = Array.isArray(installedCapabilities?.installed) ? installedCapabilities.installed : [];
  const pendingReview = Array.isArray(teamShelf?.pending_review) ? teamShelf.pending_review : [];
  const capabilityAwareness = {
    installedCount: installedList.length,
    installed: installedList.slice(0, 6).map((c) => ({
      id: String(c?.capability_id ?? ""),
      name: String(c?.name ?? c?.capability_id ?? ""),
      scope: String(c?.scope ?? "")
    })),
    pendingCount: pendingReview.length,
    pendingNames: pendingReview
      .slice(0, 3)
      .map((c) => String(c?.name ?? c?.id ?? ""))
      .filter(Boolean)
  };

  const activeLessons = (Array.isArray(activeMemory?.active_memories) ? activeMemory.active_memories : [])
    .filter((m) => String(m?.active_memory_state ?? "") === "active")
    .map((m) => ({
      summary: String(m?.lesson_summary ?? ""),
      owner: String(m?.review_owner ?? "").replace(/^[a-z]+:/, ""),
      receiptId: String(m?.active_memory_receipt_id ?? "")
    }))
    .filter((m) => m.summary);

  return json({
    acrossMdx,
    capabilityAwareness,
    activeLessons,
    briefing: {
      reachable: Boolean(messages || pages || observatory),
      messageCount: Number(messages?.message_count ?? 0),
      latestMessage: latestMessage
        ? {
            body: String(latestMessage.body ?? "").slice(0, 120),
            actor: String(latestMessage.actor_display_name ?? "someone").replace(/^[a-z]+:/, "")
          }
        : null,
      pageCount: Number(pages?.count ?? 0),
      receiptCount: Number(observatory?.receipt_count ?? 0)
    }
  });
}
