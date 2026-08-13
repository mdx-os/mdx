// The operator's profile: who they are and how they like to work.
// Honest tiering, by design:
//   - The kernel owns the receipt-backed activation profile contract.
//     localStorage remains a machine-local fallback so the UI still works
//     before the local kernel is reachable.
//   - The style dimensions DO ride every governed Twin write through the
//     contract's own free-form fields (persona_profile_id, prompt_shape),
//     so the persona's application is receipt-backed and visible in each
//     answer's provenance. Nothing is injected silently - the profile
//     card shows exactly the id and framing that travel.
//   - Writing samples are stored for the day Twin reads context sources
//     (CTX assembly); they do not ride writes today and this module says
//     so. When a governed operator-profile contract lands, load/save swap
//     to it with no surface rework.

const PROFILE_KEY = "mdx-you";
const THEME_KEY = "mdx-theme";
const ACTIVATION_PROFILE_ROUTE = "/activation/profile.json";

export const roles = [
  { id: "engineering", label: "Engineering", line: "You build it. MDx shows its work." },
  { id: "product", label: "Product", line: "You decide what matters. MDx keeps the why close." },
  { id: "leadership", label: "Leadership", line: "You set direction. MDx keeps the signal clean." },
  { id: "design", label: "Design", line: "You shape the experience. MDx sweats the details with you." }
];

export const styleDimensions = [
  {
    id: "register",
    label: "Answers should be",
    options: [
      { id: "direct", label: "Direct", line: "Lead with the point." },
      { id: "narrative", label: "Narrative", line: "Walk me through it." }
    ]
  },
  {
    id: "grounding",
    label: "Convince me with",
    options: [
      { id: "data_first", label: "Data first", line: "Numbers, then the story." },
      { id: "story_first", label: "Story first", line: "Context, then the numbers." }
    ]
  },
  {
    id: "guidance",
    label: "When I have to choose",
    options: [
      { id: "options", label: "Give me options", line: "Lay out the paths." },
      { id: "recommendation", label: "Make the call", line: "Recommend one, defend it." }
    ]
  }
];

export const emptyProfile = {
  name: "",
  role: "",
  style: { register: "", grounding: "", guidance: "" },
  defaultCompanionId: "",
  samples: [],
  // The voice layer: how you think and how you sound. Composed into the
  // persona context that rides every Twin question.
  principles: "",
  toneKeywords: "",
  avoidPhrases: "",
  quirks: { ellipsis: false, arrows: false, casualGreetings: false }
};

export function loadProfile() {
  try {
    const raw = localStorage.getItem(PROFILE_KEY);
    if (!raw) {
      return structuredClone(emptyProfile);
    }
    const parsed = JSON.parse(raw);
    return {
      ...structuredClone(emptyProfile),
      ...parsed,
      style: { ...emptyProfile.style, ...(parsed.style ?? {}) },
      quirks: { ...emptyProfile.quirks, ...(parsed.quirks ?? {}) },
      samples: Array.isArray(parsed.samples) ? parsed.samples : []
    };
  } catch (error) {
    return structuredClone(emptyProfile);
  }
}

export function saveProfile(profile) {
  try {
    localStorage.setItem(PROFILE_KEY, JSON.stringify(profile));
    return true;
  } catch (error) {
    return false;
  }
}

export async function loadServerProfile(fetchImpl = fetch) {
  try {
    const response = await fetchImpl(`/api/kernel${ACTIVATION_PROFILE_ROUTE}`, {
      signal: AbortSignal.timeout(1500)
    });
    if (!response.ok) {
      return null;
    }
    const packet = await response.json();
    if (!packet?.recorded || !packet?.profile) {
      return null;
    }
    return profileFromActivation(packet.profile);
  } catch (error) {
    return null;
  }
}

export async function loadProfileWithServer(fetchImpl = fetch) {
  const serverProfile = await loadServerProfile(fetchImpl);
  if (serverProfile) {
    try {
      saveProfile(serverProfile);
    } catch (error) {
      // local cache is best effort; the kernel profile remains the source.
    }
    return serverProfile;
  }
  return loadProfile();
}

export async function saveServerProfile(profile, fetchImpl = fetch) {
  try {
    const response = await fetchImpl(`/api/kernel${ACTIVATION_PROFILE_ROUTE}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(profile),
      signal: AbortSignal.timeout(3000)
    });
    const packet = response.ok ? await response.json() : null;
    if (packet?.recorded) {
      saveProfile(profile);
      return packet;
    }
    return packet ?? { recorded: false, reason: "profile route unavailable" };
  } catch (error) {
    saveProfile(profile);
    return { recorded: false, reason: "saved on this machine only" };
  }
}

export function profileFromActivation(profile) {
  const next = structuredClone(emptyProfile);
  next.name = profile?.name ?? "";
  next.role = profile?.role ?? "";
  next.voice = profile?.voice ?? "";
  next.style = styleFromActivation(profile?.style);
  next.principles = profile?.principles ?? "";
  next.toneKeywords = profile?.toneKeywords ?? profile?.tone_keywords ?? "";
  next.avoidPhrases = profile?.avoidPhrases ?? profile?.avoid_phrases ?? "";
  return next;
}

function styleFromActivation(style) {
  const next = { ...emptyProfile.style };
  if (!style) return next;
  if (typeof style === "string") {
    for (const part of style.split(/[;,]/).map((value) => value.trim())) {
      const [key, value] = part.split(":").map((piece) => piece?.trim());
      if (key && value && key in next) {
        next[key] = value;
      }
    }
    if (!next.register && (style === "direct" || style === "narrative")) {
      next.register = style;
    }
    return next;
  }
  return { ...next, ...style };
}

export function loadTheme() {
  try {
    const stored = localStorage.getItem(THEME_KEY);
    return stored === "light" || stored === "dark" ? stored : "system";
  } catch (error) {
    return "system";
  }
}

export function applyTheme(mode) {
  try {
    if (mode === "light" || mode === "dark") {
      localStorage.setItem(THEME_KEY, mode);
      document.documentElement.dataset.theme = mode;
    } else {
      localStorage.removeItem(THEME_KEY);
      document.documentElement.dataset.theme = matchMedia("(prefers-color-scheme: light)").matches
        ? "light"
        : "dark";
    }
    return true;
  } catch (error) {
    return false;
  }
}

// The persona id that travels with every Twin question. Derived, stable,
// and shown verbatim on the profile card - never a hidden value.
export function personaIdFrom(profile) {
  const parts = [profile?.role, profile?.style?.register, profile?.style?.grounding, profile?.style?.guidance]
    .filter(Boolean);
  if (parts.length === 0) {
    return "";
  }
  return `you_v1_${parts.join("_")}`;
}

// The question framing that travels with every Twin question: the
// companion's own shape plus how this person likes to be answered.
export function promptShapeFrom(profile, companionShape) {
  const wants = [];
  if (profile?.style?.register === "direct") wants.push("lead with the point");
  if (profile?.style?.register === "narrative") wants.push("walk through the reasoning");
  if (profile?.style?.grounding === "data_first") wants.push("numbers before story");
  if (profile?.style?.grounding === "story_first") wants.push("context before numbers");
  if (profile?.style?.guidance === "options") wants.push("lay out options");
  if (profile?.style?.guidance === "recommendation") wants.push("recommend one path");
  if (wants.length === 0) {
    return companionShape ?? "";
  }
  return `${companionShape ?? "answer"}; this person prefers: ${wants.join(", ")}`;
}

// One human sentence for the profile card: what MDx currently believes.
export function beliefLine(profile) {
  const role = roles.find((entry) => entry.id === profile?.role);
  const pieces = [];
  if (role) {
    pieces.push(`you work in ${role.label.toLowerCase()}`);
  }
  if (profile?.style?.register === "direct") pieces.push("you want the point first");
  if (profile?.style?.register === "narrative") pieces.push("you want the reasoning shown");
  if (profile?.style?.grounding === "data_first") pieces.push("numbers persuade you");
  if (profile?.style?.grounding === "story_first") pieces.push("context persuades you");
  if (profile?.style?.guidance === "options") pieces.push("you like choosing between paths");
  if (profile?.style?.guidance === "recommendation") pieces.push("you want a clear recommendation");
  if (pieces.length === 0) {
    return "";
  }
  return `${pieces.join(", ")}.`;
}

// The person's own prompt layer - v1's "{Name}'s Layer", now composed
// from the You surface instead of hardcoded. This is what makes Twin
// sound like THEM. Sent with each question; never recited back.
export function personaContextFrom(profile) {
  if (!profile) {
    return "";
  }
  const lines = [];
  if (profile.name) {
    lines.push(`Name: ${profile.name}.`);
  }
  if (profile.role) {
    lines.push(`They work in ${profile.role}.`);
  }
  const styleWants = [];
  if (profile.style?.register === "direct") styleWants.push("lead with the point");
  if (profile.style?.register === "narrative") styleWants.push("walk through the reasoning");
  if (profile.style?.grounding === "data_first") styleWants.push("numbers before story");
  if (profile.style?.grounding === "story_first") styleWants.push("context before numbers");
  if (profile.style?.guidance === "options") styleWants.push("lay out options");
  if (profile.style?.guidance === "recommendation") styleWants.push("recommend one path and defend it");
  if (styleWants.length > 0) {
    lines.push(`How they like answers: ${styleWants.join(", ")}.`);
  }
  if (profile.principles?.trim()) {
    lines.push(`Their principles, in their own words:\n${profile.principles.trim()}`);
  }
  if (profile.toneKeywords?.trim()) {
    lines.push(`Their tone: ${profile.toneKeywords.trim()}.`);
  }
  if (profile.avoidPhrases?.trim()) {
    lines.push(`Never use these phrases: ${profile.avoidPhrases.trim()}.`);
  }
  const quirkLines = [];
  if (profile.quirks?.casualGreetings) {
    quirkLines.push(
      "greet casually and briefly (a 'hey' gets a 'hey hey - what's up?', never a paragraph)"
    );
    quirkLines.push(
      "open dives casually too: 'Alright cool... let's think through this', 'Check it out...', 'Ok so here's the deal...'"
    );
  }
  if (profile.quirks?.ellipsis) {
    quirkLines.push(
      "use '...' naturally for pacing, like thinking out loud ('Hmm... ok so the real question is...')"
    );
  }
  if (profile.quirks?.arrows) {
    quirkLines.push("prefer '->' arrows over bullet points for lists");
  }
  if (quirkLines.length > 0) {
    lines.push(`Voice quirks to mirror: ${quirkLines.join("; ")}.`);
  }
  return lines.join("\n");
}
