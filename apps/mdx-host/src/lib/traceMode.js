// Trace mode: the system-wide trust switch. Off by default, so every surface
// reads clean - the receipts, provenance, memory recall, and boundaries stay
// out of the way. Flip the shield in the shell and the proof appears across
// MDx, so anyone who wants to know why to trust the contents can see the depth
// on demand. The choice persists so a demo stays in the mode you set.
import { writable } from "svelte/store";

const KEY = "mdx-trace-mode";

export const traceMode = writable(false);

export function toggleTrace() {
  traceMode.update((on) => {
    const next = !on;
    try {
      localStorage.setItem(KEY, next ? "on" : "off");
    } catch (error) {
      // best effort - private mode / no storage
    }
    return next;
  });
}

// Called once on the client so the server render stays stable (off) and the
// saved choice is applied after hydration.
export function hydrateTrace() {
  try {
    traceMode.set(localStorage.getItem(KEY) === "on");
  } catch (error) {
    // best effort
  }
}
