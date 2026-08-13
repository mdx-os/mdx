export function buildMessageCatchUp(messages = [], limit = 12) {
  const topLevel = messages.filter((message) => !message.replyTo && !message.reactTo);
  const recent = topLevel.slice(-Math.max(1, Math.min(limit, 50)));
  const participants = [...new Set(recent.map((message) => String(message.actor || "someone")))];
  const highlights = recent.slice(-4).map((message) => ({
    id: String(message.receiptId || message.messageId || `${message.sequence || 0}`),
    actor: String(message.actor || "someone"),
    body: String(message.body ?? "").trim().slice(0, 180)
  }));
  return { recent, participants, highlights };
}

export function composeMessageWithAttachment(text = "", rawUrl = "", rawLabel = "") {
  const message = String(text).trim();
  const url = String(rawUrl).trim();
  if (!url) return { body: message, error: "" };

  let safeUrl = "";
  if (url.startsWith("/") && !url.startsWith("//") && !/[\s()[\]]/.test(url)) {
    try {
      const parsed = new URL(url, "https://mdx.local");
      const allowed = ["/pages", "/forge", "/evidence", "/message", "/twin", "/memory"];
      const knownObjectRoute = allowed.some((prefix) => parsed.pathname === prefix || parsed.pathname.startsWith(`${prefix}/`));
      if (parsed.origin === "https://mdx.local" && knownObjectRoute) {
        safeUrl = `${parsed.pathname}${parsed.search}${parsed.hash}`;
      }
    } catch (error) {
      // The returned error keeps malformed product paths out of Message.
    }
  } else if (!/\s/.test(url)) {
    try {
      const parsed = new URL(url);
      if (parsed.protocol === "https:" || parsed.protocol === "http:") safeUrl = parsed.href;
    } catch (error) {
      // The returned error keeps malformed or dangerous web links out of Message.
    }
  }
  if (!safeUrl) return { body: "", error: "Use an MDx path or an http(s) link." };
  // Parentheses terminate Markdown link targets. Percent-encode them after URL
  // parsing so the composed message cannot append a second link or payload.
  safeUrl = safeUrl.replaceAll("(", "%28").replaceAll(")", "%29");
  const label = (String(rawLabel).trim() || "Attached MDx object")
    .replaceAll("\\", "\\\\")
    .replaceAll("[", "\\[")
    .replaceAll("]", "\\]");
  return { body: `${message}${message ? "\n\n" : ""}[${label}](${safeUrl})`, error: "" };
}
