// Shared SSE client for the Slick Rails streaming bus (Codex contract 2026-06-27).
// Envelope: { seq, kind, run_id, stage, receipt_id?, model, payload, production_write_allowed }
// kinds: stage_start, token_delta, tool_call, tool_result, proof_output, diff_ready, terminal, error, heartbeat.
// Receipt-grade worker progress today (no provider token_delta yet); the fold handles
// token_delta when the adapter slice lands. Late viewers reconstruct from the projection.

// Resolve a kernel stream route to the browser proxy origin.
export function kernelStreamUrl(route) {
  if (!route) return "";
  return route.startsWith("/api/kernel") ? route : `/api/kernel${route.startsWith("/") ? "" : "/"}${route}`;
}

// Subscribe to a run/deliberation SSE stream. Returns an unsubscribe function.
// Reconnects itself with jittered backoff when the transport drops (kernel
// restart, deploy, sleep/wake): a thousand tabs must come back staggered,
// not as a synchronized stampede, and a dropped stream must never silently
// freeze a live trail. Late frames reconstruct from the projection by
// contract, so a reconnect replay is safe.
export function subscribeStream(route, { onEvent, onError, onClose, onReconnect } = {}) {
  const url = kernelStreamUrl(route);
  if (!url || typeof EventSource === "undefined") return () => {};

  let es = null;
  let stopped = false;
  let retryTimer = null;
  let attempt = 0;

  function open() {
    if (stopped) return;
    try {
      es = new EventSource(url);
    } catch (error) {
      onError?.(error);
      scheduleRetry(error);
      return;
    }
    es.onopen = () => {
      if (attempt > 0) onReconnect?.(attempt);
      attempt = 0;
    };
    es.onmessage = (m) => {
      let ev;
      try {
        ev = JSON.parse(m.data);
      } catch (error) {
        return;
      }
      if (!ev || ev.kind === "heartbeat") return;
      onEvent?.(ev);
      if (ev.kind === "terminal" || ev.kind === "error") {
        stopped = true;
        es.close();
        onClose?.(ev);
      }
    };
    es.onerror = (error) => {
      onError?.(error);
      if (stopped) return;
      try {
        es.close();
      } catch {
        /* already closed */
      }
      scheduleRetry(error);
    };
  }

  function scheduleRetry() {
    if (stopped) return;
    attempt += 1;
    // 1s, 2s, 4s, 8s... capped at 30s, with +-30% jitter.
    const base = Math.min(1000 * 2 ** (attempt - 1), 30000);
    const delay = base * (0.7 + Math.random() * 0.6);
    retryTimer = setTimeout(open, delay);
  }

  open();

  return () => {
    stopped = true;
    clearTimeout(retryTimer);
    try {
      es?.close();
    } catch (error) {
      /* already closed */
    }
  };
}
