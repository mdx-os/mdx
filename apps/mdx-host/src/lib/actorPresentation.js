const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu;

function actorToken(actorId) {
  return String(actorId ?? "").trim().replace(/^[a-z]+:/u, "");
}

export function actorDisplayName({ actorId = "", displayName = "", selfActorId = "", actorType = "human" } = {}) {
  const rawActor = String(actorId ?? "").trim();
  const rawSelf = String(selfActorId ?? "").trim();
  if (rawSelf && (rawActor === rawSelf || rawActor === `human:${rawSelf}`)) return "You";

  const candidate = String(displayName ?? "").trim().replace(/^[a-z]+:/u, "");
  const token = actorToken(rawActor);
  const exposesIdentifier =
    !candidate ||
    candidate === rawActor ||
    candidate === token ||
    UUID_PATTERN.test(candidate) ||
    UUID_PATTERN.test(token) && candidate.includes(token);

  if (!exposesIdentifier) return candidate;
  if (String(actorType).toLowerCase() === "agent") return "Agent";
  if (String(actorType).toLowerCase() === "system") return "MDx";
  return "Member";
}
