export const workspaceStorageKey = "mdx-twin-workspace-v1";

export const emptyWorkspace = Object.freeze({ name: "", instructions: "", files: [] });

export function normalizeWorkspace(value = {}) {
  return {
    name: String(value.name ?? "").trim().slice(0, 80),
    instructions: String(value.instructions ?? "").trim().slice(0, 2000),
    files: Array.isArray(value.files)
      ? value.files.map((file) => String(file).trim()).filter(Boolean).slice(0, 12)
      : []
  };
}

export function workspaceContext(value) {
  const workspace = normalizeWorkspace(value);
  if (!workspace.name && !workspace.instructions && workspace.files.length === 0) return "";
  return [
    workspace.name ? `Active project: ${workspace.name}` : "",
    workspace.instructions ? `Project instructions: ${workspace.instructions}` : "",
    workspace.files.length ? `Scoped references: ${workspace.files.join(", ")}` : ""
  ].filter(Boolean).join("\n");
}

export function applyConversationPreference(current, id, patch) {
  return { ...current, [id]: { ...(current[id] ?? {}), ...patch } };
}

export function sortedVisibleSessions(sessions, preferences, query = "", archived = false) {
  const needle = query.trim().toLowerCase();
  return sessions
    .filter(
      (session) =>
        !preferences[session.id]?.hidden && Boolean(preferences[session.id]?.archived) === archived
    )
    .map((session) => ({
      ...session,
      pinned: Boolean(preferences[session.id]?.pinned),
      archived: Boolean(preferences[session.id]?.archived),
      displayTitle: preferences[session.id]?.title || session.title || "Conversation"
    }))
    .filter((session) => !needle || session.displayTitle.toLowerCase().includes(needle))
    .sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.lastOrder - a.lastOrder);
}

export function researchPreflightState(responseOk, packet) {
  if (
    !responseOk ||
    !packet ||
    packet.status === "REFUSED" ||
    !String(packet.control_receipt_id ?? "").trim()
  ) {
    return "unavailable";
  }
  return packet.connector_execution_allowed === true ? "ready" : "blocked";
}

export function activeDraftReceiptId(turns, projection, sessionId) {
  const currentTurn = [...(Array.isArray(turns) ? turns : [])]
    .reverse()
    .find((turn) => turn?.restored !== true && String(turn?.draftReceiptId ?? "").trim());
  if (currentTurn) return String(currentTurn.draftReceiptId);
  const currentDraft = [...(Array.isArray(projection?.drafts) ? projection.drafts : [])]
    .reverse()
    .find(
      (entry) =>
        String(entry?.session_id ?? "") === String(sessionId ?? "") &&
        String(entry?.draft_receipt_id ?? "").trim()
    );
  return String(currentDraft?.draft_receipt_id ?? "");
}

export function conversationMarkdown(title, turns) {
  const body = turns
    .filter((turn) => turn?.body)
    .map((turn) => `## ${turn.kind === "you" ? "You" : turn.companionName || "Twin"}\n\n${turn.body}`)
    .join("\n\n");
  return `# ${title || "Twin conversation"}\n\n${body}\n`;
}

export function copiedConversationContext(title, turns, charCap = 12000) {
  return conversationMarkdown(title, turns).slice(0, charCap);
}

export function attachmentDisposition(file) {
  const name = String(file?.name ?? "");
  const type = String(file?.type ?? "").toLowerCase();
  if (type === "application/pdf" || /\.pdf$/i.test(name)) return "pdf";
  if (type.startsWith("text/") || /\.(txt|md|markdown|csv|tsv|json|log|ya?ml|html|xml|js|ts|py|rs|go|java|rb|sh|sql)$/i.test(name)) return "text";
  if (type.startsWith("image/") || /\.(png|jpe?g|heic|webp|gif)$/i.test(name)) return "blocked_image";
  if (/\.(docx|xlsx|pptx)$/i.test(name)) return "blocked_office";
  return "unsupported";
}
