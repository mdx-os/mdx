// Cross-MDx awareness: how Twin sees what is moving across the other
// surfaces. Every signal here is read from a real kernel projection -
// the counts and lines are runtime truth, never invented, and a surface
// that did not answer simply contributes nothing (fail-soft). The digest
// feeds two things: the proactive "Across MDx" panel in Twin's empty
// state, and a capped worldContext string the model can fold into an
// answer when you ask Twin about the company.

const MAX_CONTEXT_CHARS = 1800;

function num(value) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

function plural(count, one, many) {
  return count === 1 ? one : many;
}

function tidy(text, max = 110) {
  const line = String(text ?? "").replace(/\s+/g, " ").trim();
  return line.length > max ? `${line.slice(0, max - 1)}…` : line;
}

// The latest Forge run, said the way a person would: what state it
// landed in, not the raw RUN_FINISHED_DONE enum.
function forgeLatestPhrase(run) {
  if (!run) return "";
  const status = String(run.status ?? "").toLowerCase();
  if (status === "done" || run.finished) {
    const turns = num(run.turns);
    return turns > 0 ? `the latest finished after ${turns} ${plural(turns, "turn", "turns")}` : "the latest finished";
  }
  if (status === "failed" || status === "error") return "the latest stopped short";
  if (status === "running" || status === "in_progress") return "one is running now";
  return run.latest_event ? `the latest is ${String(run.latest_event).replace(/_/g, " ")}` : "";
}

// Each app contributes at most one signal: the single most useful "here
// is where this surface stands" line, with somewhere to go (href) and a
// natural question to hand Twin (ask). attention=true means it is asking
// for a person, so the panel can lead with it.
export function crossMdxDigest(sources = {}) {
  const { needsYou, forgeRuns, buildRequests, messages, pages, strategyProposals, productBets, twinRuntime, twinReadiness, twinArtifacts, modelConnection, ratifiedMemory } = sources;
  const signals = [];

  if (twinRuntime || twinReadiness || twinArtifacts) {
    // Readiness proves that an observation was accepted. The connection route
    // additionally proves the current tenant has a usable credential in this
    // process, which is the only state in which Twin can really answer.
    const modelConnected = modelConnection?.connected === true || modelConnection?.connections?.some(
      (connection) => connection?.health === "healthy" && connection?.live_call_allowed === true
    );
    const conversationReady = twinReadiness?.conversation_ready === true;
    const artifactCount = num(twinArtifacts?.artifact_count);
    const toolsBlocked = twinRuntime?.blocked_authority?.tool_execution_allowed === false;
    signals.push({
      key: "twin",
      app: "Twin",
      line: tidy(`${conversationReady ? "conversation is ready" : "conversation is still getting ready"}, ${modelConnected ? "a live model is connected" : "no live model is connected"}, ${artifactCount} ${plural(artifactCount, "artifact", "artifacts")} on record${toolsBlocked ? ", and tools still require governed approval" : ""}`, 150),
      href: "/twin",
      ask: "What are Twin's biggest flagship risks?",
      attention: !conversationReady || !modelConnected
    });
  }

  const needsYouCount = num(needsYou?.total_count);
  if (needsYouCount > 0) {
    signals.push({
      key: "review",
      app: "Needs you",
      line: `${needsYouCount} ${plural(needsYouCount, "thing is", "things are")} waiting on your call`,
      href: "/needs-you",
      ask: "What needs me right now?",
      attention: true
    });
  }

  const proposalCount = num(strategyProposals?.proposal_count);
  if (proposalCount > 0) {
    signals.push({
      key: "strategy",
      app: "Strategy",
      line: `${proposalCount} direction ${plural(proposalCount, "proposal is", "proposals are")} waiting to ratify`,
      href: "/strategy",
      ask: "What strategy decisions are open?",
      attention: true
    });
  }

  const betCount = num(productBets?.bet_count);
  if (betCount > 0) {
    const bets = Array.isArray(productBets?.bets) ? productBets.bets : [];
    const latest = bets[bets.length - 1];
    const latestLine = tidy(latest?.bet, 100);
    signals.push({
      key: "product",
      app: "Product",
      line: latestLine
        ? `${betCount} ${plural(betCount, "bet", "bets")} on the board; latest: "${latestLine}"`
        : `${betCount} ${plural(betCount, "bet", "bets")} on the board`,
      href: "/product-direction",
      ask: "What Product bets are open?",
      attention: false
    });
  }

  const runCount = num(forgeRuns?.run_count);
  const requestCount = num(buildRequests?.request_count);
  if (runCount > 0 || requestCount > 0) {
    const latest = Array.isArray(forgeRuns?.runs) ? forgeRuns.runs[0] : null;
    const phrase = forgeLatestPhrase(latest);
    const ran = runCount > 0 ? `${runCount} ${plural(runCount, "build", "builds")} run` : "no builds yet";
    const queued = requestCount > 0 ? `, ${requestCount} ${plural(requestCount, "request", "requests")} in the queue` : "";
    signals.push({
      key: "forge",
      app: "Forge",
      line: tidy(`${ran}${phrase ? `, ${phrase}` : ""}${queued}`),
      href: "/forge",
      ask: "What's Forge been building?",
      attention: requestCount > 0
    });
  }

  const messageCount = num(messages?.message_count);
  if (messageCount > 0) {
    const list = Array.isArray(messages?.messages) ? messages.messages : [];
    const latest = list[list.length - 1];
    const who = String(latest?.actor_display_name ?? "someone").replace(/^[a-z]+:/, "");
    const body = tidy(latest?.body, 60);
    signals.push({
      key: "message",
      app: "Message",
      line: body ? `Last from ${who}: "${body}"` : `${messageCount} ${plural(messageCount, "message", "messages")} in the channel`,
      href: "/message",
      ask: "Catch me up on Message.",
      attention: false
    });
  }

  const pageCount = num(pages?.document_count);
  if (pageCount > 0) {
    const stale = Array.isArray(pages?.documents)
      ? pages.documents.filter((doc) => doc?.freshness?.stale).length
      : 0;
    const latestPage = pages?.latest_document;
    const pageStartedInMessage = Array.isArray(latestPage?.source_receipt_ids)
      && latestPage.source_receipt_ids.some((id) => String(id).startsWith("message_"));
    const latestPageLine = latestPage?.title
      ? `; latest \"${latestPage.title}\"${pageStartedInMessage ? " started in Message" : ""}`
      : "";
    signals.push({
      key: "pages",
      app: "Pages",
      line: stale > 0
        ? `${pageCount} ${plural(pageCount, "page", "pages")}, ${stale} due for a review${latestPageLine}`
        : `${pageCount} ${plural(pageCount, "page", "pages")} in the library${latestPageLine}`,
      href: "/pages",
      ask: "What's documented in Pages?",
      attention: stale > 0
    });
  }

  const ratifications = Array.isArray(ratifiedMemory?.ratifications) ? ratifiedMemory.ratifications : [];
  if (ratifications.length > 0) {
    const latest = ratifications[ratifications.length - 1];
    signals.push({
      key: "memory",
      app: "Memory",
      line: tidy(`${ratifications.length} shared ${plural(ratifications.length, "memory", "memories")} cleared by another person; latest: \"${latest?.content}\"`, 150),
      href: "/memory",
      ask: "What shared memory has another person cleared?",
      attention: false
    });
  }

  // Attention-first, then the rest in declaration order - so the person
  // sees what is asking for them before what is merely humming along.
  signals.sort((a, b) => Number(b.attention) - Number(a.attention));

  return { signals, worldContext: signalsToContext(signals), needsYouCount };
}

// The grounding string: the same signals, flattened to one compact block
// the model can read when you ask about the company. Capped so it rides
// alongside the persona and companion layers without crowding them.
function signalsToContext(signals) {
  if (!signals.length) return "";
  const lines = signals.map((signal) => `- ${signal.app}: ${signal.line}.`);
  const text = `Across MDx right now:\n${lines.join("\n")}`;
  return text.length > MAX_CONTEXT_CHARS ? `${text.slice(0, MAX_CONTEXT_CHARS - 1)}…` : text;
}
