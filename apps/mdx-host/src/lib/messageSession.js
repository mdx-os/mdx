// Message runtime helpers for the host route. JSON rides the same-origin
// kernel proxy; the live event stream is the one blessed exception - a
// direct WebSocket to the local relay. The subscription descriptor comes
// from the kernel's bridge metadata; this surface never constructs tenant
// ids, topics, or replay policy. The cursor seeds from the bridge once,
// then advances only from envelopes actually received - reconnects use the
// client-owned cursor, never a fresh server watermark.
import { createMdxClient, createRealtimeCursor } from "@mdx/client";
import { actorDisplayName } from "./actorPresentation.js";

export const threadsPath = "/messages/threads.json";
export const timelinePath = "/messages/thread-messages/projection.json";
export const sendPath = "/messages/thread-messages.json";
export const actionVerdictPath = "/messages/action-verdicts.json";
export const bridgePath = "/messages/relay-bridge.json";

const relayWsUrl = import.meta.env.VITE_MDX_MESSAGE_RELAY_WS ?? "ws://127.0.0.1:9000/messages/realtime/ws";

// The session signs every send - without it the client refuses to build
// actor admission and the write never leaves the browser (a latent bug
// the front-door handoff exposed: the composer was sessionless).
export function createMessageClient(session) {
  return createMdxClient({ baseUrl: "/api/kernel", routeCatalog: null, session });
}

// Replies ride the contract's own thread_id field: a reply's thread is
// "reply:<parent receipt id>" - the receipt id is the one identifier
// that is unique per message and present in both the projection and the
// relay envelope. Top-level messages keep the default thread.
export const replyThreadPrefix = "reply:";
// Reactions ride the same contract: thread "react:<parent receipt id>",
// body "+:emoji" or "-:emoji" - append-only toggles, folded at render.
export const reactThreadPrefix = "react:";

// A Message event now rides a typed envelope: content_type names the shape
// (text, card, action_request, action_response) and content_payload carries
// its structured fields. The body stays the plain-language line a human can
// read without opening anything. The projection carries the full envelope;
// the live relay sends a leaner one (no display name, no source receipt), so
// every field falls back gracefully.
export function normalizeMessage(message) {
  const actorId = String(message.actor_id ?? "");
  const inferredType = actorId.includes(":") ? actorId.split(":")[0] : "system";
  const actorType = String(message.actor_type ?? inferredType).toLowerCase();
  const threadId = String(message.thread_id ?? "");
  return {
    actorId,
    actor: actorDisplayName({
      actorId,
      displayName: message.actor_display_name,
      actorType
    }),
    actorType,
    body: message.body ?? "",
    contentType: String(message.content_type ?? "text").toLowerCase(),
    contentPayload: message.content_payload ?? "",
    messageReceiptKind: String(message.message_receipt_kind ?? ""),
    channelId: message.channel_id ?? "local-ops",
    receiptId: message.message_receipt_id ?? message.receipt_id ?? "",
    messageId: String(message.message_id ?? ""),
    createdAt: String(message.created_at ?? ""),
    sourceReceiptId: String(message.source_receipt_id ?? ""),
    sourceReceiptKind: String(message.source_receipt_kind ?? ""),
    replyTo: threadId.startsWith(replyThreadPrefix) ? threadId.slice(replyThreadPrefix.length) : "",
    reactTo: threadId.startsWith(reactThreadPrefix) ? threadId.slice(reactThreadPrefix.length) : "",
    sequence: Number(message.sequence ?? 0)
  };
}

// The device id names this browser across reconnects - observability for
// the relay operator, never identity (the auth token is identity).
function deviceId() {
  try {
    const stored = localStorage.getItem("mdx-relay-device");
    if (stored) return stored;
    const minted = `dev_${(globalThis.crypto?.randomUUID?.() ?? `${Date.now()}`).replace(/-/g, "").slice(0, 16)}`;
    localStorage.setItem("mdx-relay-device", minted);
    return minted;
  } catch (error) {
    return "dev_memory_only";
  }
}

export function connectRelay({ descriptor, cursor, onState, onEnvelope, onEvent, onGap }) {
  let socket = null;
  let stopped = false;
  let reconnectTimer = null;
  let reconnectDelay = 1000;
  let generation = 0;
  let expectedTopics = 0;
  let verdicts = [];

  // All topics answered: live only if every verdict says caught_up;
  // otherwise the surface fills from the projection first - "catching
  // up" is a state we show, never a gap we hide.
  async function settleVerdicts() {
    const gapped = verdicts.filter((verdict) => verdict.state !== "caught_up");
    if (gapped.length === 0) {
      onState?.("live");
      return;
    }
    onState?.("catching_up");
    try {
      const filled = await onGap?.(gapped);
      if (filled === false) {
        return;
      }
    } catch (error) {
      // The projection fill failed; the reconnect loop tries again.
      return;
    }
    onState?.("live");
  }

  function open() {
    if (stopped || !descriptor || typeof WebSocket === "undefined") {
      return;
    }
    generation += 1;
    verdicts = [];
    expectedTopics = 0;
    onState?.("connecting");
    try {
      socket = new WebSocket(relayWsUrl);
    } catch (error) {
      onState?.("off");
      schedule();
      return;
    }
    socket.addEventListener("message", (event) => {
      let payload;
      try {
        payload = JSON.parse(event.data);
      } catch (error) {
        return;
      }
      if (payload.type === "ready") {
        socket.send(
          JSON.stringify({
            ...descriptor.subscribe_payload,
            after_sequence: cursor.value(),
            device_id: deviceId(),
            connection_generation: generation
          })
        );
      } else if (payload.type === "ack" && payload.action === "subscribe") {
        reconnectDelay = 1000;
        expectedTopics = Number(payload.topic_count ?? 0);
        // Not live yet: the catch-up verdicts decide what this is.
        onState?.("syncing");
        if (expectedTopics === 0) onState?.("live");
      } else if (payload.type === "catch_up") {
        verdicts = [...verdicts, payload];
        if (verdicts.length >= expectedTopics) settleVerdicts();
      } else if (payload.type === "drain") {
        // The relay is restarting and said so: come back fast. The close
        // follows; the shared topics make any relay the right one.
        reconnectDelay = 500;
      } else if (payload.type === "envelope") {
        const envelope = payload.envelope ?? payload;
        cursor.advance(envelope[descriptor.sequence_field ?? "sequence"]);
        onEnvelope?.(envelope);
      } else {
        onEvent?.(payload);
      }
    });
    socket.addEventListener("close", () => {
      onState?.("off");
      schedule();
    });
    socket.addEventListener("error", () => {
      onState?.("off");
    });
  }

  function schedule() {
    if (stopped || reconnectTimer) {
      return;
    }
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      reconnectDelay = Math.min(reconnectDelay * 2, 15000);
      open();
    }, reconnectDelay);
  }

  open();
  return {
    send(message) {
      if (socket?.readyState === 1) {
        socket.send(JSON.stringify(message));
        return true;
      }
      return false;
    },
    stop() {
      stopped = true;
      clearTimeout(reconnectTimer);
      socket?.close();
    }
  };
}

export { createRealtimeCursor };
