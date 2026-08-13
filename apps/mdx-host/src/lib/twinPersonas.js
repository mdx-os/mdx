// Custom companions: v1's personas-as-data, reborn. In v1 (MDx-Stack
// personas/) a persona was a record - display name, values, principles,
// voice style - injected into the prompt at a marker. Here the same idea
// rides v2's governed write: the persona lives on this machine (honest
// tiering, like the You profile), its id travels as companion_id on every
// receipt, and its layer travels as companion_context - prompt-only,
// capped server-side, never written to receipts.
const PERSONAS_KEY = "mdx-twin-personas";

export const emptyPersona = {
  id: "",
  name: "",
  purpose: "",
  voice: ""
};

export function personaIdFor(name) {
  const slug = String(name ?? "")
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .slice(0, 32);
  return slug ? `custom_${slug}` : "";
}

export function loadPersonas() {
  try {
    const raw = localStorage.getItem(PERSONAS_KEY);
    const parsed = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed)
      ? parsed.filter((entry) => entry && entry.id && entry.name)
      : [];
  } catch (error) {
    return [];
  }
}

export function savePersonas(personas) {
  try {
    localStorage.setItem(PERSONAS_KEY, JSON.stringify(personas));
    return true;
  } catch (error) {
    return false;
  }
}

// The picker entry: a custom persona looks and behaves like a built-in
// companion on the front door. Stance doubles as the receipt's
// companion_stance - short, honest, derived from what they typed.
export function companionFrom(persona) {
  return {
    id: persona.id,
    name: persona.name,
    stance: "Your companion",
    shape: persona.purpose || "answer the way this companion was set up to",
    opener: "",
    custom: true
  };
}

// The companion's own prompt layer - what v1 injected at its marker.
// Their words verbatim; the foundation's hard rules still override.
export function companionLayerFrom(persona) {
  const lines = [`You are ${persona.name}, a companion this person created inside MDx.`];
  if (persona.purpose?.trim()) {
    lines.push(`What you are for: ${persona.purpose.trim()}`);
  }
  if (persona.voice?.trim()) {
    lines.push(`How you think and sound, in your creator's own words:\n${persona.voice.trim()}`);
  }
  return lines.join("\n");
}
