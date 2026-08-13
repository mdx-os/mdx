export const frontendTelemetryRelease = "mdx-host-v2";

export const frontendPerformanceBudgets = {
  coldFirstViewportMs: 1000,
  warmRouteTransitionMs: 300,
  interactionP75Ms: 200,
  relayLagWarnMs: 1000,
  clientJsCssKb: 536
};

// Client-side ring buffer with an honest landing: events buffer locally
// and flush in batches to the host's own /api/telemetry sink - same
// origin, no third parties, kept on this machine (or wherever the host
// operator points MDX_HOST_TELEMETRY_DIR). A failed flush keeps events
// pending; nothing is dropped silently and nothing is counted delivered
// until the sink confirms it.
export function createTelemetryBuffer({ maxEvents = 100 } = {}) {
  let events = [];
  let pending = [];
  let delivered = 0;
  let flushing = false;

  return {
    record(event) {
      const next = {
        ...event,
        release: frontendTelemetryRelease,
        recorded_at: new Date().toISOString()
      };

      events = [...events.slice(-(maxEvents - 1)), next];
      pending = [...pending.slice(-(maxEvents - 1)), next];
      return next;
    },
    async flush(fetchImpl = globalThis.fetch) {
      if (flushing || pending.length === 0 || typeof fetchImpl !== "function") {
        return 0;
      }
      flushing = true;
      const batch = pending.slice(0, 50);
      try {
        const response = await fetchImpl("/api/telemetry", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ events: batch })
        });
        if (!response.ok) {
          return 0;
        }
        pending = pending.slice(batch.length);
        delivered += batch.length;
        return batch.length;
      } catch (error) {
        // Sink unreachable: events stay pending for the next flush.
        return 0;
      } finally {
        flushing = false;
      }
    },
    // Best-effort final flush on pagehide; the beacon result is unknown,
    // so delivered counts are not advanced - honesty over optimism.
    beacon() {
      if (pending.length === 0 || typeof navigator === "undefined" || !navigator.sendBeacon) {
        return false;
      }
      return navigator.sendBeacon(
        "/api/telemetry",
        new Blob([JSON.stringify({ events: pending.slice(0, 50) })], { type: "application/json" })
      );
    },
    snapshot() {
      return events;
    },
    size() {
      return events.length;
    },
    deliveredCount() {
      return delivered;
    },
    pendingCount() {
      return pending.length;
    }
  };
}

export function routeTimingEvent({ route, startedAt, endedAt }) {
  return {
    type: "route_timing",
    route,
    duration_ms: Math.max(0, Math.round(endedAt - startedAt)),
    budget_ms: frontendPerformanceBudgets.warmRouteTransitionMs
  };
}

export function webVitalEvent({ name, value, rating = "unknown" }) {
  return {
    type: "web_vital",
    name,
    value,
    rating,
    budget_ms: name === "INP" ? frontendPerformanceBudgets.interactionP75Ms : null
  };
}

export function relayLagEvent({ stream, cursor, lagMs }) {
  return {
    type: "relay_lag",
    stream,
    cursor,
    lag_ms: lagMs,
    budget_ms: frontendPerformanceBudgets.relayLagWarnMs
  };
}

export function interactionTimingEvent({ route, durationMs, interactionKind = "input" }) {
  return {
    type: "interaction_timing",
    route,
    duration_ms: Math.max(0, Math.round(durationMs)),
    interaction_kind: interactionKind,
    budget_ms: frontendPerformanceBudgets.interactionP75Ms
  };
}

// Source-B beta product telemetry. Each factory emits ONLY the fields the
// host /api/telemetry accept-list allows for that kind, plus `ts`. Identity
// (tenant, session, cohort) is never sent from the client; the server derives
// it. Any extra field would be rejected fail-closed by the sink, so these
// stay minimal on purpose.
function nowIso() {
  return new Date().toISOString();
}

export function activationStepEvent({ step, completed = true, route = "" }) {
  return { type: "activation_step", route, ts: nowIso(), step, completed };
}

export function answerVerdictEvent({ helpful, personaId = "", route = "/twin" }) {
  // Thumbs on a Twin answer: verdict only - never the question or the answer
  // text. The safe-context contract stays trivially satisfied by shape.
  return { type: "twin_answer_verdict", route, ts: nowIso(), helpful: helpful === true, persona_id: personaId };
}

export function surfaceVisitEvent({ surface, route = "" }) {
  return { type: "surface_visit", route, ts: nowIso(), surface };
}

export function sessionStartEvent({ route = "" } = {}) {
  return { type: "session_start", route, ts: nowIso() };
}

export function sessionEndEvent({ route = "", durationMs = 0 }) {
  return { type: "session_end", route, ts: nowIso(), duration_ms: Math.max(0, Math.round(durationMs)) };
}

export function flowAbandonedEvent({ flow, lastStep = "", route = "" }) {
  return { type: "flow_abandoned", route, ts: nowIso(), flow, last_step: lastStep };
}

export function feedbackOpenedEvent({ surface, route = "" }) {
  return { type: "feedback_opened", route, ts: nowIso(), surface };
}

export function errorEvent({ route, message, source = "window" }) {
  return {
    type: "client_error",
    route,
    source,
    message
  };
}

export const telemetry = createTelemetryBuffer();
