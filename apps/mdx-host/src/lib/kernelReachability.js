// Kernel reachability: the honest spine. Every surface reads one shared
// signal for "is the local kernel answering?" so the app never renders a
// live composer, a normal greeting, or an empty list when the kernel is
// actually down. Offline is a state, not an error, and a slow-but-alive
// kernel (a busy read that times out) is told apart from a dead one.
//
// The store carries:
//   online       - the last liveness probe answered
//   degraded     - alive but slow (a probe timed out while the kernel was reachable)
//   failedStreak - consecutive probes that did not answer cleanly
//   lastError    - the last real transport error, for the Diagnose disclosure
//   baseUrl      - the kernel origin behind the browser proxy
//   checkedAt    - when the last probe resolved (epoch ms)
import { writable } from "svelte/store";

// A dedicated, cheap liveness route - NOT a data read. version.json is a
// tiny contract fingerprint the kernel always serves, so a slow product
// projection can never masquerade as "offline".
const LIVENESS_ROUTE = "/api/kernel/version.json";
const PROBE_TIMEOUT_MS = 2500;
// One timeout is "slow", not "down": only after this many consecutive
// failures do we call the kernel offline, so a single busy moment stays amber.
const OFFLINE_AFTER_STREAK = 2;

export const kernelReachability = writable({
  online: true,
  degraded: false,
  failedStreak: 0,
  lastError: "",
  baseUrl: "",
  checkedAt: 0
});

// Classify a fetch failure: a timeout means the kernel is reachable but slow
// (AbortSignal.timeout raises a TimeoutError / AbortError); anything else
// (connection refused, DNS, network down) means it is not answering at all.
function isSlow(error) {
  const name = error?.name ?? "";
  return name === "TimeoutError" || name === "AbortError";
}

// Feed the store from a probe result. Loaders and the heartbeat both call
// this, so a server-side liveness read and the client heartbeat agree on the
// same honest state instead of two surfaces disagreeing about the kernel.
export function noteProbe({ ok, slow = false, error = "", baseUrl } = {}) {
  kernelReachability.update((state) => {
    const next = { ...state, checkedAt: nowMs() };
    if (baseUrl != null) next.baseUrl = baseUrl;
    if (ok) {
      next.online = true;
      next.degraded = false;
      next.failedStreak = 0;
      next.lastError = "";
      return next;
    }
    next.failedStreak = state.failedStreak + 1;
    if (error) next.lastError = String(error);
    if (slow) {
      // Alive but slow: stay online (amber) until the streak says otherwise.
      next.degraded = true;
      next.online = next.failedStreak < OFFLINE_AFTER_STREAK ? true : false;
    } else {
      // A hard transport failure is offline immediately - no pretending.
      next.online = false;
      next.degraded = false;
    }
    return next;
  });
}

// nowMs avoids Date.now() directly so the module stays testable; the browser
// always has performance.timeOrigin + performance.now().
function nowMs() {
  if (typeof performance !== "undefined" && performance.timeOrigin) {
    return performance.timeOrigin + performance.now();
  }
  return 0;
}

// Seed the store from the server load's liveness read so the first paint
// already knows the truth (no flash of "online" before the first heartbeat).
export function seedReachability({ reachable, error = "", baseUrl = "" } = {}) {
  noteProbe({ ok: reachable !== false, error: reachable === false ? error : "", baseUrl });
}

// A single probe. Returns the classified result and updates the store.
async function probeOnce(fetchImpl = fetch) {
  try {
    const response = await fetchImpl(LIVENESS_ROUTE, {
      signal: AbortSignal.timeout(PROBE_TIMEOUT_MS)
    });
    noteProbe({ ok: response.ok, error: response.ok ? "" : `kernel returned ${response.status}` });
    return response.ok;
  } catch (error) {
    noteProbe({ ok: false, slow: isSlow(error), error: error?.message ?? String(error) });
    return false;
  }
}

// The heartbeat: probe on mount and on a steady beat, but never while the tab
// is hidden (no point paying for a probe nobody is looking at), and probe
// immediately when the operator returns to the tab so a stale banner clears
// the moment they look. Returns a stop function for onDestroy.
export function startKernelHeartbeat({ intervalMs = 15000 } = {}) {
  if (typeof document === "undefined") return () => {};
  let stopped = false;
  let timer = null;

  const visible = () => document.visibilityState === "visible";

  const beat = async () => {
    if (stopped || !visible()) return;
    await probeOnce();
  };

  const schedule = () => {
    if (stopped) return;
    timer = setTimeout(async () => {
      await beat();
      schedule();
    }, intervalMs);
  };

  const onVisibility = () => {
    if (visible()) beat();
  };

  beat();
  schedule();
  document.addEventListener("visibilitychange", onVisibility);

  return () => {
    stopped = true;
    clearTimeout(timer);
    document.removeEventListener("visibilitychange", onVisibility);
  };
}

// Force a probe now (the banner's Retry, after invalidateAll refetches data).
export function retryReachability() {
  return probeOnce();
}
