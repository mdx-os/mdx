export function shouldNotifyMessage({ enabled, visibilityState, actor, selfActor, muted, liveState }) {
  return Boolean(enabled)
    && visibilityState !== "visible"
    && actor !== selfActor
    && !muted
    && liveState === "live";
}

export function createMessageNotification(NotificationConstructor, title, options) {
  if (typeof NotificationConstructor !== "function") return null;
  try {
    return new NotificationConstructor(title, options);
  } catch (error) {
    return null;
  }
}
