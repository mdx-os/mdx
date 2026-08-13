// Home landing helpers. The landing is a snapshot: the server load reads
// runtime truth through the kernel proxy and these helpers turn it into
// human sentences. Receipt ids carry a global anchor counter in their
// numeric suffix, so recency across surfaces falls out of the ids
// themselves - no wall-clock claims, no invented ordering.

export function receiptOrder(receiptId) {
  const match = /(\d+)$/.exec(String(receiptId ?? ""));
  return match ? Number(match[1]) : 0;
}

export function greetingFor(hour) {
  if (hour >= 5 && hour < 12) {
    return "Good morning.";
  }
  if (hour >= 12 && hour < 18) {
    return "Good afternoon.";
  }
  return "Good evening.";
}

function joinNaturally(parts) {
  if (parts.length <= 1) {
    return parts.join("");
  }
  return `${parts.slice(0, -1).join(", ")} and ${parts[parts.length - 1]}`;
}

function plural(count, one, many) {
  return count === 1 ? one : many;
}

// One warm sentence carrying the real counts. Null when nothing has
// happened yet, so the page can invite instead of reciting zeros.
export function pulseLine({ twinCount = 0, messageCount = 0, pagesCount = 0 } = {}) {
  const parts = [];
  if (twinCount > 0) {
    parts.push(`Twin has answered ${twinCount} ${plural(twinCount, "question", "questions")}`);
  }
  if (messageCount > 0) {
    parts.push(`the team has ${messageCount} ${plural(messageCount, "message", "messages")}`);
  }
  if (pagesCount > 0) {
    parts.push(`the library keeps ${pagesCount} ${plural(pagesCount, "page", "pages")}`);
  }
  if (parts.length === 0) {
    return null;
  }
  const sentence = joinNaturally(parts);
  return `${sentence.charAt(0).toUpperCase()}${sentence.slice(1)}.`;
}

// Recent moments across surfaces, newest first by receipt anchor order.
// Each moment is a real thing somebody did, in their own words.
export function momentsFrom({ twinDrafts = [], messages = [] } = {}, limit = 8) {
  const moments = [];
  for (const draft of twinDrafts) {
    if (!draft?.draft_text) {
      continue;
    }
    moments.push({
      text: `“${draft.draft_text}”`,
      meta: "You asked · Twin answered",
      href: "/twin",
      receiptId: draft.answer_receipt_id ?? draft.draft_receipt_id ?? "",
      order: receiptOrder(draft.answer_receipt_id ?? draft.draft_receipt_id)
    });
  }
  for (const message of messages) {
    if (!message?.body) {
      continue;
    }
    const rawActor = String(message.actor_display_name ?? message.actor_id ?? "someone").replace(/^[a-z]+:/, "");
    const actor = rawActor === "local_user" || rawActor === "local user" ? "you" : rawActor;
    moments.push({
      text: `“${message.body}”`,
      meta: `${actor} · Message`,
      href: "/message",
      receiptId: message.message_receipt_id ?? "",
      order: receiptOrder(message.message_receipt_id)
    });
  }
  moments.sort((a, b) => b.order - a.order);
  return moments.slice(0, limit);
}
