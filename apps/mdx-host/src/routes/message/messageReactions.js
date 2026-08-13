function sameActor(left, right) {
  const leftValue = String(left ?? "");
  const rightValue = String(right ?? "");
  if (leftValue === rightValue) return true;
  const split = (value) => {
    const match = value.match(/^([a-z]+):(.*)$/i);
    return match ? { kind: match[1].toLowerCase(), id: match[2] } : { kind: "", id: value };
  };
  const leftActor = split(leftValue);
  const rightActor = split(rightValue);
  if (leftActor.kind && rightActor.kind) {
    return leftActor.kind === rightActor.kind && leftActor.id === rightActor.id;
  }
  const typed = leftActor.kind ? leftActor : rightActor;
  const plain = leftActor.kind ? rightActor : leftActor;
  return ["human", "user", "owner"].includes(typed.kind) && typed.id === plain.id;
}

export function foldReactions(messages, selfActor) {
  const byParent = new Map();
  const ordered = [...messages].sort(
    (left, right) => Number(left.sequence ?? 0) - Number(right.sequence ?? 0)
  );
  for (const message of ordered) {
    if (!message.reactTo) continue;
    const active = message.body?.startsWith("+:")
      ? true
      : message.body?.startsWith("-:")
        ? false
        : null;
    if (active == null) continue;
    const emoji = message.body.slice(2);
    if (!emoji) continue;
    const parent = byParent.get(message.reactTo) ?? new Map();
    const actors = parent.get(emoji) ?? new Map();
    actors.set(message.actorId || message.actor, active);
    parent.set(emoji, actors);
    byParent.set(message.reactTo, parent);
  }

  const folded = new Map();
  for (const [parent, emojis] of byParent) {
    const chips = [];
    for (const [emoji, actors] of emojis) {
      const activeActors = [...actors.entries()].filter(([, active]) => active);
      if (activeActors.length === 0) continue;
      chips.push({
        emoji,
        count: activeActors.length,
        mine: activeActors.some(([actor]) => sameActor(actor, selfActor))
      });
    }
    if (chips.length > 0) {
      folded.set(parent, chips.sort((left, right) => left.emoji.localeCompare(right.emoji)));
    }
  }
  return folded;
}
