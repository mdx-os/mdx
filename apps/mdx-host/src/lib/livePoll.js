// Visibility-gated, jittered live polling. A hidden tab must not hammer the
// kernel: at 1000 engineers the old fixed 3s interval multiplied into a
// measured 621KB-per-tick-per-tab storm. Visible tabs poll at full cadence
// (with jitter so restarts do not synchronize into a stampede), hidden tabs
// drop to a slow heartbeat, and returning to the tab refreshes immediately.
export function startLivePoll({ isActive, refresh, activeMs = 5000, hiddenMs = 45000 }) {
  let timer = null;
  let stopped = false;

  const visible = () => document.visibilityState === "visible";

  async function tick() {
    if (stopped) return;
    if (isActive() && visible()) {
      try {
        await refresh();
      } catch {
        // refresh() owns its own error surface; polling just keeps its beat
      }
    }
    schedule();
  }

  function schedule() {
    if (stopped) return;
    const base = visible() ? activeMs : hiddenMs;
    const jittered = base * (0.8 + Math.random() * 0.4);
    timer = setTimeout(tick, jittered);
  }

  function onVisibilityChange() {
    if (visible() && isActive()) {
      clearTimeout(timer);
      tick();
    }
  }

  document.addEventListener("visibilitychange", onVisibilityChange);
  schedule();

  return () => {
    stopped = true;
    clearTimeout(timer);
    document.removeEventListener("visibilitychange", onVisibilityChange);
  };
}
