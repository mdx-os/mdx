import { actorDisplayName } from "../../lib/actorPresentation.js";

function canonicalActorId(actorId, selfActorId) {
  const raw = String(actorId ?? "").trim();
  const self = String(selfActorId ?? "").trim();
  if (self && (raw === self || raw === `human:${self}`)) return `human:${self}`;
  return raw;
}

function actionCount(value) {
  const count = Number(value);
  return Number.isFinite(count) && count > 0 ? count : 0;
}

/**
 * Fold the signed-in human's bare kernel actor and human-prefixed actor into
 * one roster entry. Other identities remain distinct even when their display
 * names happen to match.
 */
export function mergePresenceRoster(roster, selfActorId) {
  if (!Array.isArray(roster)) return [];

  const merged = new Map();
  for (const person of roster) {
    if (!person || typeof person !== "object") continue;
    const actorId = canonicalActorId(person.actor_id, selfActorId);
    if (!actorId) continue;

    const current = merged.get(actorId);
    if (!current) {
      merged.set(actorId, {
        ...person,
        actor_id: actorId,
        actor_type: actorId === `human:${selfActorId}` ? "human" : person.actor_type,
        action_count: actionCount(person.action_count)
      });
      continue;
    }

    const personIsNewer = String(person.last_active_at ?? "") > String(current.last_active_at ?? "");
    const latest = personIsNewer ? person : current;
    merged.set(actorId, {
      ...current,
      actor_id: actorId,
      actor_type: current.actor_type === "human" || person.actor_type === "human" ? "human" : current.actor_type,
      display_name: person.actor_type === "human" ? person.display_name : current.display_name,
      last_action: latest.last_action,
      last_active_at: latest.last_active_at,
      action_count: actionCount(current.action_count) + actionCount(person.action_count),
      active: Boolean(current.active || person.active)
    });
  }

  return [...merged.values()].map((person) => ({
    ...person,
    display_name: actorDisplayName({
      actorId: person.actor_id,
      displayName: person.display_name,
      selfActorId,
      actorType: person.actor_type
    })
  }));
}
