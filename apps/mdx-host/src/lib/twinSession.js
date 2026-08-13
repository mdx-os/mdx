// Twin session runtime helpers for the host route. The conversation is
// runtime truth from the kernel (restored projection + governed draft
// writes). Identity and the declared-route catalog arrive from the server
// load - generated contracts never ship in the client bundle.
import { createMdxClient, routeCatalogFromGenerated } from "@mdx/client";

export const draftsPath = "/twin/session-drafts.json";
export const projectionPath = "/twin/session-drafts/projection.json";
export const streamPath = "/twin/session-stream";

export function answerFirstPromptShape(shape, laneNote = "") {
  const base = String(shape ?? "").trim();
  const answerFirst = "Always answer the person's request directly in natural-language text before any optional tool or follow-up suggestion. Never return only a tool call.";
  return [base, answerFirst].filter(Boolean).join("; ") + String(laneNote ?? "");
}

// All kernel traffic rides the same-origin proxy: no CORS, no baked URLs,
// host security headers on every request.
const apiBase = "/api/kernel";

// The streaming send: tokens arrive as they leave the model, the receipt
// arrives in the done event (written server-side when the stream
// completed). The serve layer answers with a single "fallback" event
// when the live gates do not hold - the caller then uses the ordinary
// governed POST, so deterministic stays the default of defaults.
// Returns true when the stream produced a saved answer.
export async function streamAnswer({ body, signal, onDelta, onDone, onError, onProposal, onGuarded, onRedaction }) {
  let response;
  try {
    response = await fetch(`${apiBase}${streamPath}`, {
      method: "POST",
      signal,
      headers: { Accept: "text/event-stream", "Content-Type": "application/json" },
      body: JSON.stringify(body)
    });
  } catch (error) {
    return false;
  }
  if (!response.ok || !(response.headers.get("content-type") ?? "").includes("text/event-stream")) {
    return false;
  }
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let done = false;
  try {
    while (true) {
      const chunk = await reader.read();
      if (chunk.done) break;
      buffer += decoder.decode(chunk.value, { stream: true });
      let boundary;
      while ((boundary = buffer.indexOf("\n\n")) >= 0) {
        const frame = buffer.slice(0, boundary);
        buffer = buffer.slice(boundary + 2);
        const data = frame.split("\n").find((line) => line.startsWith("data: "));
        if (!data) continue;
        let event;
        try {
          event = JSON.parse(data.slice(6));
        } catch (error) {
          continue;
        }
        if (event.type === "delta") {
          onDelta?.(event.text ?? "");
        } else if (event.type === "tool_proposal") {
          onProposal?.(event.tool ?? "", event.args ?? {}, event.proposal_id ?? "");
        } else if (event.type === "done") {
          done = true;
          onDone?.(event);
        } else if (event.type === "guarded") {
          // An input guard held the message back: nothing was sent,
          // nothing was saved, and the surface says so.
          done = true;
          onGuarded?.(event.message ?? "that message was held back by a guard");
        } else if (event.type === "redaction") {
          // Output guard corrected the record; the display follows.
          onRedaction?.(event.text ?? "");
        } else if (event.type === "fallback") {
          return false;
        } else if (event.type === "error") {
          done = true;
          onError?.(event.message ?? "the live answer was interrupted");
        }
      }
    }
  } catch (error) {
    // Aborted (the person hit stop) or the connection dropped. Either
    // way no receipt exists unless done already arrived.
    return done;
  }
  return done;
}

export function createTwinClient({ session, catalogSlice }, fetchImpl = globalThis.fetch) {
  return createMdxClient({
    baseUrl: apiBase,
    fetchImpl,
    routeCatalog: routeCatalogFromGenerated(catalogSlice),
    session
  });
}

export function answerTurn(companions, packet, restored) {
  const meta = companions.find((entry) => entry.id === packet.companion_id);
  return {
    kind: "companion",
    restored,
    companionId: packet.companion_id ?? "",
    companionName: meta?.name ?? String(packet.companion_id ?? "companion").replace(/_/g, " "),
    stance: packet.companion_stance ?? meta?.stance ?? "",
    body: packet.grounded_answer ?? "",
    draftReceiptId: packet.draft_receipt_id ?? "",
    answerReceiptId: packet.answer_receipt_id ?? "",
    model: packet.model_gateway_model_id ?? "",
    trustedContextUsed: packet.trusted_context_used === true || packet.trusted_context_used === "true",
    trustedContextSourceCount: Number(packet.trusted_context_source_count ?? 0),
    trustedContextSourceIds: packet.trusted_context_source_ids ?? "",
    trustedContextReceiptIds: packet.trusted_context_receipt_ids ?? "",
    trustedContextProjectionRoute: packet.trusted_context_projection_route ?? "/pages/context-sources/projection.json",
    // A turn with no live model behind it renders as a quiet notice with
    // a path to connect one - never as a companion pretending to answer.
    stubAnswer: packet.model_gateway_driver === "local_model_gateway",
    // The persona echo: the kernel repeats back the persona id that rode
    // this write, so provenance shows what actually traveled - never what
    // the UI merely intended.
    personaId: packet.persona_profile_id ?? "",
    // Memory recall, as the receipt recorded it: the real relevance score
    // and how many sources actually rode this answer's prompt. Zero means
    // no memory shaped the answer - the surface stays quiet then.
    recallScore:
      typeof packet.memory_relevance_score === "number" ? packet.memory_relevance_score : null,
    recallSourceCount: Number(packet.brain_recall_source_count ?? 0),
    recallReceiptId: packet.brain_recall_receipt_id ?? "",
    recallSources: []
  };
}

// Sessions ride the kernel's own session_id field - no new contract
// needed. The sidebar groups the projection by session; titles come from
// each session's first question.
export function sessionsFromProjection(packet) {
  const drafts = Array.isArray(packet?.drafts) ? packet.drafts : [];
  const sessions = new Map();
  for (const entry of drafts) {
    const id = entry.session_id ?? "twin_session_local_post_001";
    if (!sessions.has(id)) {
      sessions.set(id, { id, title: "", count: 0, lastOrder: 0 });
    }
    const session = sessions.get(id);
    if (!session.title && entry.draft_text) {
      session.title = entry.draft_text.length > 44 ? `${entry.draft_text.slice(0, 44)}…` : entry.draft_text;
    }
    session.count += 1;
    const order = Number(/(\d+)$/.exec(entry.answer_receipt_id ?? "")?.[1] ?? 0);
    if (order > session.lastOrder) {
      session.lastOrder = order;
    }
  }
  return [...sessions.values()].sort((a, b) => b.lastOrder - a.lastOrder);
}

export function conversationFromProjection(companions, packet, sessionId) {
  const drafts = (Array.isArray(packet?.drafts) ? packet.drafts : []).filter(
    (entry) => !sessionId || (entry.session_id ?? "twin_session_local_post_001") === sessionId
  );
  return drafts.flatMap((entry) => {
    const turns = [];
    if (entry.draft_text) {
      turns.push({ kind: "you", body: entry.draft_text, restored: true });
    }
    turns.push(answerTurn(companions, entry, true));
    return turns;
  });
}
