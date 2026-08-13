// Browser notifications with the three-trigger doctrine: an interrupt is for
// a decision, not a status. Fires ONLY when a run finishes, needs a call, or
// fails - and only while the tab is hidden (a visible tab already shows it).
// Permission is asked on an explicit user gesture, never ambushed on load.
export const WEB_APP_VERSION = "0.9.0";

export function notificationsSupported() {
  return typeof Notification !== "undefined";
}

export function notificationState() {
  return notificationsSupported() ? Notification.permission : "unsupported";
}

export async function enableNotifications() {
  if (!notificationsSupported()) return "unsupported";
  return Notification.requestPermission();
}

// Compare the previous and next run lists; notify on meaningful transitions.
export function notifyRunTransitions(prevById, runs, { isRunning }) {
  if (!notificationsSupported() || Notification.permission !== "granted") return;
  if (document.visibilityState === "visible") return;
  for (const run of runs) {
    const prev = prevById.get(run.runId);
    if (!prev || !isRunning(prev.status) || isRunning(run.status)) continue;
    const failed = /error|cannot|fail/i.test(run.status);
    const needsCall = !failed && Boolean(run.branch);
    const title = failed ? "Run hit a wall" : needsCall ? "Ready for your call" : "Run finished";
    try {
      const notification = new Notification(title, {
        body: run.runTitle || run.runId,
        tag: `mdx-run-${run.runId}`,
        icon: "/icon-192.png"
      });
      notification.onclick = () => {
        window.focus();
        window.location.assign(`/forge/runs?run=${encodeURIComponent(run.runId)}`);
      };
    } catch (error) {
      // notification construction can throw in odd embed contexts; never fatal
    }
  }
}
