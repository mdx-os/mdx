<script>
  import { onMount, tick } from "svelte";
  import { replaceState } from "$app/navigation";
  import Provenance from "@mdx/ui/components/Provenance.svelte";
  import Markdown from "@mdx/ui/components/Markdown.svelte";
  import FirstUseHint from "../../lib/FirstUseHint.svelte";
  import MessageSearchPanel from "./MessageSearchPanel.svelte";
  import MessageCatchUpPanel from "./MessageCatchUpPanel.svelte";
  import MarketplacePackPrompt from "../../lib/MarketplacePackPrompt.svelte";
  import { composeMessageWithAttachment } from "./messageCatchUp.js";
  import { foldReactions } from "./messageReactions.js";
  import { createMessageNotification, shouldNotifyMessage } from "./messageNotifications.js";
  import { mergePresenceRoster } from "./messagePresence.js";
  import {
    createMessageClient,
    connectRelay,
    createRealtimeCursor,
    normalizeMessage,
    replyThreadPrefix,
    reactThreadPrefix,
    threadsPath,
    timelinePath,
    sendPath,
    actionVerdictPath,
    bridgePath
  } from "../../lib/messageSession.js";
  import { createOutbox, mintMessageId } from "../../lib/messageOutbox.js";
  import { loadProfile } from "../../lib/youProfile.js";
  import { kernelReachability, retryReachability } from "../../lib/kernelReachability.js";


  let { data } = $props();

  const client = $derived(createMessageClient(data.session));
  const SELF_ACTOR = $derived(`human:${data.session.user_id}`);
  const SEEN_KEY = "mdx-host-message-last-seen";

  let allMessages = $state([]);
  // Long sessions must not accumulate an unbounded timeline in memory: keep
  // the most recent window; older history stays one load-older click away.
  const MESSAGE_WINDOW = 1500;
  function capMessages(list) {
    return list.length > MESSAGE_WINDOW ? list.slice(list.length - MESSAGE_WINDOW) : list;
  }
  let threadId = $state("");
  // Channels: the kernel records whatever channel a message names, the
  // relay topics are per-channel - so channels emerge from the messages
  // themselves. A new channel becomes real when its first message lands.
  const CHANNEL_KEY = "mdx-message-channel";
  let activeChannelId = $state("local-ops");
  let localChannels = $state([]);
  // Governed channels: the first-class channel objects, folded from their
  // created + updated receipts by /messages/channels/projection.json. These
  // carry a real name, description, topic, and lifecycle - a channel can now
  // be renamed, described, and archived, not just summoned by a message. The
  // message-derived channels below stay as a back-compat floor so a channel
  // that only ever carried messages still appears.
  let governedChannels = $state([]);
  // The activity substrate: the receipt ledger rendered as a live feed of
  // what the stack is doing, routed into per-area channels (#forge, #deploys,
  // #strategy, ...). Derived, never written - every card traces to a receipt.
  let activityFeed = $state([]);
  let activityAreas = $state([]);
  // Presence: who is active in the stack, derived from the ledger (recently
  // acting people and agents). Recency is ledger order, not wall-clock; the
  // live typing signal rides on top of this roster.
  let presenceRoster = $state([]);
  let channelSettingsOpen = $state(false);
  let channelDraft = $state({ name: "", description: "", topic: "", visibility: "public" });
  let channelSaveState = $state("idle");
  let memberDraft = $state({ actorId: "", role: "member" });
  const activeMembers = $derived(channelMeta.get(activeChannelId)?.members ?? []);
  let unreadByChannel = $state({});
  let newChannelName = $state("");
  let addingChannel = $state(false);
  let addingDm = $state(false);
  let newDmTarget = $state("");
  let loaded = $state(false);
  let draft = $state("");
  let attachmentOpen = $state(false);
  let attachmentUrl = $state("");
  let attachmentLabel = $state("");
  let attachmentError = $state("");
  // The quiet capture chip after a top-level send: a team message becomes a
  // team note MDx can recall once a teammate clears it. Shown only for the
  // composer send, never for reactions or thread replies.
  let captureChip = $state(false);
  let sendState = $state("idle");
  let liveState = $state("off");
  let typingNames = $state([]);
  let unreadStartSeq = $state(0);
  let pendingNew = $state(0);
  let timelineEl = $state(null);
  let composerEl = $state(null);
    // The offline outbox: every send goes through it, online or not. These
  // mirror its queue for rendering; the truth lives in IndexedDB.
  let outboxItems = $state([]);
  // "Online" here means the KERNEL is answering, not the browser's internet.
  // A queued message delivers when the kernel is back, so the outbox must
  // watch kernel reachability (the shared store) rather than navigator.onLine,
  // which stays true on a working laptop whose local kernel has stopped.
  const online = $derived($kernelReachability.online);
  let lastSyncedAt = $state(null);
  // The timeline window: the DOM renders at most windowSize entries per
  // room (the v1 lesson, kept simple) - "show earlier" grows the window
  // from memory first, then pages older history from the kernel by
  // cursor. Scroll positions are remembered per room.
  const WINDOW_STEP = 120;
  let windowSize = $state(WINDOW_STEP);
  let hasOlderByChannel = $state({});
  let loadingOlder = $state(false);
  let scrollMemory = {};

  let workspaceControls = $state(null);
  const reactionsEnabled = $derived(workspaceControls?.keys?.reactions_enabled !== false);
  const slashEnabled = $derived(workspaceControls?.keys?.slash_commands_enabled !== false);

  let refusalNote = $state("");
  let refusalTimer = null;

  const outbox = createOutbox({
    send: async (item) => {
      const packet = await client.write(
        sendPath,
        {
          body: item.body,
          channel_id: item.channelId,
          message_id: item.messageId,
          ...(item.threadId ? { thread_id: item.threadId } : {})
        },
        { receiptIntent: "message_thread_message" }
      );
      if (packet?.status === "REFUSED") {
        const refusal = new Error(packet.reason ?? "this workspace's rules refused the message");
        refusal.terminal = true;
        throw refusal;
      }
      return packet;
    },
    onRefused: (item, reason) => {
      refusalNote = reason;
      clearTimeout(refusalTimer);
      // Refusals must stay readable: 20s instead of a 6s blink.
    refusalTimer = setTimeout(() => (refusalNote = ""), 20000);
    },
    // Retry safety: the projection echoes message_id, so "did my earlier
    // attempt land?" has a true answer before anything is resent.
    probe: async (messageId) => {
      const projection = await client.read(timelinePath);
      return (projection.messages ?? []).some((message) => message.message_id === messageId);
    },
    onChange: (items) => {
      const hadItems = outboxItems.length > 0;
      outboxItems = items;
      if (hadItems && items.length === 0) {
        loadConversation();
        // A message into an activity channel joins that channel's feed; it
        // also makes you the most recent actor in the presence roster.
        loadActivity();
        loadPresence();
      }
    }
  });

  const messages = $derived(
    allMessages.filter((message) => (message.channelId || "local-ops") === activeChannelId)
  );
  // Threads: replies live under their parent message, never in the main
  // stream. A reply names its parent through the contract's thread_id
  // field, so threads survive reload, ride the relay live, and are
  // receipt-backed like everything else.
  const topLevel = $derived(messages.filter((message) => !message.replyTo && !message.reactTo));
  // Reactions fold append-only +/- events into current state per parent,
  // per emoji - count and whether THIS seat is among them.
  const reactionsByParent = $derived.by(() => {
    return foldReactions(messages, SELF_ACTOR);
  });

  const REACTION_SET = ["👍", "✅", "🎉", "👀"];

  function toggleReaction(message, emoji) {
    const mine = (reactionsByParent.get(message.receiptId) ?? []).find(
      (chip) => chip.emoji === emoji
    )?.mine;
    outbox.enqueue({
      messageId: mintMessageId(),
      channelId: activeChannelId,
      threadId: `${reactThreadPrefix}${message.receiptId}`,
      body: `${mine ? "-" : "+"}:${emoji}`
    });
  }
  const visibleTopLevel = $derived(topLevel.slice(-windowSize));
  // Agent chatter folds: three or more consecutive non-human posts
  // collapse into one calm run row, expandable in place. Human words are
  // never folded - the lane structure keeps agents useful without
  // letting them drown the room. (v1's agent lanes, the governed cut.)
  // Mentions, mute, slash commands, and search - the interaction layer.
  const MUTED_KEY = "mdx-message-muted";
  let myName = $state("");
  let mentionsByChannel = $state({});
  let mutedChannels = $state([]);
  let slashOpen = $state(false);
  let slashIndex = $state(0);
  let searchOpen = $state(false);
  let catchUpOpen = $state(false);
  let notificationsEnabled = $state(false);
  let searchQuery = $state("");
  let searchResults = $state(null);
  let searching = $state(false);
  let flashSeq = $state(0);
  // Focus: a reactive mirror of the per-room seen watermark (the truth
  // lives in localStorage; this copy lets the Focus band recompute the
  // moment you catch up). Seeded on load, advanced whenever storeSeen
  // writes - so an item leaves Focus the instant you read or answer it.
  let seenMap = $state({});
  let focusDismissed = $state(false);

  async function enableNotifications() {
    if (typeof Notification === "undefined") return;
    const permission = Notification.permission === "granted" ? "granted" : await Notification.requestPermission();
    notificationsEnabled = permission === "granted";
    try {
      localStorage.setItem("mdx-message-notifications", notificationsEnabled ? "on" : "off");
    } catch (error) {
      // This preference can remain tab-local when storage is unavailable.
    }
  }

  const SLASH_COMMANDS = [
    { id: "decision", label: "/decision", line: "record a decision for the company", insert: "Decision: \nWhy: " },
    { id: "handoff", label: "/handoff", line: "hand work to the team, context attached", insert: "Handoff: done so far -\nNext: \nReceipts: " },
    { id: "search", label: "/search", line: "search every room", insert: "" },
    { id: "mute", label: "/mute", line: "mute or unmute this channel", insert: "" }
  ];

  function mentionsMe(body) {
    const lower = body.toLowerCase();
    if (lower.includes("@here") || lower.includes("@team")) return true;
    const first = myName.split(/\s+/)[0]?.toLowerCase();
    return Boolean(first) && lower.includes(`@${first}`);
  }

  function channelMuted(id) {
    return mutedChannels.includes(id);
  }

  function toggleMute(id) {
    mutedChannels = channelMuted(id)
      ? mutedChannels.filter((entry) => entry !== id)
      : [...mutedChannels, id];
    try {
      localStorage.setItem(MUTED_KEY, JSON.stringify(mutedChannels));
    } catch (error) {
      // tab-local only
    }
  }

  function runSlash(command) {
    slashOpen = false;
    if (command.id === "search") {
      draft = "";
      searchOpen = true;
      return;
    }
    if (command.id === "mute") {
      draft = "";
      toggleMute(activeChannelId);
      return;
    }
    draft = command.insert;
  }

  async function runSearch() {
    const query = searchQuery.trim();
    if (!query || searching) return;
    searching = true;
    try {
      const packet = await client.read(`${timelinePath}?q=${encodeURIComponent(query)}&limit=50`);
      searchResults = (Array.isArray(packet.messages) ? packet.messages : [])
        .map(normalizeMessage)
        .filter((message) => !message.reactTo)
        .reverse();
    } catch (error) {
      searchResults = [];
    }
    searching = false;
  }

  // Jump to a search hit: open its room, make sure its window of history
  // is loaded, then scroll to the message and flash it.
  async function jumpToResult(result) {
    searchOpen = false;
    if (result.channelId !== activeChannelId) openChannel(result.channelId);
    const target = result.sequence;
    if (!messages.some((message) => message.sequence === target)) {
      try {
        const packet = await client.read(
          `${timelinePath}?channel_id=${encodeURIComponent(result.channelId)}&before_sequence=${target + 1}&limit=60`
        );
        const older = (Array.isArray(packet.messages) ? packet.messages : []).map(normalizeMessage);
        const known = new Set(allMessages.map((message) => message.receiptId));
        allMessages = capMessages([...older.filter((message) => !known.has(message.receiptId)), ...allMessages].sort(
          (a, b) => a.sequence - b.sequence
        ));
      } catch (error) {
        // kernel unreachable; the jump lands where it can
      }
    }
    const newerCount = topLevel.filter((message) => message.sequence >= target).length;
    windowSize = Math.max(windowSize, newerCount + 10);
    flashSeq = target;
    await tick();
    document.querySelector(`[data-seq="${target}"]`)?.scrollIntoView({ block: "center" });
    setTimeout(() => (flashSeq = 0), 1800);
  }

  const RUN_FOLD_THRESHOLD = 3;
  let expandedRuns = $state({});
  const renderTimeline = $derived.by(() => {
    const out = [];
    let run = [];
    const flushRun = () => {
      if (run.length >= RUN_FOLD_THRESHOLD) {
        out.push({ kind: "run", id: run[0].receiptId, entries: run });
      } else {
        for (const entry of run) out.push({ kind: "message", id: entry.receiptId, entry });
      }
      run = [];
    };
    let chain = 0;
    for (const message of visibleTopLevel) {
      if (message.actorType !== "human") {
        run.push(message);
        chain = 0;
      } else {
        flushRun();
        // Slack's old kindness: consecutive messages from the same person
        // group under one name, in short chains.
        const previous = out[out.length - 1];
        const grouped =
          previous?.kind === "message" && previous.entry.actor === message.actor && chain < 5;
        chain = grouped ? chain + 1 : 0;
        out.push({ kind: "message", id: message.receiptId, entry: { ...message, grouped } });
      }
    }
    flushRun();
    return out;
  });

  function runActors(entries) {
    return [...new Set(entries.map((entry) => humanToken(entry.actor)))].slice(0, 3).join(", ");
  }

  function runLane(entries) {
    const lanes = new Set(entries.map((entry) => entry.actorType));
    return lanes.size === 1 ? [...lanes][0] : "agent";
  }

  // Typed Message envelopes (Message vNext). A non-text content_type carries
  // a structured content_payload that renders as a compact card. The body is
  // always the plain-language line; the payload is parsed defensively so a
  // malformed event degrades to its body, never to a blank or a crash.
  function safePayload(raw) {
    if (!raw) return null;
    try {
      const value = JSON.parse(raw);
      return value && typeof value === "object" ? value : null;
    } catch (error) {
      return null;
    }
  }
  const CARD_KIND_LABEL = {
    forge_build_request: "Forge build request"
  };
  // State reads as a human status, never as an execution claim. "execution_blocked"
  // is shown as "Stopped before execution" - honest about what did not happen.
  const CARD_STATE = {
    execution_blocked: { label: "Stopped before execution", tone: "hold" },
    ready_for_review: { label: "Ready for review", tone: "review" },
    recorded: { label: "On the record", tone: "done" }
  };
  function cardView(message) {
    const data = safePayload(message.contentPayload) ?? {};
    const kind = String(data.kind ?? "");
    const state = String(data.state ?? "");
    return {
      label: CARD_KIND_LABEL[kind] ?? (kind ? humanToken(kind) : "Update"),
      summary: String(data.requested_change ?? data.summary ?? data.detail ?? ""),
      state: CARD_STATE[state] ?? (state ? { label: humanToken(state), tone: "hold" } : null),
      recordId: message.sourceReceiptId || String(data.build_request_receipt_id ?? data.source_receipt_id ?? "")
    };
  }
  // The four verdicts of a governed action. Approve and decline are one
  // gesture; edit and respond carry the human's own words, so they open an
  // input before they send. Execution always stays with the backend gate -
  // nothing here runs a worker, a provider, or a write.
  const VERDICT_NEEDS_INPUT = new Set(["edit", "respond"]);
  let actionBusy = $state("");
  let actionInput = $state(null); // { requestId, verdict } while the human types
  let actionInputText = $state("");
  let actionErrors = $state({}); // requestId -> human-readable refusal
  function actionView(message) {
    const data = safePayload(message.contentPayload) ?? {};
    // A response names the request it answers in its payload; a request is
    // its own source receipt. Both resolve to the same request id, so a
    // request and its answer can always be paired.
    const requestReceiptId =
      message.contentType === "action_response"
        ? String(data.action_request_receipt_id ?? "")
        : message.sourceReceiptId || String(data.action_request_receipt_id ?? "");
    return {
      title: String(data.title ?? data.action ?? "Action proposed"),
      summary: String(data.summary ?? data.detail ?? data.requested_change ?? ""),
      requestReceiptId,
      verdict: String(data.verdict ?? ""),
      appliedReceiptKind: String(data.applied_receipt_kind ?? ""),
      appliedReceiptId: String(data.applied_receipt_id ?? ""),
      gateState: String(data.gate_state ?? ""),
      executionAllowed: data.execution_allowed === true
    };
  }
  // A request is decided once its answer lands in the thread. The answer
  // names the request it resolves, so the link is exact, never guessed.
  const actionDecisions = $derived.by(() => {
    const map = new Map();
    for (const message of allMessages) {
      if (message.contentType !== "action_response") continue;
      const data = safePayload(message.contentPayload) ?? {};
      const requestId = String(data.action_request_receipt_id ?? "");
      if (requestId) map.set(requestId, actionView(message));
    }
    return map;
  });
  // The plain-language outcome of a recorded verdict. The receipt kind and
  // gate state are evidence kept in disclosure; this line is for a human,
  // and it never claims anything ran.
  const VERDICT_OUTCOME = {
    approve: "Approved. Recorded and ready for the next gate. Nothing has run.",
    edit: "Approved with your edits. Recorded. Nothing has run.",
    reject: "Declined. Recorded with your reason. Nothing ran.",
    respond: "You answered. Recorded on the thread. Nothing ran."
  };
  function verdictOutcome(verdict) {
    return VERDICT_OUTCOME[verdict] ?? "Recorded. Nothing has run.";
  }
  function beginVerdict(message, verdict) {
    if (actionBusy) return;
    if (VERDICT_NEEDS_INPUT.has(verdict)) {
      actionInput = { requestId: actionView(message).requestReceiptId, verdict };
      actionInputText = "";
      return;
    }
    recordActionVerdict(message, verdict, "");
  }
  function cancelVerdictInput() {
    actionInput = null;
    actionInputText = "";
  }
  async function recordActionVerdict(message, verdict, note) {
    const action = actionView(message);
    if (!action.requestReceiptId || actionBusy) return;
    if (VERDICT_NEEDS_INPUT.has(verdict) && !note.trim()) return;
    actionBusy = `${action.requestReceiptId}:${verdict}`;
    actionErrors = { ...actionErrors, [action.requestReceiptId]: "" };
    // respond puts the human's answer on the record; edit carries the
    // changed scope; approve and decline record a clean human decision.
    const decisionNote =
      verdict === "respond" ? note.trim()
      : verdict === "edit" ? "Approved with edited scope."
      : verdict === "reject" ? (note.trim() || "Declined from Message.")
      : "Approved from Message.";
    try {
      const packet = await client.write(
        actionVerdictPath,
        {
          action_request_receipt_id: action.requestReceiptId,
          action_verdict_id: `${message.messageId || message.receiptId}_${verdict}`,
          verdict,
          decision_note: decisionNote,
          edited_action_payload: verdict === "edit" ? note.trim() : ""
        },
        { receiptIntent: "message_action_verdict" }
      );
      if (packet?.status === "REFUSED") {
        actionErrors = { ...actionErrors, [action.requestReceiptId]: packet.reason ?? "That decision was refused." };
      } else {
        actionInput = null;
        actionInputText = "";
        await loadConversation();
        await loadActivity();
        await loadPresence();
      }
    } catch (error) {
      actionErrors = { ...actionErrors, [action.requestReceiptId]: "The decision did not land. Try again." };
    }
    actionBusy = "";
  }
  // Evidence stays one quiet disclosure away: the message receipt, its place
  // in the order, the underlying record an agent post points at, and - for a
  // non-human post - the receipt kind that proves it was never recorded as a
  // person.
  function messageProvenance(message) {
    const items = [
      { label: "This message", value: message.receiptId },
      { label: "Position", value: message.sequence ? `#${message.sequence}` : "" }
    ];
    if (message.sourceReceiptId) items.push({ label: "The record", value: message.sourceReceiptId });
    if (message.actorType !== "human" && message.messageReceiptKind) {
      items.push({ label: "Posted as", value: message.messageReceiptKind });
    }
    return items;
  }

  // Trusted time. Receipts now carry a real signed timestamp, so the timeline
  // can show when things happened instead of a placeholder. "Now" for this
  // surface is the newest event the client holds - the sync frontier - so an
  // unsynced clock never makes a message look stale, and the relative read is
  // honest in both the deterministic local kernel and a real wall clock.
  function parseTs(iso) {
    if (!iso) return NaN;
    const ms = Date.parse(iso);
    return Number.isNaN(ms) ? NaN : ms;
  }
  const latestActivityMs = $derived.by(() => {
    let max = NaN;
    for (const message of allMessages) {
      const ms = parseTs(message.createdAt);
      if (!Number.isNaN(ms) && (Number.isNaN(max) || ms > max)) max = ms;
    }
    return max;
  });
  function relTime(iso, nowMs) {
    const then = parseTs(iso);
    if (Number.isNaN(then)) return "";
    const ref = Number.isNaN(nowMs) ? then : nowMs;
    const seconds = Math.max(0, Math.round((ref - then) / 1000));
    if (seconds < 10) return "just now";
    if (seconds < 60) return `${seconds}s`;
    const minutes = Math.round(seconds / 60);
    if (minutes < 60) return `${minutes}m`;
    const hours = Math.round(minutes / 60);
    if (hours < 24) return `${hours}h`;
    return `${Math.round(hours / 24)}d`;
  }
  function fullTime(iso) {
    const ms = parseTs(iso);
    if (Number.isNaN(ms)) return "";
    return `${new Date(ms).toISOString().replace("T", " ").replace(/\.\d+Z$/, "")} UTC`;
  }

  // The S30 skill output shape: a recorded decision reads like one.
  function decisionParts(body) {
    if (!body.startsWith("Decision: ")) return null;
    const lines = body.split("\n");
    return {
      title: lines[0].slice("Decision: ".length),
      detail: lines.slice(1).filter((line) => !line.startsWith("Why: ")).join("\n"),
      why: (lines.find((line) => line.startsWith("Why: ")) ?? "").slice("Why: ".length)
    };
  }
  // Focus: the one thing this surface leads with - what your conversations
  // need from you, across every channel. Two reasons, the way Front and
  // Superhuman split a triage inbox: someone named you, or an agent asked
  // and is still waiting. Everything here is derived from messages already
  // on the record - no new authority, no server-side inbox to trust.
  const FOCUS_LIMIT = 6;
  function isAgentAsk(message) {
    if (message.actorType === "human") return false;
    // A real action_request IS an ask - a governed decision is waiting. This
    // is the only structured ask today; everything else still leans on the
    // body heuristic until the action lifecycle lands.
    if (message.contentType === "action_request") return true;
    const body = (message.body ?? "").trim();
    if (!body) return false;
    if (body.endsWith("?")) return true;
    return /\b(need your|needs your|need you to|your call|please (?:approve|confirm|review|decide)|waiting on you|sign off|should we|should i|which (?:one|option)|approve this|your decision|awaiting your)\b/i.test(
      body
    );
  }
  function seenFor(channelId) {
    return seenMap[`${threadId || "thread"}:${channelId}`] ?? 0;
  }
  function focusPreview(body) {
    const line = (body ?? "").split("\n").find((entry) => entry.trim()) ?? "";
    const text = line.replace(/^Decision:\s*/, "").trim();
    return text.length > 96 ? `${text.slice(0, 95)}…` : text;
  }
  // Everything across every channel that is waiting on you: decisions to make,
  // mentions to answer, agent asks still open. The Focus band shows the top of
  // this; the Agent Inbox shows all of it and lets you decide in place. One
  // source, derived from messages on the record - no server-side inbox to trust.
  const pendingForYou = $derived.by(() => {
    if (allMessages.length === 0) return [];
    // An agent question is "open" only until a person speaks after it in
    // that channel - the last human word per channel is the answer line.
    const lastHumanSeq = new Map();
    for (const message of allMessages) {
      if (message.actorType === "human" && !message.replyTo && !message.reactTo) {
        const id = message.channelId || "local-ops";
        if (message.sequence > (lastHumanSeq.get(id) ?? 0)) lastHumanSeq.set(id, message.sequence);
      }
    }
    const items = [];
    for (const message of allMessages) {
      if (message.replyTo || message.reactTo) continue;
      const channelId = message.channelId || "local-ops";
      // A decided action never needs you again - its answer is on the record.
      if (message.contentType === "action_request" && actionDecisions.has(actionView(message).requestReceiptId)) {
        continue;
      }
      let reason = "";
      if (message.contentType === "action_request" && message.sequence > (lastHumanSeq.get(channelId) ?? 0)) {
        reason = "decision";
      } else if (mentionsMe(message.body) && message.actorType !== "human" && message.sequence > seenFor(channelId)) {
        reason = "mention";
      } else if (isAgentAsk(message) && message.sequence > (lastHumanSeq.get(channelId) ?? 0)) {
        reason = "ask";
      }
      if (!reason) continue;
      const action = message.contentType === "action_request" ? actionView(message) : null;
      items.push({
        message,
        receiptId: message.receiptId,
        channelId,
        actor: humanToken(message.actor),
        actorType: message.actorType,
        sequence: message.sequence,
        reason,
        title: action ? action.title : "",
        preview: focusPreview(message.body),
        createdAt: message.createdAt
      });
    }
    // A decision waiting on you outranks a mention, which outranks an open
    // ask; within a tier, the most recent leads.
    const weight = (reason) => (reason === "decision" ? 2 : reason === "mention" ? 1 : 0);
    return items.sort((a, b) => weight(b.reason) - weight(a.reason) || b.sequence - a.sequence);
  });
  const focusItems = $derived(pendingForYou.slice(0, FOCUS_LIMIT));
  const focusVisible = $derived(loaded && !focusDismissed && focusItems.length > 0);
  // The Agent Inbox: the full cross-channel queue, decisions first.
  let inboxOpen = $state(false);
  const inboxDecisionCount = $derived(pendingForYou.filter((item) => item.reason === "decision").length);
  const showFirstUseHint = $derived(
    loaded &&
      allMessages.length === 0 &&
      governedChannels.length === 0 &&
      localChannels.length === 0 &&
      pendingForYou.length === 0
  );

  function focusJump(item) {
    jumpToResult({ channelId: item.channelId, sequence: item.sequence });
  }

  const earlierAvailable = $derived(
    topLevel.length > windowSize || Boolean(hasOlderByChannel[activeChannelId])
  );
  // Pending sends for this room, minus anything the kernel already echoed
  // back (the relay can deliver our own message before the queue retires
  // it - the timeline must never show both).
  const savedMessageIds = $derived(new Set(messages.map((message) => message.messageId).filter(Boolean)));
  const pendingTopLevel = $derived(
    outboxItems.filter(
      (item) => item.channelId === activeChannelId && !item.threadId && !savedMessageIds.has(item.messageId)
    )
  );
  const pendingByParent = $derived.by(() => {
    const map = new Map();
    for (const item of outboxItems) {
      if (item.channelId !== activeChannelId || !item.threadId || savedMessageIds.has(item.messageId)) continue;
      const parent = item.threadId.startsWith(replyThreadPrefix)
        ? item.threadId.slice(replyThreadPrefix.length)
        : item.threadId;
      const list = map.get(parent) ?? [];
      list.push(item);
      map.set(parent, list);
    }
    return map;
  });
  const pendingCount = $derived(outboxItems.length);
  const failedCount = $derived(outboxItems.filter((item) => item.status === "failed").length);
  const repliesByParent = $derived.by(() => {
    const map = new Map();
    for (const message of messages) {
      if (!message.replyTo || message.reactTo) continue;
      const list = map.get(message.replyTo) ?? [];
      list.push(message);
      map.set(message.replyTo, list);
    }
    return map;
  });
  let openThreadId = $state("");
  let replyDraft = $state("");
  let replyState = $state("idle");
  // First-visit landing: once the channel list arrives, prefer #general
  // (the welcome) over the agent-ops lane. Runs once, never fights a
  // person's own pick (any explicit pick writes CHANNEL_KEY).
  let firstVisit = $state(false);
  $effect(() => {
    if (firstVisit && channels.some((entry) => entry.id === "general")) {
      activeChannelId = "general";
      firstVisit = false;
    }
  });
  const channelMeta = $derived.by(() => {
    const map = new Map();
    for (const channel of governedChannels) map.set(channel.channel_id, channel);
    return map;
  });
  const archivedIds = $derived(
    new Set(governedChannels.filter((channel) => channel.status === "archived").map((channel) => channel.channel_id))
  );
  const channels = $derived.by(() => {
    const known = new Map();
    // Governed channels lead - they carry the real name and description.
    for (const channel of governedChannels) {
      if (channel.status === "archived") continue;
      known.set(channel.channel_id, {
        id: channel.channel_id,
        name: channel.name,
        description: channel.description,
        topic: channel.topic,
        kind: channel.channel_kind,
        governed: true,
        count: 0,
        last: 0
      });
    }
    for (const message of allMessages) {
      const id = message.channelId || "local-ops";
      if (archivedIds.has(id)) continue;
      const entry = known.get(id) ?? { id, count: 0, last: 0 };
      entry.count += 1;
      if (message.sequence > entry.last) entry.last = message.sequence;
      known.set(id, entry);
    }
    for (const id of localChannels) {
      if (!known.has(id) && !archivedIds.has(id)) known.set(id, { id, count: 0, last: 0 });
    }
    // The per-area activity channels surface even with no human messages -
    // a feed is a place, not just a conversation.
    for (const area of activityAreas) {
      const entry = known.get(area.channel_id) ?? { id: area.channel_id, count: 0, last: area.item_count };
      entry.activity = true;
      if (!entry.name) entry.name = area.label;
      known.set(area.channel_id, entry);
    }
    if (!known.has("local-ops")) known.set("local-ops", { id: "local-ops", count: 0, last: 0 });
    return [...known.values()].sort((a, b) => b.last - a.last);
  });
  function channelLabel(id) {
    return channelMeta.get(id)?.name || humanToken(id);
  }
  const activityChannelIds = $derived(new Set(activityAreas.map((area) => area.channel_id)));
  // Once an agent posts a real Message event into an area channel (Forge is
  // the first), that channel graduates from the derived activity projection
  // to the real thread timeline. The activity feed stays as compatibility
  // scaffolding for areas that have no real events yet; it is no longer the
  // product truth where real events exist.
  const channelsWithRealMessages = $derived(
    new Set(allMessages.map((message) => message.channelId || "local-ops"))
  );
  const activeIsActivity = $derived(
    activityChannelIds.has(activeChannelId) && !channelsWithRealMessages.has(activeChannelId)
  );
  // A busy area (e.g. #forge after thousands of runs) would render every
  // card at once - too much DOM and not calm. The feed shows the most
  // recent window, newest at the bottom, the way a chat room does.
  const FEED_WINDOW = 150;
  const activeChannelFeed = $derived(
    activityFeed
      .filter((item) => item.channel_id === activeChannelId)
      .slice()
      .sort((a, b) => a.ledger_seq - b.ledger_seq)
  );
  // Collapse runs of identical machine cards (live feed carried 3,950 copies
  // of the same "moved a build forward" headline): consecutive items with the
  // same headline+actor fold into one row with a count. Every underlying
  // receipt stays on the ledger; this is a reading affordance, not a filter.
  const collapsedChannelFeed = $derived.by(() => {
    const out = [];
    for (const item of activeChannelFeed) {
      const prev = out[out.length - 1];
      const key = `${item.headline ?? ""}|${item.actor ?? ""}|${item.receipt_kind ?? ""}`;
      if (prev && prev.collapseKey === key) {
        prev.repeatCount += 1;
        prev.lastLedgerSeq = item.ledger_seq;
      } else {
        out.push({ ...item, collapseKey: key, repeatCount: 1, lastLedgerSeq: item.ledger_seq });
      }
    }
    return out;
  });
  const activeFeed = $derived(collapsedChannelFeed.slice(-FEED_WINDOW));
  const feedTruncated = $derived(collapsedChannelFeed.length - activeFeed.length);
  // Presence: the active faces (a short stack) and a human one-line summary.
  const activeRoster = $derived(presenceRoster.filter((person) => person.active));
  const aroundCount = $derived(presenceRoster.length - activeRoster.length);
  const presenceSummary = $derived.by(() => {
    if (activeRoster.length === 0) return "";
    const names = activeRoster.slice(0, 2).map((person) => person.display_name);
    const lead = names.join(", ") + (activeRoster.length > 2 ? ` +${activeRoster.length - 2}` : "");
    const around = aroundCount > 0 ? ` · ${aroundCount} more around` : "";
    return `${lead} active now${around}`;
  });
  const activeChannel = $derived(channels.find((channel) => channel.id === activeChannelId) ?? null);
  // The room title follows the channel's display name, so a rename shows
  // everywhere at once - header, title, composer placeholder.
  const threadTitle = $derived(channelLabel(activeChannelId));
  const empty = $derived(messages.length === 0);

  let relay = null;
  let descriptor = null;
  const cursor = createRealtimeCursor();
  const typingTimers = {};
  let lastTypingSentAt = 0;
  let typingStopTimer = null;

  function humanToken(value) {
    return String(value ?? "")
      .replace(/[_-]+/g, " ")
      .replace(/\b\w/g, (ch) => ch.toUpperCase());
  }

  async function scrollToEnd() {
    await tick();
    if (timelineEl) {
      timelineEl.scrollTop = timelineEl.scrollHeight;
    }
  }

  function readSeen() {
    try {
      return JSON.parse(localStorage.getItem(SEEN_KEY) ?? "{}");
    } catch (error) {
      return {};
    }
  }

  function seenKey() {
    return `${threadId || "thread"}:${activeChannelId}`;
  }

  function storeSeen(sequence) {
    if (!threadId || !Number.isFinite(sequence)) return;
    if ((seenMap[seenKey()] ?? 0) < sequence) {
      seenMap = { ...seenMap, [seenKey()]: sequence };
    }
    try {
      const seen = readSeen();
      if ((seen[seenKey()] ?? 0) >= sequence) return;
      seen[seenKey()] = sequence;
      localStorage.setItem(SEEN_KEY, JSON.stringify(seen));
    } catch (error) {
      // Storage unavailable: markers stay off, the thread is untouched.
    }
  }

  function latestSeq() {
    return messages[messages.length - 1]?.sequence ?? 0;
  }

  function allLatestSeq() {
    return allMessages[allMessages.length - 1]?.sequence ?? 0;
  }

  function rememberChannel(id) {
    activeChannelId = id;
    if (typeof location !== "undefined") {
      const url = new URL(location.href);
      url.searchParams.set("channel", id);
      url.searchParams.delete("thread");
      replaceState(`${url.pathname}${url.search}`, {});
    }
    try {
      localStorage.setItem(CHANNEL_KEY, id);
    } catch (error) {
      // tab-local only
    }
  }

  function openChannel(id) {
    // Scroll memory: the room you left keeps its place; the room you
    // enter restores its own, or starts at the latest words.
    if (timelineEl) scrollMemory[activeChannelId] = timelineEl.scrollTop;
    rememberChannel(id);
    catchUpOpen = false;
    windowSize = WINDOW_STEP;
    unreadByChannel = { ...unreadByChannel, [id]: 0 };
    mentionsByChannel = { ...mentionsByChannel, [id]: 0 };
    pendingNew = 0;
    typingNames = [];
    const seen = readSeen()[seenKey()];
    const first = seen != null ? messages.find((message) => message.sequence > seen) : null;
    unreadStartSeq = first?.sequence ?? 0;
    storeSeen(latestSeq());
    tick().then(() => {
      if (timelineEl && scrollMemory[id] != null) {
        timelineEl.scrollTop = scrollMemory[id];
      } else {
        scrollToEnd();
      }
    });
  }

  function slugifyChannel(name) {
    return name.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 32);
  }

  function resubscribe() {
    if (!relay || !descriptor) return;
    const channelIds = [...new Set([...channels.map((entry) => entry.id), activeChannelId])];
    descriptor = {
      ...descriptor,
      subscribe_payload: { ...descriptor.subscribe_payload, channel_ids: channelIds }
    };
    relay.send({ ...descriptor.subscribe_payload, after_sequence: cursor.value() });
  }

  async function loadChannels() {
    try {
      const packet = await client.read("/messages/channels/projection.json");
      governedChannels = Array.isArray(packet.channels) ? packet.channels : [];
    } catch (error) {
      // The kernel is not answering; the message-derived floor still stands.
    }
  }

  async function loadActivity() {
    try {
      const packet = await client.read("/messages/activity/projection.json?limit=300");
      activityFeed = Array.isArray(packet.items) ? packet.items : [];
      activityAreas = Array.isArray(packet.areas) ? packet.areas : [];
    } catch (error) {
      // No kernel; the activity channels simply show empty.
    }
  }

  async function loadPresence() {
    try {
      const packet = await client.read("/messages/presence/projection.json");
      presenceRoster = mergePresenceRoster(packet.roster, data.session.user_id);
    } catch (error) {
      // No kernel; the presence strip simply stays hidden.
    }
  }

  async function createChannel() {
    const id = slugifyChannel(newChannelName);
    const name = newChannelName.trim();
    if (!id || !name) return;
    // The channel becomes a governed object - named, owned, on the record.
    // It still shows immediately (the local floor) so the room never stalls
    // waiting on the round trip.
    if (!localChannels.includes(id)) {
      localChannels = [...localChannels, id];
    }
    newChannelName = "";
    addingChannel = false;
    openChannel(id);
    resubscribe();
    try {
      const packet = await client.write(
        "/messages/channels.json",
        { channel_id: id, name },
        { receiptIntent: "create_message_channel" }
      );
      if (packet?.status === "REFUSED") {
        showRefusal(packet.reason ?? "this workspace's rules refused the channel");
      }
    } catch (error) {
      // Offline or refused: the local floor keeps the room usable.
    }
    loadChannels();
  }

  // A direct message is a private channel with exactly the two of you - no new
  // primitive, just kind=dm + private + the other party as a member. You are
  // its owner, so only the two of you can read it.
  async function createDm() {
    const target = normalizeActorId(newDmTarget);
    if (!target) return;
    const bare = target.replace(/^[a-z]+:/, "");
    const slug = `dm-${bare.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 40)}`;
    if (slug.length < 4) return;
    newDmTarget = "";
    addingDm = false;
    try {
      await client.write(
        "/messages/channels.json",
        { channel_id: slug, name: `DM · ${humanToken(bare)}`, channel_kind: "dm", visibility: "private" },
        { receiptIntent: "create_message_channel" }
      );
      await client.write(
        "/messages/channel-members.json",
        { channel_id: slug, member_actor_id: target, member_role: "member" },
        { receiptIntent: "add_message_channel_member" }
      );
    } catch (error) {
      // Offline: nothing landed.
    }
    await loadChannels();
    openChannel(slug);
    resubscribe();
  }

  function openChannelSettings() {
    const meta = channelMeta.get(activeChannelId);
    channelDraft = {
      name: meta?.name ?? channelLabel(activeChannelId),
      description: meta?.description ?? "",
      topic: meta?.topic ?? "",
      visibility: meta?.visibility ?? "public"
    };
    channelSaveState = "idle";
    channelSettingsOpen = true;
  }

  // Editing a channel that was only ever a message string promotes it to a
  // governed object on first save (create), then edits it (update) - so the
  // settings panel works for both born-governed and legacy channels.
  async function saveChannelSettings() {
    const name = channelDraft.name.trim();
    if (!name) return;
    channelSaveState = "saving";
    const exists = channelMeta.has(activeChannelId);
    const route = exists ? "/messages/channel-updates.json" : "/messages/channels.json";
    const intent = exists ? "update_message_channel" : "create_message_channel";
    try {
      const packet = await client.write(
        route,
        {
          channel_id: activeChannelId,
          name,
          description: channelDraft.description.trim(),
          topic: channelDraft.topic.trim(),
          visibility: channelDraft.visibility
        },
        { receiptIntent: intent }
      );
      if (packet?.status === "REFUSED") {
        showRefusal(packet.reason ?? "this workspace's rules refused the change");
        channelSaveState = "idle";
        return;
      }
      await loadChannels();
      channelSettingsOpen = false;
      channelSaveState = "idle";
    } catch (error) {
      channelSaveState = "idle";
    }
  }

  async function archiveChannel() {
    if (!channelMeta.has(activeChannelId)) {
      // A legacy channel must be governed before it can be archived: name it
      // first (create), then the archive lands on a real object.
      await client
        .write(
          "/messages/channels.json",
          { channel_id: activeChannelId, name: channelLabel(activeChannelId) },
          { receiptIntent: "create_message_channel" }
        )
        .catch(() => {});
    }
    try {
      const packet = await client.write(
        "/messages/channel-updates.json",
        { channel_id: activeChannelId, status: "archived" },
        { receiptIntent: "update_message_channel" }
      );
      if (packet?.status === "REFUSED") {
        showRefusal(packet.reason ?? "this workspace's rules refused the change");
        return;
      }
      await loadChannels();
      channelSettingsOpen = false;
      const next = channels[0]?.id ?? "local-ops";
      openChannel(next);
    } catch (error) {
      // Offline: the archive will not have landed; nothing changes.
    }
  }

  function showRefusal(reason) {
    refusalNote = reason;
    clearTimeout(refusalTimer);
    // Refusals must stay readable: 20s instead of a 6s blink.
    refusalTimer = setTimeout(() => (refusalNote = ""), 20000);
  }

  // Members are people or agents. An id like "human:priya" or "agent:scout"
  // names them; a bare name is treated as a person. A legacy channel is
  // promoted to a governed object before its first member lands.
  function normalizeActorId(value) {
    const trimmed = value.trim();
    if (!trimmed) return "";
    if (trimmed.includes(":")) return trimmed;
    return `human:${trimmed.toLowerCase().replace(/\s+/g, "_")}`;
  }

  async function ensureGoverned() {
    if (channelMeta.has(activeChannelId)) return;
    await client
      .write(
        "/messages/channels.json",
        { channel_id: activeChannelId, name: channelLabel(activeChannelId) },
        { receiptIntent: "create_message_channel" }
      )
      .catch(() => {});
  }

  async function addMember() {
    const memberActorId = normalizeActorId(memberDraft.actorId);
    if (!memberActorId) return;
    await ensureGoverned();
    try {
      const packet = await client.write(
        "/messages/channel-members.json",
        { channel_id: activeChannelId, member_actor_id: memberActorId, member_role: memberDraft.role },
        { receiptIntent: "add_message_channel_member" }
      );
      if (packet?.status === "REFUSED") {
        showRefusal(packet.reason ?? "this workspace's rules refused the member");
        return;
      }
      memberDraft = { actorId: "", role: "member" };
      await loadChannels();
    } catch (error) {
      // Offline: nothing landed.
    }
  }

  async function removeMember(actorId) {
    try {
      const packet = await client.write(
        "/messages/channel-member-removals.json",
        { channel_id: activeChannelId, member_actor_id: actorId },
        { receiptIntent: "remove_message_channel_member" }
      );
      if (packet?.status === "REFUSED") {
        showRefusal(packet.reason ?? "this workspace's rules refused the change");
        return;
      }
      await loadChannels();
    } catch (error) {
      // Offline: nothing landed.
    }
  }

  async function loadConversation() {
    try {
      const packet = await client.read(threadsPath);
      threadId = packet.thread_id ?? packet.threads?.[0]?.thread_id ?? "thread";
      // The room is named for its channel - a name people chose - never the
      // thread's storage id. threadTitle is derived from the channel's
      // display name, so it follows renames without an imperative set here.
      client
        .read("/messages/controls.json")
        .then((packet) => (workspaceControls = packet))
        .catch(() => (workspaceControls = null));
      loadChannels();
      loadActivity();
      loadPresence();
      const projection = await client.read(`${timelinePath}?limit=300`);
      allMessages = (Array.isArray(projection.messages) ? projection.messages : [])
        .map(normalizeMessage)
        .sort((a, b) => a.sequence - b.sequence);
      if (projection.window?.has_older) {
        hasOlderByChannel = { ...hasOlderByChannel, [activeChannelId]: true };
      }
      cursor.advance(allLatestSeq());
      seenMap = readSeen();
      const seen = readSeen()[seenKey()];
      if (seen != null) {
        const first = messages.find((message) => message.sequence > seen);
        unreadStartSeq = first?.sequence ?? 0;
      }
      storeSeen(latestSeq());
      lastSyncedAt = new Date();
    } catch (error) {
      // The local kernel is not answering; the composer reports honestly.
    }
    loaded = true;
    scrollToEnd();
  }

  function onEnvelope(envelope) {
    const message = normalizeMessage(envelope);
    if (
      allMessages.some(
        (existing) =>
          (existing.receiptId && existing.receiptId === message.receiptId) ||
          (existing.messageId && message.messageId && existing.messageId === message.messageId)
      )
    ) {
      return;
    }
    allMessages = capMessages([...allMessages, message]);
    const channelId = message.channelId || "local-ops";
    if (shouldNotifyMessage({
      enabled: notificationsEnabled,
      visibilityState: document.visibilityState,
      actor: message.actor,
      selfActor: myName,
      muted: channelMuted(channelId),
      liveState
    })) {
      const notification = createMessageNotification(
        globalThis.Notification,
        `${message.actor} in #${channelLabel(channelId)}`,
        {
          body: String(message.body ?? "").slice(0, 180),
          tag: message.receiptId || message.messageId
        }
      );
      if (notification) notification.onclick = () => {
        window.focus();
        openChannel(channelId);
        notification.close();
      };
    }
    if (channelId !== activeChannelId) {
      if (!channelMuted(channelId)) {
        unreadByChannel = { ...unreadByChannel, [channelId]: (unreadByChannel[channelId] ?? 0) + 1 };
        if (mentionsMe(message.body)) {
          mentionsByChannel = { ...mentionsByChannel, [channelId]: (mentionsByChannel[channelId] ?? 0) + 1 };
        }
      }
      return;
    }
    if (message.replyTo) {
      // A live reply lands inside its parent's thread, not the main
      // stream - the reply count on the parent is the visible signal.
      storeSeen(message.sequence);
      return;
    }
    const nearBottom =
      timelineEl && timelineEl.scrollHeight - timelineEl.scrollTop - timelineEl.clientHeight < 48;
    if (nearBottom) {
      scrollToEnd();
      storeSeen(message.sequence);
    } else {
      pendingNew += 1;
    }
  }

  function onEvent(payload) {
    if (payload.type !== "typing" || String(payload.actor_id ?? "") === SELF_ACTOR) {
      return;
    }
    if (String(payload.channel_id ?? "local-ops") !== activeChannelId) {
      return;
    }
    const name = humanToken(String(payload.actor_id ?? "someone").replace(/^[a-z]+:/, ""));
    clearTimeout(typingTimers[name]);
    if (payload.is_typing) {
      typingTimers[name] = setTimeout(() => {
        typingNames = typingNames.filter((entry) => entry !== name);
      }, 10000);
      if (!typingNames.includes(name)) {
        typingNames = [...typingNames, name];
      }
    } else {
      typingNames = typingNames.filter((entry) => entry !== name);
    }
  }

  function sendTyping(kind) {
    if (!descriptor?.typing_signal_allowed) return;
    relay?.send({
      type: kind,
      tenant_id: descriptor.subscribe_payload?.tenant_id,
      channel_id: activeChannelId
    });
  }

  function onComposerInput() {
    const now = Date.now();
    if (now - lastTypingSentAt > 3000) {
      lastTypingSentAt = now;
      sendTyping("typing_start");
    }
    clearTimeout(typingStopTimer);
    typingStopTimer = setTimeout(() => {
      lastTypingSentAt = 0;
      sendTyping("typing_stop");
    }, 4000);
  }

  function send() {
    const text = draft.trim();
    const url = attachmentUrl.trim();
    if (!text && !url) return;
    const composed = composeMessageWithAttachment(text, url, attachmentLabel);
    attachmentError = composed.error;
    if (attachmentError) return;
    const body = composed.body;
    clearTimeout(typingStopTimer);
    sendTyping("typing_stop");
    // One path, online or offline: the outbox owns delivery. The words
    // appear immediately with their honest state; the kernel's own echo
    // replaces them the moment the write lands.
    outbox.enqueue({
      messageId: mintMessageId(),
      channelId: activeChannelId,
      threadId: "",
      body
    });
    draft = "";
    attachmentUrl = "";
    attachmentLabel = "";
    attachmentError = "";
    attachmentOpen = false;
    captureChip = true;
    scrollToEnd();
  }

  function onComposerKeydown(event) {
    if (slashOpen) {
      if (event.key === "ArrowDown") {
        event.preventDefault();
        slashIndex = (slashIndex + 1) % SLASH_COMMANDS.length;
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        slashIndex = (slashIndex - 1 + SLASH_COMMANDS.length) % SLASH_COMMANDS.length;
        return;
      }
      if (event.key === "Enter") {
        event.preventDefault();
        runSlash(SLASH_COMMANDS[slashIndex]);
        return;
      }
      if (event.key === "Escape") {
        slashOpen = false;
        return;
      }
    }
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      send();
    }
  }

  function onComposerSlashInput() {
    slashOpen = slashEnabled && draft === "/";
    if (slashOpen) slashIndex = 0;
  }

  function toggleThread(receiptId) {
    openThreadId = openThreadId === receiptId ? "" : receiptId;
    replyDraft = "";
    replyState = "idle";
    if (typeof location !== "undefined") {
      const url = new URL(location.href);
      if (openThreadId) url.searchParams.set("thread", openThreadId);
      else url.searchParams.delete("thread");
      replaceState(`${url.pathname}${url.search}`, {});
    }
  }

  function sendReply(parentReceiptId) {
    const text = replyDraft.trim();
    if (!text) return;
    outbox.enqueue({
      messageId: mintMessageId(),
      channelId: activeChannelId,
      threadId: `${replyThreadPrefix}${parentReceiptId}`,
      body: text
    });
    replyDraft = "";
    replyState = "idle";
  }

  function onReplyKeydown(event, parentReceiptId) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      sendReply(parentReceiptId);
    }
    if (event.key === "Escape") {
      toggleThread(parentReceiptId);
    }
  }

  // Show earlier: grow the window from already-loaded history first;
  // when that runs out, page older messages from the kernel by cursor.
  // The scroll anchor holds - the message you were reading stays put.
  async function showEarlier() {
    if (loadingOlder) return;
    const anchorHeight = timelineEl?.scrollHeight ?? 0;
    const anchorTop = timelineEl?.scrollTop ?? 0;
    if (topLevel.length > windowSize) {
      windowSize += WINDOW_STEP;
    } else if (hasOlderByChannel[activeChannelId]) {
      loadingOlder = true;
      try {
        const oldest = messages[0]?.sequence ?? 0;
        const packet = await client.read(
          `${timelinePath}?channel_id=${encodeURIComponent(activeChannelId)}&before_sequence=${oldest}&limit=${WINDOW_STEP}`
        );
        const older = (Array.isArray(packet.messages) ? packet.messages : []).map(normalizeMessage);
        const known = new Set(allMessages.map((message) => message.receiptId));
        allMessages = capMessages([...older.filter((message) => !known.has(message.receiptId)), ...allMessages].sort(
          (a, b) => a.sequence - b.sequence
        ));
        windowSize += WINDOW_STEP;
        hasOlderByChannel = { ...hasOlderByChannel, [activeChannelId]: Boolean(packet.window?.has_older) };
      } catch (error) {
        // The kernel is not answering; the button stays and says so by doing nothing.
      }
      loadingOlder = false;
    }
    await tick();
    if (timelineEl) {
      timelineEl.scrollTop = anchorTop + (timelineEl.scrollHeight - anchorHeight);
    }
  }

  // Copying a message's record id lives inside its Details disclosure
  // (the Provenance card copies any row) - the primary row stays human.

  // Promote a message into a page draft: the words land in the pages
  // editor's machine-local store and the editor opens with them. The exact
  // Message receipt rides with the draft so the governed save can preserve
  // its lineage instead of turning this into an untraceable copy.
  function draftPageFrom(message) {
    const title = `Notes from #${activeChannelId}`;
    const slug = `page_notes_from_${activeChannelId.replace(/[^a-z0-9]+/g, "_")}`;
    try {
      const existing = JSON.parse(localStorage.getItem(`mdx-pages-editor:${slug}`) ?? "null");
      const text = `${existing?.text ? existing.text + "\n\n" : ""}> ${message.body}\n> - ${humanToken(message.actor)}`;
      localStorage.setItem(
        `mdx-pages-editor:${slug}`,
        JSON.stringify({
          title,
          text,
          origin: [message.receiptId, "message"]
        })
      );
      location.href = `/pages?draft=${slug}`;
    } catch (error) {
      // hand-off is best effort
    }
  }

  function recordAsDecision(message) {
    draft = `Decision: \n${message.body}\nWhy: `;
    composerEl?.focus();
  }

  // Send a message to the incoming stream: it becomes a triage entry in
  // the message's own words, pointing back at this exact message. The
  // call on it happens in Incoming, not here.
  let sentToTriage = $state("");
  async function sendToTriage(message) {
    try {
      const packet = await client.write(
        "/product/triage-entries.json",
        {
          text: String(message.body ?? "").slice(0, 1000),
          source: "message",
          source_ref: message.receiptId,
          actor_id: data.session?.user_id ?? "local_user"
        },
        { receiptIntent: "triage_entry" }
      );
      if (packet?.status === "RECORDED") sentToTriage = message.receiptId;
    } catch (error) {
      sentToTriage = "";
    }
  }

  function jumpToUnread() {
    const target = unreadStartSeq;
    if (!target) return;
    const newerCount = topLevel.filter((message) => message.sequence >= target).length;
    windowSize = Math.max(windowSize, newerCount + 5);
    tick().then(() => {
      document.querySelector(".unread-line")?.scrollIntoView({ block: "center" });
    });
  }

  const unreadAboveWindow = $derived(
    unreadStartSeq > 0 && visibleTopLevel.length > 0 && visibleTopLevel[0].sequence > unreadStartSeq
  );

  function jumpToNew() {
    pendingNew = 0;
    scrollToEnd();
    storeSeen(latestSeq());
  }

  function onTimelineScroll() {
    if (!timelineEl || pendingNew === 0) return;
    const nearBottom = timelineEl.scrollHeight - timelineEl.scrollTop - timelineEl.clientHeight < 48;
    if (nearBottom) {
      pendingNew = 0;
      storeSeen(latestSeq());
    }
  }

  onMount(() => {
    const locationURL = new URL(location.href);
    const linkedThread = locationURL.searchParams.get("thread");
    const linkedChannel = locationURL.searchParams.get("channel");
    if (linkedThread) openThreadId = linkedThread;
    try {
      const stored = localStorage.getItem(CHANNEL_KEY);
      if (linkedChannel) {
        activeChannelId = linkedChannel;
      } else if (stored) {
        activeChannelId = stored;
      } else {
        // First visit: open where the people are talking. #general is the
        // narrated welcome when it exists; the agent-ops lane is never the
        // first thing a new person reads.
        firstVisit = true;
      }
    } catch (error) {
      // tab-local only
    }
    myName = loadProfile().name ?? "";
    try {
      notificationsEnabled = typeof Notification !== "undefined" && localStorage.getItem("mdx-message-notifications") === "on" && Notification.permission === "granted";
    } catch (error) {
      notificationsEnabled = false;
    }
    try {
      mutedChannels = JSON.parse(localStorage.getItem(MUTED_KEY) ?? "[]");
    } catch (error) {
      mutedChannels = [];
    }
    outbox.load().then(() => outbox.flush());
    const onVisible = () => {
      if (document.visibilityState === "visible") outbox.flush();
    };
    // Two signals compose: the banner's online state is the KERNEL reachability
    // (the shared store), but the browser's own online event still matters as a
    // trigger. When the machine's network returns, push the queued sends and
    // re-probe the kernel at once, rather than waiting for the next heartbeat.
    const onBrowserOnline = () => {
      outbox.retryNow();
      retryReachability();
    };
    window.addEventListener("online", onBrowserOnline);
    document.addEventListener("visibilitychange", onVisible);
    loadConversation().then(async () => {
      try {
        const bridge = await client.read(bridgePath);
        descriptor = (bridge.descriptors ?? []).find((entry) => entry.domain === "message") ?? null;
        if (descriptor) {
          // Subscribe across every known channel so unread counts move
          // even for rooms you are not looking at. The tenant and replay
          // discipline still come from the descriptor.
          const channelIds = [...new Set([...channels.map((entry) => entry.id), activeChannelId])];
          descriptor = {
            ...descriptor,
            subscribe_payload: { ...descriptor.subscribe_payload, channel_ids: channelIds }
          };
          relay = connectRelay({
            descriptor,
            cursor,
            onState: (state) => (liveState = state),
            onEnvelope,
            onEvent,
            // A gap or required snapshot fills from the projection - the
            // saved record is the truth the relay's cache could not cover.
            onGap: () => loadConversation()
          });
        }
      } catch (error) {
        // No bridge metadata: the thread stays a saved conversation, and
        // nothing pretends to be live.
      }
    });
    return () => {
      relay?.stop();
      outbox.dispose();
      window.removeEventListener("online", onBrowserOnline);
      document.removeEventListener("visibilitychange", onVisible);
      clearTimeout(typingStopTimer);
      Object.values(typingTimers).forEach(clearTimeout);
    };
  });

  // When the kernel comes back (the shared reachability store flips online),
  // push the queued outbox immediately instead of waiting for the next tick.
  let wasOnline = true;
  $effect(() => {
    const nowOnline = online;
    if (nowOnline && !wasOnline) outbox.retryNow();
    wasOnline = nowOnline;
  });
</script>

<svelte:head><title>Message - MDx</title></svelte:head>

<section class="message-route" data-route-state="ready">
  <header class="message-head">
    <div class="mdx-page-head">
      <h1>Message</h1>
      <p class="mdx-page-sub">
        <span class="room-name">{threadTitle}</span>{activeChannel?.description ? ` - ${activeChannel.description}` : " - people and agents in one conversation."}
      </p>
    </div>
    <button type="button" class="head-icon inbox-trigger" class:has-pending={pendingForYou.length > 0} onclick={() => (inboxOpen = true)} aria-label="Open your inbox" title="Your inbox - decisions and asks waiting on you">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="14" height="14" aria-hidden="true"><path d="M22 12h-6l-2 3h-4l-2-3H2"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/></svg>
      {#if pendingForYou.length > 0}<span class="inbox-count" class:decisions={inboxDecisionCount > 0}>{pendingForYou.length}</span>{/if}
    </button>
    <button type="button" class="head-icon" onclick={openChannelSettings} aria-label="Channel settings" title="Channel settings">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="14" height="14" aria-hidden="true"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
    </button>
    <button type="button" class="head-icon" onclick={() => (searchOpen = !searchOpen)} aria-label="Search messages" title="Search messages">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" width="14" height="14" aria-hidden="true"><circle cx="11" cy="11" r="7"/><line x1="21" y1="21" x2="16.5" y2="16.5"/></svg>
    </button>
    <button type="button" class="head-action" class:active={catchUpOpen} onclick={() => (catchUpOpen = !catchUpOpen)} aria-expanded={catchUpOpen}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="14" height="14" aria-hidden="true"><path d="m12 3-1.6 4.4L6 9l4.4 1.6L12 15l1.6-4.4L18 9l-4.4-1.6L12 3Z"/><path d="m5 14-.9 2.1L2 17l2.1.9L5 20l.9-2.1L8 17l-2.1-.9L5 14Z"/></svg>
      Catch up
    </button>
    <button type="button" class="head-icon" class:active={notificationsEnabled} onclick={enableNotifications} aria-label={notificationsEnabled ? "Message notifications are on" : "Turn on Message notifications"} title={notificationsEnabled ? "Notifications on" : "Notify me about new messages while this tab is hidden"}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="14" height="14" aria-hidden="true"><path d="M18 8a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9"/><path d="M10 21h4"/></svg>
    </button>
    {#if liveState === "live"}
      <span class="live-dot" title="Connected - new messages arrive as they happen">
        <span class="pulse" aria-hidden="true"></span>
        live
      </span>
    {:else if liveState === "catching_up" || liveState === "syncing"}
      <span class="live-dot catching" title="Reconnected - filling in what happened while you were away">
        catching up...
      </span>
    {/if}
  </header>

  {#if showFirstUseHint}
    <FirstUseHint
      surface="message"
      title="First time in Message?"
      body="People and agents share one conversation here. Everything said is kept, and anything worth keeping can become a page or a decision - without leaving the thread. Delivery stays on this machine until production turns on."
    />
  {/if}

  {#if searchOpen}
    <MessageSearchPanel bind:query={searchQuery} {searching} results={searchResults} onSearch={runSearch} onJump={jumpToResult} />
  {/if}

  {#if catchUpOpen}
    <MessageCatchUpPanel channel={activeChannelId} messages={topLevel} pending={pendingForYou.filter((item) => item.channelId === activeChannelId)} onClose={() => (catchUpOpen = false)} />
  {/if}

  {#if inboxOpen}
    <div class="inbox-scrim" role="presentation" onclick={() => (inboxOpen = false)}></div>
    <aside class="inbox-drawer mdx-elevated" aria-label="Your inbox">
      <header class="inbox-head">
        <div>
          <p class="inbox-eyebrow">Your inbox</p>
          <h2>{pendingForYou.length === 0 ? "All clear" : `${pendingForYou.length} waiting on you`}</h2>
        </div>
        <button type="button" class="head-icon" onclick={() => (inboxOpen = false)} aria-label="Close inbox" title="Close">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" width="14" height="14" aria-hidden="true"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
        </button>
      </header>
      {#if pendingForYou.length === 0}
        <div class="inbox-empty">
          <p>Nothing needs a decision right now.</p>
          <p class="inbox-empty-sub">When an agent asks for your call, or names you, it lands here - across every channel.</p>
        </div>
      {:else}
        <ul class="inbox-list">
          {#each pendingForYou as item (item.receiptId)}
            <li class="inbox-item" data-reason={item.reason}>
              <div class="inbox-item-head">
                <span class="inbox-tag" data-reason={item.reason}>{item.reason === "decision" ? "Decision" : item.reason === "mention" ? "Mention" : "Ask"}</span>
                <span class="inbox-where">{item.actor} · #{item.channelId}</span>
                {#if item.createdAt}<time class="inbox-time" datetime={item.createdAt} title={fullTime(item.createdAt)}>{relTime(item.createdAt, latestActivityMs)}</time>{/if}
              </div>
              {#if item.title}<p class="inbox-title">{item.title}</p>{/if}
              <p class="inbox-preview">{item.preview}</p>
              <div class="inbox-actions">
                {#if item.reason === "decision"}
                  {@const reqId = actionView(item.message).requestReceiptId}
                  <button type="button" class="verdict primary" disabled={Boolean(actionBusy)} aria-busy={actionBusy === `${reqId}:approve`} onclick={() => recordActionVerdict(item.message, "approve", "")}>Approve</button>
                  <button type="button" class="verdict danger" disabled={Boolean(actionBusy)} onclick={() => recordActionVerdict(item.message, "reject", "")}>Decline</button>
                  <button type="button" class="verdict ghost" onclick={() => { inboxOpen = false; focusJump(item); }}>Open</button>
                {:else}
                  <button type="button" class="verdict ghost" onclick={() => { inboxOpen = false; focusJump(item); }}>Open in #{item.channelId}</button>
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </aside>
  {/if}

  {#if channelSettingsOpen}
    <section class="channel-settings mdx-elevated" aria-label="Channel settings">
      <header class="cs-head">
        <div>
          <span class="cs-eyebrow">Channel</span>
          <h2>#{activeChannelId}</h2>
        </div>
        <button type="button" class="cs-close" onclick={() => (channelSettingsOpen = false)} aria-label="Close channel settings">Close</button>
      </header>
      <label class="cs-field">
        <span>Name</span>
        <input type="text" bind:value={channelDraft.name} placeholder="Channel name" maxlength="80" aria-label="Channel name" />
      </label>
      <label class="cs-field">
        <span>Description</span>
        <input type="text" bind:value={channelDraft.description} placeholder="What this channel is for" maxlength="280" aria-label="Channel description" />
      </label>
      <label class="cs-field">
        <span>Topic</span>
        <input type="text" bind:value={channelDraft.topic} placeholder="What's happening right now" maxlength="120" aria-label="Channel topic" />
      </label>
      <div class="cs-field">
        <span>Who can see this</span>
        <div class="cs-visibility">
          <button type="button" class="vis-opt" class:on={channelDraft.visibility === "public"} onclick={() => (channelDraft.visibility = "public")}>
            Anyone in the tenant
          </button>
          <button type="button" class="vis-opt" class:on={channelDraft.visibility === "private"} onclick={() => (channelDraft.visibility = "private")}>
            Members only
          </button>
        </div>
        {#if channelDraft.visibility === "private"}
          <p class="cs-note">Private: only members see this channel and its messages. Even a tenant admin cannot read it without being a member.</p>
        {/if}
      </div>
      <div class="cs-members">
        <span class="cs-label">Members</span>
        {#if activeMembers.length > 0}
          <ul class="member-list">
            {#each activeMembers as member, i (i)}
              <li class="member-row">
                <span class="member-avatar" data-kind={member.actor_id.startsWith("agent:") ? "agent" : "human"} aria-hidden="true">{humanToken(member.actor_id.replace(/^[a-z]+:/, "")).slice(0, 1)}</span>
                <span class="member-name">{humanToken(member.actor_id.replace(/^[a-z]+:/, ""))}</span>
                <span class="member-role">{member.role}</span>
                {#if member.role !== "owner"}
                  <button type="button" class="member-remove" onclick={() => removeMember(member.actor_id)} aria-label="Remove {member.actor_id}">Remove</button>
                {/if}
              </li>
            {/each}
          </ul>
        {:else}
          <p class="cs-note">No members yet - the creator is the owner. Add people or agents below.</p>
        {/if}
        <div class="member-add">
          <input
            type="text"
            bind:value={memberDraft.actorId}
            placeholder="Add a person or agent (e.g. priya, agent:scout)"
            onkeydown={(event) => { if (event.key === "Enter") addMember(); }}
            aria-label="Add a member"
          />
          <select bind:value={memberDraft.role} aria-label="Member role">
            <option value="member">Member</option>
            <option value="observer">Observer</option>
            <option value="owner">Owner</option>
          </select>
          <button type="button" class="member-add-btn" onclick={addMember} disabled={!memberDraft.actorId.trim()}>Add</button>
        </div>
      </div>
      <div class="cs-actions">
        <button type="button" class="cs-archive" onclick={archiveChannel}>Archive channel</button>
        <button type="button" class="cs-save" onclick={saveChannelSettings} disabled={!channelDraft.name.trim() || channelSaveState === "saving"}>
          {channelSaveState === "saving" ? "Saving..." : "Save"}
        </button>
      </div>
      <p class="cs-note">Renaming, describing, and archiving are recorded - the channel's history stays on the record. Archiving retires it from the list without deleting what was said.</p>
    </section>
  {/if}

  {#if focusVisible}
    <section class="focus mdx-elevated" aria-label="What your conversations need from you">
      <header class="focus-head">
        <div class="focus-title">
          <span class="focus-eyebrow">Focus</span>
          <h2>{focusItems.length} {focusItems.length === 1 ? "conversation needs" : "conversations need"} you</h2>
        </div>
        <button type="button" class="focus-dismiss" onclick={() => (focusDismissed = true)} aria-label="Clear focus for now">
          Clear
        </button>
      </header>
      <ul class="focus-list">
        {#each focusItems as item, i (i)}
          <li>
            <button type="button" class="focus-item" data-lane={item.actorType} onclick={() => focusJump(item)}>
              <span class="focus-reason" data-reason={item.reason}>{item.reason === "mention" ? "@you" : item.reason === "decision" ? "decide" : "asks"}</span>
              <span class="focus-main">
                <span class="focus-who">{item.actor}<span class="focus-channel">#{item.channelId}</span></span>
                <span class="focus-preview">{item.preview}</span>
              </span>
              <span class="focus-go" aria-hidden="true">→</span>
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  <section class="mobile-channels" aria-label="The team's channels">
    <span>The team's channels</span>
    <select
      value={activeChannelId}
      aria-label="Open channel"
      onchange={(event) => openChannel(event.currentTarget.value)}
    >
      {#each channels as channel}
        <option value={channel.id}>{channelLabel(channel.id)}</option>
      {/each}
    </select>
  </section>

  <div class="room-body">
  <aside class="channels" aria-label="Channels">
    <div class="channels-head">
      <span>The team's channels</span>
      <span class="channels-head-actions">
        <button type="button" class="add-channel" onclick={() => { addingDm = !addingDm; addingChannel = false; }} aria-label="New direct message" title="New direct message">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="13" height="13" aria-hidden="true"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>
        </button>
        <button type="button" class="add-channel" onclick={() => { addingChannel = !addingChannel; addingDm = false; }} aria-label="New channel">+</button>
      </span>
    </div>
    {#if addingChannel}
      <input
        class="channel-input"
        type="text"
        placeholder="new-channel-name"
        bind:value={newChannelName}
        onkeydown={(event) => { if (event.key === "Enter") createChannel(); if (event.key === "Escape") addingChannel = false; }}
        aria-label="New channel name"
      />
    {/if}
    {#if addingDm}
      <input
        class="channel-input"
        type="text"
        placeholder="message who? (e.g. priya, agent:scout)"
        bind:value={newDmTarget}
        onkeydown={(event) => { if (event.key === "Enter") createDm(); if (event.key === "Escape") addingDm = false; }}
        aria-label="Direct message recipient"
      />
    {/if}
    <ul class="channel-list">
      {#each channels as channel}
        <li>
          <button
            type="button"
            class="channel"
            class:active={channel.id === activeChannelId}
            onclick={() => openChannel(channel.id)}
          >
            <span class="channel-name" class:muted={channelMuted(channel.id)}>
              {#if channelMeta.get(channel.id)?.visibility === "private"}
                <svg class="channel-lock" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="11" height="11" aria-label="Private"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>
              {:else}
                <span class="hash">#</span>
              {/if}{channelLabel(channel.id)}</span>
            {#if (mentionsByChannel[channel.id] ?? 0) > 0}
              <span class="channel-mention">@{mentionsByChannel[channel.id]}</span>
            {:else if (unreadByChannel[channel.id] ?? 0) > 0}
              <span class="channel-unread">{unreadByChannel[channel.id]}</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  </aside>
  <div class="room-main">
  {#if activeRoster.length > 0}
    <div class="presence" aria-label="Who's active right now">
      <span class="presence-stack">
        {#each activeRoster.slice(0, 6) as person, i (i)}
          <span class="presence-face" data-lane={person.actor_type} title="{person.display_name} - last: {person.last_action}{person.last_active_at ? ` · ${relTime(person.last_active_at, latestActivityMs)}` : ''}">
            {person.display_name.slice(0, 1).toUpperCase()}
            <span class="presence-pulse" aria-hidden="true"></span>
          </span>
        {/each}
      </span>
      <span class="presence-line">{presenceSummary}</span>
      {#if !Number.isNaN(latestActivityMs)}
        <span class="presence-synced" title="Newest event on this timeline by trusted receipt time: {fullTime(new Date(latestActivityMs).toISOString())}">· up to date</span>
      {/if}
    </div>
  {/if}
  <div class="timeline" bind:this={timelineEl} onscroll={onTimelineScroll} aria-live="polite">
    {#if !loaded}
      <p class="quiet">Opening the conversation...</p>
    {:else if activeIsActivity}
      <div class="feed">
        {#if activeFeed.length === 0}
          <div class="welcome">
            <h2>Quiet for now.</h2>
            <p>This channel is a live feed of everything happening here - every run, ship, and decision in this area lands the moment it's on the record.</p>
          </div>
        {:else}
          {#if feedTruncated > 0}
            <p class="feed-earlier">{feedTruncated.toLocaleString()} earlier {feedTruncated === 1 ? "event" : "events"} above - showing the most recent {activeFeed.length}.</p>
          {/if}
          {#each activeFeed as item, i (i)}
            {#if item.feed_kind === "message"}
              <article class="msg feed-msg" data-lane={item.actor_type}>
                <span class="avatar" data-kind={item.actor_type} aria-hidden="true">{item.actor.slice(0, 1).toUpperCase()}</span>
                <div class="msg-body">
                  <p class="msg-who">{item.actor}{#if item.actor_type !== "human"}<span class="who-tag">{item.actor_type}</span>{/if}</p>
                  <div class="msg-text"><Markdown text={item.detail} /></div>
                </div>
                <span class="msg-proof">
                  <Provenance label="Message details" items={[{ label: "This message", value: item.receipt_id }]} />
                </span>
              </article>
            {:else}
              <article class="feed-card" data-lane={item.actor_type}>
                <span class="feed-rail" aria-hidden="true"></span>
                <div class="feed-card-body">
                  <p class="feed-headline">
                    {#if item.href}
                      <a class="feed-link" href={item.href}>{item.headline}</a>
                    {:else}
                      {item.headline}
                    {/if}
                    {#if item.repeatCount > 1}<span class="feed-repeat" title="{item.repeatCount} identical events, each with its own receipt on the ledger">×{item.repeatCount}</span>{/if}
                  </p>
                  {#if item.detail}<p class="feed-detail">{item.detail}</p>{/if}
                  <p class="feed-meta">
                    <span class="feed-kind">{item.receipt_kind}</span>
                    <Provenance label="Activity details" items={[{ label: "This event", value: item.receipt_id }, { label: "Kind", value: item.receipt_kind }]} />
                  </p>
                </div>
              </article>
            {/if}
          {/each}
        {/if}
      </div>
    {:else if empty}
      <div class="welcome">
        <h2>Start the conversation.</h2>
        <p>{data.viewport.currentSignal}</p>
        <p class="welcome-sub">{data.viewport.judgment}</p>
      </div>
    {:else}
      {#if unreadAboveWindow}
        <button type="button" class="show-earlier unread-jump" onclick={jumpToUnread}>
          Jump to where you left off
        </button>
      {/if}
      {#if earlierAvailable}
        <button type="button" class="show-earlier" onclick={showEarlier} disabled={loadingOlder}>
          {loadingOlder ? "Fetching earlier messages..." : "Show earlier messages"}
        </button>
      {/if}
      {#snippet messageRow(message)}
        {@const replies = repliesByParent.get(message.receiptId) ?? []}
        {#if unreadStartSeq > 0 && message.sequence === unreadStartSeq}
          <div class="unread-line" role="separator"><span>new since you were here</span></div>
        {/if}
        <article
          class="msg"
          class:self={message.actorType === "human"}
          class:mentions-me={mentionsMe(message.body)}
          class:flash={flashSeq > 0 && message.sequence === flashSeq}
          data-lane={message.actorType}
          data-seq={message.sequence}
        >
          {#if message.grouped}
            <span class="avatar-spacer" aria-hidden="true"></span>
          {:else}
            <span class="avatar" data-kind={message.actorType} aria-hidden="true">{message.actor.slice(0, 1).toUpperCase()}</span>
          {/if}
          <div class="msg-body">
            {#if !message.grouped}
            <p class="msg-who">
              {humanToken(message.actor)}
              {#if message.actorType !== "human"}<span class="who-tag">{message.actorType}</span>{/if}
              {#if mentionsMe(message.body)}<span class="mention-tag">@you</span>{/if}
              {#if message.createdAt}<time class="msg-time" datetime={message.createdAt} title={fullTime(message.createdAt)}>{relTime(message.createdAt, latestActivityMs)}</time>{/if}
            </p>
            {:else if mentionsMe(message.body)}
            <p class="msg-who"><span class="mention-tag">@you</span></p>
            {/if}
            {#if message.contentType === "card"}
              {@const card = cardView(message)}
              <div class="event-card" data-lane={message.actorType}>
                <span class="event-rail" aria-hidden="true"></span>
                <div class="event-card-body">
                  <p class="event-eyebrow">
                    {card.label}
                    {#if card.state}<span class="event-state" data-tone={card.state.tone}>{card.state.label}</span>{/if}
                  </p>
                  <div class="event-line"><Markdown text={message.body} /></div>
                  {#if card.summary}<p class="event-summary">{card.summary}</p>{/if}
                </div>
              </div>
            {:else if message.contentType === "action_request"}
              {@const action = actionView(message)}
              {@const decided = actionDecisions.get(action.requestReceiptId)}
              {@const typing = actionInput && actionInput.requestId === action.requestReceiptId}
              <div class="action-card" data-state={decided ? "resolved" : "pending"}>
                <p class="action-eyebrow">
                  {#if decided}Decision recorded<span class="action-state" data-tone="done">{humanToken(decided.verdict)}</span>
                  {:else}An agent needs your decision<span class="action-state" data-tone="review">Ready for review</span>{/if}
                </p>
                <p class="action-title">{action.title}</p>
                {#if action.summary}<p class="action-summary">{action.summary}</p>{/if}
                <div class="action-line"><Markdown text={message.body} /></div>
                <p class="action-links">
                  <a href={`/evidence/${encodeURIComponent(action.requestReceiptId)}`}>the request on the record →</a>
                  {#if /forge/i.test(String(message.sourceReceiptKind ?? "") + action.title)}
                    <a href="/forge/review">open in Forge →</a>
                  {/if}
                </p>
                {#if decided}
                  <p class="action-note">{verdictOutcome(decided.verdict)}</p>
                {:else if typing}
                  <div class="action-input">
                    <textarea
                      rows="2"
                      bind:value={actionInputText}
                      placeholder={actionInput.verdict === "respond" ? "Your answer becomes the result on the record" : "Describe the change you want before approving"}
                      aria-label={actionInput.verdict === "respond" ? "Your answer" : "Your edit"}
                    ></textarea>
                    <div class="action-input-row">
                      <button
                        type="button"
                        class="verdict primary"
                        disabled={!actionInputText.trim() || Boolean(actionBusy)}
                        onclick={() => recordActionVerdict(message, actionInput.verdict, actionInputText)}
                      >
                        {actionInput.verdict === "respond" ? "Send answer" : "Approve with edits"}
                      </button>
                      <button type="button" class="verdict ghost" onclick={cancelVerdictInput}>Cancel</button>
                    </div>
                  </div>
                {:else}
                  <div class="action-verdicts" role="group" aria-label="Your decision">
                    <button type="button" class="verdict primary" data-verdict="approve" disabled={Boolean(actionBusy)} aria-busy={actionBusy === `${action.requestReceiptId}:approve`} onclick={() => beginVerdict(message, "approve")}>Approve</button>
                    <button type="button" class="verdict danger" data-verdict="reject" disabled={Boolean(actionBusy)} aria-busy={actionBusy === `${action.requestReceiptId}:reject`} onclick={() => beginVerdict(message, "reject")}>Decline</button>
                    <button type="button" class="verdict ghost" data-verdict="edit" disabled={Boolean(actionBusy)} onclick={() => beginVerdict(message, "edit")}>Edit</button>
                    <button type="button" class="verdict ghost" data-verdict="respond" disabled={Boolean(actionBusy)} onclick={() => beginVerdict(message, "respond")}>Respond</button>
                  </div>
                  <p class="action-note" class:action-error={Boolean(actionErrors[action.requestReceiptId])}>
                    {actionErrors[action.requestReceiptId] || "Your call is recorded the moment you choose. Nothing runs from here."}
                  </p>
                {/if}
              </div>
            {:else if message.contentType === "action_response"}
              {@const action = actionView(message)}
              <div class="action-card action-result" data-state="resolved">
                <p class="action-eyebrow">Action result {#if action.verdict}<span class="action-state" data-tone="done">{humanToken(action.verdict)}</span>{/if}</p>
                <div class="action-line"><Markdown text={message.body} /></div>
              </div>
            {:else if decisionParts(message.body)}
              {@const decision = decisionParts(message.body)}
              <div class="decision-card">
                <p class="decision-eyebrow">Decision</p>
                <p class="decision-title">{decision.title}</p>
                {#if decision.detail}<p class="decision-detail">{decision.detail}</p>{/if}
                {#if decision.why}<p class="decision-why">Why: {decision.why}</p>{/if}
              </div>
            {:else}
              <div class="msg-text"><Markdown text={message.body} /></div>
            {/if}
            {#if reactionsByParent.get(message.receiptId)}
              <div class="react-chips">
                {#each reactionsByParent.get(message.receiptId) as chip (chip.emoji)}
                  <button
                    type="button"
                    class="react-chip"
                    class:mine={chip.mine}
                    onclick={() => toggleReaction(message, chip.emoji)}
                    aria-label="{chip.emoji} {chip.count}, toggle your reaction"
                  >
                    {chip.emoji} {chip.count}
                  </button>
                {/each}
              </div>
            {/if}
            {#if replies.length > 0 && openThreadId !== message.receiptId}
              <button type="button" class="thread-open" onclick={() => toggleThread(message.receiptId)}>
                {replies.length} {replies.length === 1 ? "reply" : "replies"}
              </button>
            {/if}
            {#if openThreadId === message.receiptId}
              <div class="thread">
                {#each replies as reply}
                  <article class="reply">
                    <span class="avatar reply-avatar" data-kind={reply.actorType} aria-hidden="true">{reply.actor.slice(0, 1).toUpperCase()}</span>
                    <div class="msg-body">
                      <p class="msg-who">
                        {humanToken(reply.actor)}
                        {#if reply.actorType !== "human"}<span class="who-tag">{reply.actorType}</span>{/if}
                      </p>
                      <p class="msg-text">{reply.body}</p>
                    </div>
                    <span class="msg-proof">
                      <Provenance label="Reply details" items={[{ label: "This reply", value: reply.receiptId }, { label: "Position", value: reply.sequence ? `#${reply.sequence}` : "" }]} />
                    </span>
                  </article>
                {/each}
                {#each pendingByParent.get(message.receiptId) ?? [] as item (item.messageId)}
                  <article class="reply pending" data-pending-state={item.status}>
                    <span class="avatar reply-avatar" data-kind="human" aria-hidden="true">Y</span>
                    <div class="msg-body">
                      <p class="msg-who">you</p>
                      <p class="msg-text">{item.body}</p>
                      {#if item.status === "failed"}
                        <button type="button" class="pending-retry" onclick={() => outbox.retryNow()}>
                          Not sent yet - tap to retry
                        </button>
                      {:else}
                        <p class="pending-note">{online ? "sending..." : "will send when you're back online"}</p>
                      {/if}
                    </div>
                  </article>
                {/each}
                {#if replyState === "error"}
                  <p class="send-error" role="alert">Not sent - a reply only counts once it is saved. Try again.</p>
                {/if}
                <div class="reply-input">
                  <textarea
                    rows="1"
                    placeholder="Reply..."
                    bind:value={replyDraft}
                    onkeydown={(event) => onReplyKeydown(event, message.receiptId)}
                    aria-label="Write a reply"
                  ></textarea>
                  <button
                    type="button"
                    class="send-btn reply-send"
                    onclick={() => sendReply(message.receiptId)}
                    disabled={!replyDraft.trim() || replyState === "sending"}
                    aria-label="Send reply"
                  >
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="14" height="14" aria-hidden="true">
                      <line x1="12" y1="19" x2="12" y2="5" />
                      <polyline points="5 12 12 5 19 12" />
                    </svg>
                  </button>
                </div>
              </div>
            {/if}
          </div>
          <span class="msg-actions">
            <span class="react-bar" class:off={!reactionsEnabled}>
              {#each REACTION_SET as emoji (emoji)}
                <button type="button" class="react-add" onclick={() => toggleReaction(message, emoji)} aria-label="React {emoji}">
                  {emoji}
                </button>
              {/each}
            </span>
            <button type="button" class="msg-act" onclick={() => draftPageFrom(message)} aria-label="Draft a page from this" title="Draft a page from this">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="14" height="14" aria-hidden="true"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>
            </button>
            <button type="button" class="msg-act" onclick={() => recordAsDecision(message)} aria-label="Record as decision" title="Record as decision">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="14" height="14" aria-hidden="true"><path d="M9 11l3 3L22 4"/><path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11"/></svg>
            </button>
            <button type="button" class="msg-act" onclick={() => toggleThread(message.receiptId)} aria-label="Reply in thread" title="Reply in thread">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="14" height="14" aria-hidden="true">
                <polyline points="9 17 4 12 9 7" />
                <path d="M20 18v-2a4 4 0 0 0-4-4H4" />
              </svg>
            </button>
            {#if sentToTriage === message.receiptId}
              <a class="msg-act-done" href="/product-direction/triage" title="It waits in Incoming">in Incoming →</a>
            {:else}
              <button type="button" class="msg-act" onclick={() => sendToTriage(message)} aria-label="Send to triage" title="Send to triage">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="14" height="14" aria-hidden="true">
                  <path d="M22 12h-6l-2 3h-4l-2-3H2" />
                  <path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" />
                </svg>
              </button>
            {/if}
            <Provenance label="Message details" items={messageProvenance(message)} />
          </span>
        </article>
      {/snippet}
      {#each renderTimeline as item (item.id)}
        {#if item.kind === "run" && !expandedRuns[item.id]}
          <button
            type="button"
            class="run-fold"
            data-lane={runLane(item.entries)}
            onclick={() => (expandedRuns = { ...expandedRuns, [item.id]: true })}
          >
            <span class="run-gear" aria-hidden="true">⚙</span>
            {runActors(item.entries)} moved the work forward - {item.entries.length} steps, every one on the record
            <span class="run-act">show</span>
          </button>
        {:else if item.kind === "run"}
          <div class="run-open" data-lane={runLane(item.entries)}>
            <button
              type="button"
              class="run-fold open"
              onclick={() => (expandedRuns = { ...expandedRuns, [item.id]: false })}
            >
              <span class="run-gear" aria-hidden="true">⚙</span>
              {runActors(item.entries)} - {item.entries.length} updates
              <span class="run-act">fold</span>
            </button>
            {#each item.entries as message (message.receiptId)}
              {@render messageRow(message)}
            {/each}
          </div>
        {:else}
          {@render messageRow(item.entry)}
        {/if}
      {/each}
    {/if}
    {#each pendingTopLevel as item (item.messageId)}
      <article class="msg self pending" data-pending-state={item.status}>
        <span class="avatar" data-kind="human" aria-hidden="true">Y</span>
        <div class="msg-body">
          <p class="msg-who">you</p>
          <p class="msg-text">{item.body}</p>
          {#if item.status === "failed"}
            <button type="button" class="pending-retry" onclick={() => outbox.retryNow()}>
              Not sent yet - tap to retry
            </button>
          {:else}
            <p class="pending-note">{online ? "sending..." : "will send when you're back online"}</p>
          {/if}
        </div>
      </article>
    {/each}
  </div>

  {#if pendingNew > 0}
    <button type="button" class="jump-new" onclick={jumpToNew}>
      {pendingNew} new {pendingNew === 1 ? "message" : "messages"} <span aria-hidden="true">↓</span>
    </button>
  {/if}

  {#if typingNames.length > 0}
    <p class="typing">
      <span class="dots" aria-hidden="true"><span></span><span></span><span></span></span>
      {typingNames.join(" and ")} {typingNames.length === 1 ? "is" : "are"} typing
    </p>
  {/if}

  {#if !online}
    <p class="net-banner" role="status">
      Offline - messages you send now wait here and go out the moment you're back.
      {#if lastSyncedAt}Last synced {lastSyncedAt.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })}.{/if}
    </p>
  {:else if pendingCount > 0}
    <p class="net-banner pending-banner" role="status">
      {pendingCount} {pendingCount === 1 ? "message" : "messages"} waiting to send{failedCount > 0 ? " - retrying" : ""}.
      <button type="button" class="banner-retry" onclick={() => outbox.retryNow()}>Send now</button>
    </p>
  {/if}

  {#if slashOpen}
    <div class="slash-menu" role="listbox" aria-label="Commands">
      {#each SLASH_COMMANDS as command, index (command.id)}
        <button
          type="button"
          class="slash-item"
          class:focused={index === slashIndex}
          role="option"
          aria-selected={index === slashIndex}
          onclick={() => runSlash(command)}
        >
          <strong>{command.label}</strong>
          <span>{command.id === "mute" && channelMuted(activeChannelId) ? "unmute this channel" : command.line}</span>
        </button>
      {/each}
    </div>
  {/if}

  {#if refusalNote}
    <p class="net-banner refusal" role="alert">{refusalNote}</p>
  {/if}

  <MarketplacePackPrompt app="message" objectId={threadId || activeChannelId} task={draft} session={data.session} compact />

  <footer class="composer-area">
    {#if attachmentOpen}
      <div class="attachment-composer" aria-label="Attach an MDx object">
        <label><span>Link</span><input type="url" bind:value={attachmentUrl} placeholder="/pages, /forge/runs, or https://…" aria-label="Attachment link" /></label>
        <label><span>Label</span><input type="text" bind:value={attachmentLabel} placeholder="What people should see" aria-label="Attachment label" /></label>
        <button type="button" onclick={() => { attachmentOpen = false; attachmentUrl = ""; attachmentLabel = ""; attachmentError = ""; }}>Cancel</button>
        {#if attachmentError}<p class="attachment-error" role="alert">{attachmentError}</p>{/if}
      </div>
    {/if}
    <div class="input-wrapper">
      <button type="button" class="attach-btn" class:active={attachmentOpen} onclick={() => (attachmentOpen = !attachmentOpen)} aria-label="Attach an MDx object" title="Attach a Page, Forge run, or other link">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="16" height="16" aria-hidden="true"><path d="M21.44 11.05 12.25 20.24a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"/></svg>
      </button>
      <textarea
        rows="1"
        bind:this={composerEl}
        placeholder="Message {threadTitle}..."
        bind:value={draft}
        onkeydown={onComposerKeydown}
        oninput={() => {
          onComposerInput();
          onComposerSlashInput();
        }}
        aria-label="Write a message"
      ></textarea>
      <button type="button" class="send-btn" onclick={send} disabled={!draft.trim() && !attachmentUrl.trim()} aria-label="Send">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="16" height="16" aria-hidden="true">
          <line x1="12" y1="19" x2="12" y2="5" />
          <polyline points="5 12 12 5 19 12" />
        </svg>
      </button>
    </div>
    {#if captureChip}
      <div class="capture-chip" role="status">
        <span>MDx noted this for the team. <a href="/memory">See it in Memory</a> once a teammate clears it.</span>
        <button type="button" class="capture-chip-dismiss" aria-label="Dismiss" onclick={() => (captureChip = false)}>Dismiss</button>
      </div>
    {/if}
    <div class="quiet-line">
      Messages stay on record {data.session?.verified ? "in this signed-in cloud workspace" : "on this Mac"}.
      <details class="quiet-more">
        <summary>more</summary>
        <span>Boundary: the browser queues a send if the kernel is unavailable; notifications, service-role fanout, presence mutation, and production writes keep their own governed gates. Safe next: send normally and follow the receipt into Memory when the note is worth keeping.</span>
      </details>
    </div>
  </footer>
  </div>
  </div>
</section>

<style>
  /* Walkthrough finding: the room scrolled inside a box while the page also
     scrolled - a double-scroll, cramped feel. When Message is mounted the
     stage fills its assigned shell row and the timeline is the only scroll,
     with the composer pinned below it. The grid row is shorter under the
     compact navigation, so a viewport-height override would overflow it. */
  :global(.host-stage:has(.message-route)) {
    height: auto;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  .message-route {
    /* A column of slots: head, optional panels, the room (which takes
       the remaining height), banners, composer. Flex keeps every
       optional slot honest - a grid with two fixed rows silently gave
       the search panel the room's row. */
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    max-width: 1040px;
    width: 100%;
    position: relative;
  }

  .message-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 16px;
    padding-bottom: 14px;
    border-bottom: 1px solid var(--mdx-border-subtle);
  }

  .head-action {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex: none;
    min-height: 30px;
    padding: 5px 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-secondary);
    font: inherit;
    font-size: var(--mdx-text-xs);
    cursor: pointer;
  }

  .head-action:hover,
  .head-action.active {
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 45%, var(--mdx-border-subtle));
    color: var(--mdx-accent-primary);
  }

  .live-dot.catching {
    color: var(--mdx-text-muted);
  }

  .live-dot {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    color: var(--mdx-accent-success);
    font-size: var(--mdx-text-xs);
  }

  .pulse {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: currentColor;
    animation: breathe 2.4s ease-in-out infinite;
  }

  @keyframes breathe {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.35; }
  }

  @media (prefers-reduced-motion: reduce) {
    .pulse { animation: none; }
  }

  /* Focus: the surface leads with what needs a person. Raised card, a
     reason chip per row, lane-aware, every row jumps to the message. */
  .focus {
    margin-bottom: 14px;
    padding: 13px 16px 14px;
    background: var(--mdx-surface-raised);
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    animation: mdx-page-enter 0.35s var(--mdx-ease-out, ease) both;
  }

  .focus-head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 10px;
  }

  .focus-eyebrow {
    display: block;
    font-size: var(--mdx-text-xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--mdx-accent-primary);
  }

  .focus-title h2 {
    margin: 2px 0 0;
    font-family: var(--mdx-font-display);
    font-size: 17px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .focus-dismiss {
    flex: none;
    border: none;
    background: none;
    color: var(--mdx-text-muted);
    font-size: 12.5px;
    cursor: pointer;
    padding: 4px 7px;
    border-radius: var(--mdx-radius-sm);
  }

  .focus-dismiss:hover {
    color: var(--mdx-text-primary);
    background: color-mix(in srgb, var(--mdx-text-muted) 12%, transparent);
  }

  .focus-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 3px;
  }

  .focus-item {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 9px 12px;
    border-radius: var(--mdx-radius-md);
    background: transparent;
    border: 1px solid transparent;
    text-align: left;
    cursor: pointer;
    color: inherit;
    transition: background 0.15s ease, border-color 0.15s ease, transform 0.15s ease;
  }

  .focus-item:hover {
    background: var(--mdx-surface-base);
    border-color: var(--mdx-border-default);
    transform: translateX(2px);
  }

  .focus-reason {
    flex: none;
    min-width: 48px;
    text-align: center;
    padding: 2px 9px;
    border-radius: var(--mdx-radius-pill);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.01em;
  }

  .focus-reason[data-reason="mention"] {
    color: var(--mdx-accent-primary);
    background: color-mix(in srgb, var(--mdx-accent-primary) 16%, transparent);
  }

  .focus-reason[data-reason="ask"] {
    color: var(--mdx-accent-warning);
    background: color-mix(in srgb, var(--mdx-accent-warning) 16%, transparent);
  }

  /* A decision waiting on you is the strongest call in the band: a filled
     chip, not a tinted one. */
  .focus-reason[data-reason="decision"] {
    color: var(--mdx-on-accent, #fff);
    background: var(--mdx-accent-primary);
  }

  .focus-main {
    display: grid;
    gap: 1px;
    min-width: 0;
    flex: 1;
  }

  .focus-who {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: 12px;
    color: var(--mdx-text-muted);
  }

  .focus-channel {
    font-family: var(--mdx-font-mono);
    font-size: 11px;
    color: var(--mdx-text-faint);
  }

  .focus-preview {
    font-size: 13.5px;
    color: var(--mdx-text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .focus-go {
    flex: none;
    color: var(--mdx-text-muted);
    transition: color 0.15s ease;
  }

  .focus-item:hover .focus-go {
    color: var(--mdx-accent-primary);
  }

  @media (prefers-reduced-motion: reduce) {
    .focus { animation: none; }
    .focus-item:hover { transform: none; }
  }

  /* The activity feed: the receipt ledger as a live stream of stack work.
     Each card is one receipt, rendered human, with its provenance one tap
     away. A lane-colored rail tells agent work from system from human. */
  .feed {
    display: grid;
    gap: 8px;
    padding: 4px 0;
  }

  .feed-earlier {
    margin: 0 0 2px;
    text-align: center;
    font-size: 12px;
    color: var(--mdx-text-muted);
  }

  .feed-card {
    display: flex;
    gap: 12px;
    padding: 11px 14px 12px;
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    border: 1px solid var(--mdx-border-subtle);
  }

  .feed-rail {
    flex: none;
    width: 3px;
    border-radius: 3px;
    background: var(--mdx-text-faint);
    align-self: stretch;
  }

  .feed-card[data-lane="agent"] .feed-rail { background: var(--mdx-accent-primary); }
  .feed-card[data-lane="system"] .feed-rail { background: var(--mdx-accent-purple, var(--mdx-text-muted)); }
  .feed-card[data-lane="human"] .feed-rail { background: var(--mdx-accent-success); }

  .feed-card-body {
    display: grid;
    gap: 3px;
    min-width: 0;
    flex: 1;
  }

  .feed-headline {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--mdx-text-primary);
    letter-spacing: -0.01em;
  }

  .feed-detail {
    margin: 0;
    font-size: 13px;
    color: var(--mdx-text-secondary, var(--mdx-text-muted));
    line-height: 1.5;
  }

  .feed-meta {
    margin: 2px 0 0;
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-muted);
  }

  .feed-kind {
    font-family: var(--mdx-font-mono);
    font-size: 11px;
    color: var(--mdx-text-faint);
    padding: 1px 7px;
    border-radius: var(--mdx-radius-pill);
    background: color-mix(in srgb, var(--mdx-text-muted) 10%, transparent);
  }

  .feed-msg {
    background: transparent;
  }

  .room-body {
    display: grid;
    grid-template-columns: 180px minmax(0, 1fr);
    gap: 0 18px;
    flex: 1;
    min-height: 0;
  }

  .room-main {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: auto minmax(0, 1fr) auto;
    min-height: 0;
    min-width: 0;
  }

  /* Presence: a small stack of the faces active now, with a one-line read. */
  .presence {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 2px 12px;
  }

  .presence-stack {
    display: flex;
    flex: none;
  }

  .presence-face {
    position: relative;
    width: 26px;
    height: 26px;
    margin-left: -7px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-size: 11px;
    font-weight: 700;
    color: var(--mdx-text-primary);
    background: color-mix(in srgb, var(--mdx-accent-success) 24%, var(--mdx-surface-raised));
    box-shadow: 0 0 0 2px var(--mdx-bg);
  }

  .presence-face:first-child {
    margin-left: 0;
  }

  .presence-face[data-lane="agent"] {
    background: color-mix(in srgb, var(--mdx-accent-primary) 26%, var(--mdx-surface-raised));
  }

  .presence-face[data-lane="system"] {
    background: color-mix(in srgb, var(--mdx-accent-purple) 24%, var(--mdx-surface-raised));
  }

  .presence-pulse {
    position: absolute;
    right: -1px;
    bottom: -1px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--mdx-accent-success);
    box-shadow: 0 0 0 2px var(--mdx-bg);
    animation: breathe 2.4s ease-in-out infinite;
  }

  .presence-line {
    font-size: 12.5px;
    color: var(--mdx-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .presence-synced {
    flex: none;
    font-size: 11.5px;
    color: var(--mdx-text-faint);
    cursor: default;
  }

  /* Agent Inbox: the cross-channel queue of what needs a human, openable from
     the header, decided in place. The trigger carries a live count; a pending
     decision tints it so an approval waiting on you is impossible to miss. */
  .inbox-trigger {
    position: relative;
  }

  .inbox-count {
    position: absolute;
    top: -5px;
    right: -5px;
    min-width: 15px;
    height: 15px;
    padding: 0 4px;
    border-radius: 8px;
    background: var(--mdx-text-muted);
    color: var(--mdx-on-accent, #fff);
    font-size: 10px;
    font-weight: 700;
    line-height: 15px;
    text-align: center;
  }

  .inbox-count.decisions {
    background: var(--mdx-accent-primary);
  }

  .inbox-scrim {
    position: fixed;
    inset: 0;
    background: color-mix(in srgb, #000 38%, transparent);
    z-index: 40;
  }

  .inbox-drawer {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(420px, 92vw);
    z-index: 41;
    background: var(--mdx-surface-base, var(--mdx-surface-raised));
    border-left: 1px solid var(--mdx-border-subtle);
    display: flex;
    flex-direction: column;
    box-shadow: var(--mdx-shadow-card);
  }

  .inbox-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding: 16px 16px 12px;
    border-bottom: 1px solid var(--mdx-border-subtle);
  }

  .inbox-head h2 {
    margin: 2px 0 0;
    font-size: 16px;
    font-weight: 600;
  }

  .inbox-eyebrow {
    margin: 0;
    font-size: var(--mdx-text-xs);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--mdx-text-muted);
  }

  .inbox-empty {
    padding: 32px 20px;
    text-align: center;
    color: var(--mdx-text-secondary, var(--mdx-text-muted));
  }

  .inbox-empty-sub {
    margin: 6px 0 0;
    font-size: var(--mdx-text-sm);
    color: var(--mdx-text-muted);
  }

  .inbox-list {
    list-style: none;
    margin: 0;
    padding: 8px;
    overflow-y: auto;
    display: grid;
    gap: 8px;
  }

  .inbox-item {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    padding: 11px 13px 12px;
    display: grid;
    gap: 5px;
  }

  .inbox-item[data-reason="decision"] {
    border-left: 3px solid var(--mdx-accent-primary);
  }

  .inbox-item-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .inbox-tag {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    padding: 1px 7px;
    border-radius: var(--mdx-radius-pill);
    color: var(--mdx-text-muted);
    background: color-mix(in srgb, var(--mdx-text-muted) 12%, transparent);
  }

  .inbox-tag[data-reason="decision"] {
    color: var(--mdx-on-accent, #fff);
    background: var(--mdx-accent-primary);
  }

  .inbox-tag[data-reason="mention"] {
    color: var(--mdx-accent-primary);
    background: color-mix(in srgb, var(--mdx-accent-primary) 16%, transparent);
  }

  .inbox-where {
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .inbox-time {
    margin-left: auto;
    flex: none;
    font-size: 11px;
    color: var(--mdx-text-faint);
    font-variant-numeric: tabular-nums;
  }

  .inbox-title {
    margin: 0;
    font-weight: 600;
    font-size: var(--mdx-text-sm);
  }

  .inbox-preview {
    margin: 0;
    font-size: var(--mdx-text-sm);
    color: var(--mdx-text-secondary, var(--mdx-text-muted));
    line-height: 1.45;
  }

  .inbox-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 3px;
  }

  @media (prefers-reduced-motion: reduce) {
    .presence-pulse { animation: none; }
  }

  .channels {
    display: grid;
    grid-template-rows: auto auto minmax(0, 1fr);
    gap: 8px;
    align-content: start;
    padding: 14px;
    border-right: 1px solid var(--mdx-border-subtle);
    min-height: 0;
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
    border-radius: var(--mdx-radius-lg);
  }

  .mobile-channels {
    display: none;
  }

  .channels-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .channels-head-actions {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }

  .add-channel {
    border: none;
    background: none;
    color: var(--mdx-text-muted);
    font-size: 15px;
    cursor: pointer;
    padding: 0 4px;
    display: inline-flex;
    align-items: center;
  }

  .add-channel:hover {
    color: var(--mdx-accent-primary);
  }

  .channel-input {
    padding: 7px 10px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font-family: var(--mdx-font-mono);
    font-size: 12px;
  }

  .channel-list {
    display: grid;
    gap: 1px;
    align-content: start;
    margin: 0;
    padding: 0;
    list-style: none;
    overflow-y: auto;
    min-height: 0;
  }

  .channel {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    padding: 7px 10px;
    border: none;
    border-radius: var(--mdx-radius-md);
    background: transparent;
    color: var(--mdx-text-muted);
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }

  .channel:hover,
  .channel.active {
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
  }

  .channel-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  .hash {
    color: var(--mdx-text-faint, var(--mdx-text-muted));
    margin-right: 3px;
  }

  .channel-unread {
    flex: none;
    min-width: 18px;
    padding: 1px 6px;
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-accent-primary);
    color: #fff;
    font-size: var(--mdx-text-xs);
    font-weight: 700;
    text-align: center;
  }

  @media (max-width: 860px) {
    .room-body {
      grid-template-columns: minmax(0, 1fr);
    }

    .mobile-channels {
      display: grid;
      gap: 6px;
      padding: 10px 12px;
      border: 1px solid var(--mdx-border-subtle);
      border-radius: var(--mdx-radius-lg);
      background: var(--mdx-surface-raised);
      color: var(--mdx-text-muted);
      font-size: var(--mdx-text-xs);
      font-weight: 600;
      letter-spacing: 0.04em;
      text-transform: uppercase;
    }

    .mobile-channels select {
      width: 100%;
      min-width: 0;
      padding: 8px 10px;
      border: 1px solid var(--mdx-border-default);
      border-radius: var(--mdx-radius-md);
      background: var(--mdx-surface-base);
      color: var(--mdx-text-primary);
      font: inherit;
      letter-spacing: 0;
      text-transform: none;
    }

    .channels {
      display: none;
    }

    .message-head {
      align-items: center;
      flex-wrap: wrap;
    }

    .head-action {
      order: 3;
    }

    .msg-actions,
    .msg-proof {
      opacity: 1;
    }

    .msg {
      grid-template-columns: 32px minmax(0, 1fr);
    }

    .msg-actions {
      grid-column: 2;
      position: static;
      margin-top: 6px;
      margin-left: 0;
      width: fit-content;
      max-width: 100%;
      justify-self: start;
    }

    .msg-proof {
      grid-column: 2;
      justify-self: start;
    }

    .reply {
      grid-template-columns: 24px minmax(0, 1fr);
    }
  }

  @media (hover: none) {
    .msg-actions,
    .msg-proof { opacity: 1; }
  }

  .timeline {
    overflow-y: auto;
    padding: 18px 4px;
    display: grid;
    gap: 14px;
    align-content: start;
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
  }

  .quiet {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-sm);
    text-align: center;
  }

  .welcome {
    max-width: 560px;
    margin: 10vh auto 0;
    text-align: center;
    display: grid;
    gap: 10px;
  }

  .welcome h2 {
    margin: 0;
    font-size: 28px;
  }

  .welcome p {
    margin: 0;
    color: var(--mdx-text-secondary);
    line-height: 1.55;
  }

  .welcome-sub {
    color: var(--mdx-text-tertiary) !important;
    font-size: var(--mdx-text-sm);
  }

  .unread-line {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 2px 0;
  }

  .unread-line::before,
  .unread-line::after {
    content: "";
    flex: 1;
    border-top: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 35%, transparent);
  }

  .unread-line span {
    color: var(--mdx-accent-primary);
    font-size: var(--mdx-text-xs);
  }

  .msg {
    position: relative;
    display: grid;
    grid-template-columns: 32px minmax(0, 1fr) auto;
    gap: 11px;
    align-items: start;
  }

  .msg-body {
    min-width: 0;
  }

  .avatar {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 50%;
    background: var(--mdx-surface-raised);
    border: 1px solid var(--mdx-border-subtle);
    color: var(--mdx-text-secondary);
    font-size: 13px;
    font-weight: 600;
  }

  .avatar[data-kind="agent"] {
    color: var(--mdx-accent-info);
  }

  .msg-who {
    margin: 0 0 2px;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    font-weight: 600;
  }

  .who-tag {
    margin-left: 7px;
    color: var(--mdx-text-tertiary);
    font-weight: 400;
    font-size: var(--mdx-text-xs);
  }

  .msg-time {
    margin-left: 8px;
    color: var(--mdx-text-faint);
    font-weight: 400;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    cursor: default;
  }

  .msg-text {
    margin: 0;
    color: var(--mdx-text-primary);
    line-height: 1.55;
    font-size: 14px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .msg-proof,
  .msg-actions {
    opacity: 0;
    transition: opacity 160ms ease-out;
  }

  /* The action pill floats above the row on hover - an elevated surface with
     real hit areas, not bare glyphs in the flow (walkthrough finding). */
  .msg-actions {
    position: absolute;
    top: -16px;
    right: 6px;
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 3px 5px;
    background: var(--mdx-surface-base);
    border: 1px solid var(--mdx-border-subtle);
    border-radius: 999px;
    box-shadow: var(--mdx-shadow-card);
    z-index: 2;
  }

  .msg-act {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border: none;
    border-radius: var(--mdx-radius-sm, 6px);
    background: transparent;
    color: var(--mdx-text-muted);
    cursor: pointer;
  }

  .msg-act:hover {
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
  }

  .msg-act-done {
    font-size: var(--mdx-text-xs);
    color: var(--mdx-accent-primary);
    text-decoration: none;
    align-self: center;
    white-space: nowrap;
  }

  .msg:hover .msg-proof,
  .msg:focus-within .msg-proof,
  .msg:hover .msg-actions,
  .msg:focus-within .msg-actions,
  .reply:hover .msg-proof,
  .reply:focus-within .msg-proof {
    opacity: 1;
  }

  .thread-open {
    margin-top: 6px;
    padding: 3px 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    background: transparent;
    color: var(--mdx-accent-primary);
    font-size: 12px;
    cursor: pointer;
  }

  .thread-open:hover {
    background: var(--mdx-surface-raised);
    border-color: var(--mdx-border-default);
  }

  .thread {
    margin-top: 10px;
    padding-left: 14px;
    border-left: 2px solid var(--mdx-border-subtle);
    display: grid;
    gap: 10px;
  }

  .reply {
    display: grid;
    grid-template-columns: 24px minmax(0, 1fr) auto;
    gap: 9px;
    align-items: start;
  }

  .reply-avatar {
    width: 24px;
    height: 24px;
    font-size: var(--mdx-text-xs);
  }

  .reply-input {
    display: flex;
    align-items: flex-end;
    gap: 8px;
    border: 1px solid var(--mdx-border-default);
    border-radius: 18px;
    padding: 6px 6px 6px 14px;
    background: var(--mdx-surface-raised);
    max-width: 460px;
  }

  .reply-input:focus-within {
    border-color: var(--mdx-accent-primary);
  }

  .reply-input textarea {
    flex: 1;
    border: none;
    background: transparent;
    color: var(--mdx-text-primary);
    font-family: inherit;
    font-size: 13.5px;
    line-height: 1.5;
    resize: none;
    outline: none;
    max-height: 100px;
  }

  .reply-send {
    width: 30px;
    height: 30px;
  }

  @media (prefers-reduced-motion: reduce) {
    .msg-proof,
    .msg-actions { transition: none; }
  }

  .jump-new {
    position: absolute;
    left: 50%;
    bottom: 132px;
    transform: translateX(-50%);
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-full);
    padding: 8px 16px;
    background: var(--mdx-surface-sunken);
    color: var(--mdx-text-primary);
    font-size: 13px;
    cursor: pointer;
  }

  .typing {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--mdx-text-tertiary);
    font-size: 12px;
    font-style: italic;
  }

  .dots {
    display: inline-flex;
    gap: 3px;
  }

  .dots span {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--mdx-text-tertiary);
    animation: bounce 1.2s ease-in-out infinite;
  }

  .dots span:nth-child(2) { animation-delay: 0.15s; }
  .dots span:nth-child(3) { animation-delay: 0.3s; }

  @keyframes bounce {
    0%, 60%, 100% { opacity: 0.35; transform: translateY(0); }
    30% { opacity: 1; transform: translateY(-2px); }
  }

  @media (prefers-reduced-motion: reduce) {
    .dots span { animation: none; }
  }

  .room-name {
    color: var(--mdx-text-primary);
    font-weight: 600;
  }

  .head-icon {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-tertiary);
    padding: 6px 8px;
    cursor: pointer;
    transition: color 0.15s ease, border-color 0.15s ease;
  }

  .head-icon:hover {
    color: var(--mdx-text-primary);
    border-color: var(--mdx-border-default);
  }

  .head-icon.active {
    color: var(--mdx-accent-primary);
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 45%, var(--mdx-border-subtle));
  }

  .head-icon:first-of-type {
    margin-left: auto;
  }

  .head-icon + .head-icon {
    margin-left: 6px;
  }

  /* Channel settings: rename, describe, archive - a governed object now. */
  .channel-settings {
    margin-bottom: 14px;
    padding: 16px;
    background: var(--mdx-surface-raised);
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    display: grid;
    gap: 12px;
    animation: mdx-page-enter 0.35s var(--mdx-ease-out, ease) both;
  }

  .cs-head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 12px;
  }

  .cs-eyebrow {
    display: block;
    font-size: var(--mdx-text-xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--mdx-accent-primary);
  }

  .cs-head h2 {
    margin: 2px 0 0;
    font-family: var(--mdx-font-display);
    font-size: 17px;
    font-weight: 600;
  }

  .cs-close {
    flex: none;
    border: none;
    background: none;
    color: var(--mdx-text-muted);
    font-size: 12.5px;
    cursor: pointer;
    padding: 4px 7px;
    border-radius: var(--mdx-radius-sm);
  }

  .cs-close:hover {
    color: var(--mdx-text-primary);
    background: color-mix(in srgb, var(--mdx-text-muted) 12%, transparent);
  }

  .cs-field {
    display: grid;
    gap: 5px;
  }

  .cs-field span {
    font-size: 12px;
    color: var(--mdx-text-muted);
  }

  .cs-field input {
    padding: 9px 12px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-primary);
    font-size: 14px;
  }

  .cs-field input:focus {
    outline: none;
    border-color: var(--mdx-accent-primary);
  }

  .cs-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-top: 2px;
  }

  .cs-archive {
    border: 1px solid var(--mdx-border-default);
    background: transparent;
    color: var(--mdx-accent-error, #e5658a);
    font-size: 13px;
    padding: 8px 14px;
    border-radius: var(--mdx-radius-md);
    cursor: pointer;
  }

  .cs-archive:hover {
    border-color: var(--mdx-accent-error, #e5658a);
  }

  .cs-save {
    border: none;
    background: var(--mdx-accent-primary);
    color: #fff;
    font-size: 13px;
    font-weight: 600;
    padding: 8px 18px;
    border-radius: var(--mdx-radius-md);
    cursor: pointer;
  }

  .cs-save:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .cs-note {
    margin: 0;
    font-size: 12px;
    color: var(--mdx-text-muted);
    line-height: 1.5;
  }

  .cs-visibility {
    display: flex;
    gap: 6px;
  }

  .vis-opt {
    flex: 1;
    padding: 8px 12px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-muted);
    font-size: 13px;
    cursor: pointer;
    transition: border-color 0.15s ease, color 0.15s ease;
  }

  .vis-opt.on {
    border-color: var(--mdx-accent-primary);
    color: var(--mdx-text-primary);
    background: color-mix(in srgb, var(--mdx-accent-primary) 10%, transparent);
  }

  .channel-lock {
    margin-right: 4px;
    color: var(--mdx-text-faint);
    vertical-align: -1px;
  }

  .cs-members {
    display: grid;
    gap: 8px;
    padding-top: 4px;
    border-top: 1px solid var(--mdx-border-subtle);
  }

  .cs-label {
    font-size: 12px;
    color: var(--mdx-text-muted);
  }

  .member-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 2px;
  }

  .member-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    border-radius: var(--mdx-radius-md);
  }

  .member-row:hover {
    background: var(--mdx-surface-base);
  }

  .member-avatar {
    flex: none;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-size: 11px;
    font-weight: 700;
    color: var(--mdx-text-primary);
    background: color-mix(in srgb, var(--mdx-accent-success) 20%, transparent);
  }

  .member-avatar[data-kind="agent"] {
    background: color-mix(in srgb, var(--mdx-accent-primary) 22%, transparent);
  }

  .member-name {
    flex: 1;
    min-width: 0;
    font-size: 13.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .member-role {
    flex: none;
    font-size: 11px;
    color: var(--mdx-text-muted);
    text-transform: capitalize;
  }

  .member-remove {
    flex: none;
    border: none;
    background: none;
    color: var(--mdx-text-muted);
    font-size: 12px;
    cursor: pointer;
    padding: 2px 6px;
    border-radius: var(--mdx-radius-sm);
  }

  .member-remove:hover {
    color: var(--mdx-accent-error);
  }

  .member-add {
    display: flex;
    gap: 6px;
    margin-top: 2px;
  }

  .member-add input {
    flex: 1;
    min-width: 0;
    padding: 8px 11px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-primary);
    font-size: 13px;
  }

  .member-add input:focus {
    outline: none;
    border-color: var(--mdx-accent-primary);
  }

  .member-add select {
    flex: none;
    padding: 8px 10px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-primary);
    font-size: 13px;
  }

  .member-add-btn {
    flex: none;
    border: none;
    background: var(--mdx-surface-raised);
    border: 1px solid var(--mdx-border-default);
    color: var(--mdx-text-primary);
    font-size: 13px;
    padding: 8px 14px;
    border-radius: var(--mdx-radius-md);
    cursor: pointer;
  }

  .member-add-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .slash-menu {
    margin: 0 16px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    display: grid;
    padding: 4px;
  }

  .slash-item {
    display: flex;
    gap: 10px;
    align-items: baseline;
    border: none;
    background: none;
    color: var(--mdx-text-primary);
    padding: 8px 10px;
    border-radius: var(--mdx-radius-md);
    cursor: pointer;
    text-align: left;
    font-size: var(--mdx-text-sm);
  }

  .slash-item.focused,
  .slash-item:hover {
    background: var(--mdx-surface-base, rgba(127, 127, 127, 0.08));
  }

  .slash-item span {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
  }

  .react-bar {
    display: none;
    gap: 2px;
  }

  .msg:hover .react-bar {
    display: inline-flex;
  }

  .react-bar.off {
    display: none !important;
  }

  .net-banner.refusal {
    color: var(--mdx-accent-danger, #d4183d);
  }

  .react-add {
    border: none;
    background: none;
    cursor: pointer;
    font-size: 13px;
    width: 26px;
    height: 26px;
    display: grid;
    place-items: center;
    border-radius: 8px;
    padding: 0;
    opacity: 0.75;
  }

  .react-add:hover {
    background: color-mix(in srgb, var(--mdx-accent-primary) 10%, transparent);
    opacity: 1;
  }

  .react-chips {
    display: flex;
    gap: 6px;
    margin-top: 4px;
  }

  .react-chip {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-raised);
    padding: 1px 8px;
    font-size: 12px;
    cursor: pointer;
    color: var(--mdx-text-primary);
  }

  .react-chip.mine {
    border-color: var(--mdx-accent-primary);
  }

  .mention-tag {
    color: var(--mdx-accent-primary);
    font-size: var(--mdx-text-xs);
    font-weight: 600;
  }

  .msg.mentions-me {
    border-left: 2px solid var(--mdx-accent-primary);
    padding-left: 8px;
  }

  .msg.flash {
    animation: hit-flash 1.6s ease-out;
  }

  @keyframes hit-flash {
    0% { background: color-mix(in oklab, var(--mdx-accent-primary) 18%, transparent); }
    100% { background: transparent; }
  }

  @media (prefers-reduced-motion: reduce) {
    .msg.flash { animation: none; }
  }

  .channel-name.muted {
    opacity: 0.55;
  }

  .channel-mention {
    background: var(--mdx-accent-primary);
    color: #fff;
    border-radius: var(--mdx-radius-pill);
    font-size: var(--mdx-text-xs);
    padding: 0 7px;
  }

  .run-fold {
    display: flex;
    align-items: center;
    gap: 8px;
    width: fit-content;
    margin: 2px 0;
    padding: 6px 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
    cursor: pointer;
  }

  .run-gear {
    opacity: 0.7;
  }

  .run-act {
    color: var(--mdx-text-muted);
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .run-open {
    border-left: 2px solid var(--mdx-border-subtle);
    padding-left: 10px;
    display: grid;
    gap: 2px;
  }

  .msg[data-lane="agent"] .avatar,
  .msg[data-lane="pipeline"] .avatar,
  .msg[data-lane="system"] .avatar {
    border-radius: var(--mdx-radius-md, 8px);
  }

  .decision-card {
    border: 1px solid var(--mdx-border-subtle);
    border-left: 3px solid var(--mdx-accent-primary);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    padding: 10px 14px;
    display: grid;
    gap: 4px;
  }

  .decision-eyebrow {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .decision-title {
    margin: 0;
    font-weight: 600;
    font-size: 14px;
  }

  .decision-detail {
    margin: 0;
    font-size: var(--mdx-text-sm);
    line-height: 1.5;
  }

  .decision-why {
    margin: 0;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-sm);
  }

  /* Typed Message events. A card is a compact, scannable agent post; an
     action card is the shell of a governed decision. Both reuse the
     surface-raised + accent-rail language of the activity feed so a real
     event reads like family with the rest of the room. */
  .event-card {
    display: flex;
    gap: 12px;
    padding: 10px 14px 11px;
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    border: 1px solid var(--mdx-border-subtle);
  }

  .event-rail {
    flex: none;
    width: 3px;
    border-radius: 3px;
    align-self: stretch;
    background: var(--mdx-text-faint);
  }

  .event-card[data-lane="agent"] .event-rail { background: var(--mdx-accent-primary); }
  .event-card[data-lane="system"] .event-rail { background: var(--mdx-accent-purple, var(--mdx-text-muted)); }
  .event-card[data-lane="worker"] .event-rail { background: var(--mdx-accent-purple, var(--mdx-text-muted)); }
  .event-card[data-lane="pipeline"] .event-rail { background: var(--mdx-accent-warning, var(--mdx-text-muted)); }
  .event-card[data-lane="human"] .event-rail { background: var(--mdx-accent-success); }

  .event-card-body {
    display: grid;
    gap: 3px;
    min-width: 0;
    flex: 1;
  }

  .event-eyebrow {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--mdx-text-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--mdx-text-muted);
  }

  .event-state {
    text-transform: none;
    letter-spacing: 0;
    font-size: 11px;
    padding: 1px 8px;
    border-radius: var(--mdx-radius-pill);
    background: color-mix(in srgb, var(--mdx-text-muted) 12%, transparent);
    color: var(--mdx-text-secondary, var(--mdx-text-muted));
  }

  .event-state[data-tone="review"] {
    background: color-mix(in srgb, var(--mdx-accent-primary) 16%, transparent);
    color: var(--mdx-accent-primary);
  }

  .event-state[data-tone="done"] {
    background: color-mix(in srgb, var(--mdx-accent-success) 16%, transparent);
    color: var(--mdx-accent-success);
  }

  .event-line {
    font-size: 14px;
    line-height: 1.5;
    color: var(--mdx-text-primary);
  }

  .event-summary {
    margin: 0;
    font-size: var(--mdx-text-sm);
    color: var(--mdx-text-secondary, var(--mdx-text-muted));
    line-height: 1.5;
  }

  /* The action card is the flagship moment: a governed decision a human
     makes inside the thread. It carries a touch more weight than a plain
     event card, and its verdicts have a clear hierarchy - one primary act,
     one quiet decline, two that ask for the human's words. */
  .action-card {
    border: 1px solid var(--mdx-border-subtle);
    border-left: 3px solid var(--mdx-accent-primary);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    padding: 12px 15px 13px;
    display: grid;
    gap: 5px;
  }

  .action-card[data-state="resolved"] {
    border-left-color: var(--mdx-accent-success);
    background: var(--mdx-surface-base, var(--mdx-surface-raised));
  }

  /* The answer reads lighter than the ask it resolves - a quieter echo, not
     a second proposal. */
  .action-card.action-result {
    padding: 9px 14px 10px;
    border-left-width: 2px;
    gap: 3px;
  }

  .action-eyebrow {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--mdx-text-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--mdx-text-muted);
  }

  .action-state {
    text-transform: none;
    letter-spacing: 0;
    font-size: 11px;
    padding: 1px 8px;
    border-radius: var(--mdx-radius-pill);
    background: color-mix(in srgb, var(--mdx-accent-primary) 16%, transparent);
    color: var(--mdx-accent-primary);
  }

  .action-state[data-tone="done"] {
    background: color-mix(in srgb, var(--mdx-accent-success) 18%, transparent);
    color: var(--mdx-accent-success);
  }

  .action-title {
    margin: 0;
    font-weight: 600;
    font-size: 14px;
    letter-spacing: -0.01em;
  }

  .action-summary {
    margin: 0;
    font-size: var(--mdx-text-sm);
    color: var(--mdx-text-secondary, var(--mdx-text-muted));
    line-height: 1.5;
  }

  .action-line {
    font-size: var(--mdx-text-sm);
    line-height: 1.5;
    color: var(--mdx-text-primary);
  }

  .action-verdicts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 4px;
  }

  .verdict {
    padding: 5px 14px;
    border-radius: var(--mdx-radius-pill);
    border: 1px solid var(--mdx-border-subtle);
    background: var(--mdx-surface-base, transparent);
    color: var(--mdx-text-secondary, var(--mdx-text-muted));
    font-size: var(--mdx-text-xs);
    font-weight: 600;
    cursor: pointer;
    transition: background 120ms ease, border-color 120ms ease;
  }

  .verdict.primary {
    border-color: transparent;
    background: var(--mdx-accent-primary);
    color: var(--mdx-on-accent, #fff);
  }

  .verdict.primary:hover:not(:disabled) {
    background: color-mix(in srgb, var(--mdx-accent-primary) 88%, #000);
  }

  .verdict.danger {
    color: var(--mdx-accent-danger, var(--mdx-text-secondary));
    border-color: color-mix(in srgb, var(--mdx-accent-danger, var(--mdx-text-muted)) 35%, var(--mdx-border-subtle));
  }

  .verdict.danger:hover:not(:disabled) {
    background: color-mix(in srgb, var(--mdx-accent-danger, var(--mdx-text-muted)) 10%, transparent);
  }

  .verdict.ghost {
    border-color: transparent;
    background: transparent;
    color: var(--mdx-text-muted);
    padding-inline: 10px;
  }

  .verdict.ghost:hover:not(:disabled) {
    color: var(--mdx-text-secondary, var(--mdx-text-primary));
    background: color-mix(in srgb, var(--mdx-text-muted) 8%, transparent);
  }

  .verdict:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .action-input {
    display: grid;
    gap: 6px;
    margin-top: 4px;
  }

  .action-input textarea {
    width: 100%;
    resize: vertical;
    border-radius: var(--mdx-radius-sm, 8px);
    border: 1px solid var(--mdx-border-subtle);
    background: var(--mdx-surface-base, transparent);
    color: var(--mdx-text-primary);
    padding: 8px 10px;
    font: inherit;
    font-size: var(--mdx-text-sm);
  }

  .action-input-row {
    display: flex;
    gap: 6px;
  }

  .action-note {
    margin: 3px 0 0;
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-muted);
  }

  .action-note.action-error {
    color: var(--mdx-accent-danger, var(--mdx-text-secondary));
  }

  .show-earlier {
    margin: 0 auto 8px;
    display: block;
    padding: 6px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
    cursor: pointer;
  }

  .show-earlier:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .avatar-spacer {
    flex: none;
    width: 32px;
  }

  .unread-jump {
    color: var(--mdx-accent-primary);
  }

  .msg.pending {
    opacity: 0.65;
  }

  .pending-note {
    margin: 2px 0 0;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .pending-retry {
    margin: 2px 0 0;
    padding: 0;
    border: none;
    background: none;
    color: var(--mdx-accent-danger, #d4183d);
    font-size: var(--mdx-text-xs);
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .net-banner {
    margin: 0;
    padding: 8px 16px;
    background: var(--mdx-surface-raised);
    border-top: 1px solid var(--mdx-border-subtle);
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-sm);
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .banner-retry {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    background: none;
    color: var(--mdx-text-primary);
    padding: 2px 10px;
    font-size: var(--mdx-text-xs);
    cursor: pointer;
  }

  .composer-area {
    display: grid;
    gap: 8px;
    padding-top: 12px;
  }

  .attachment-composer {
    display: grid;
    grid-template-columns: minmax(180px, 1.2fr) minmax(140px, 1fr) auto;
    gap: 8px;
    align-items: end;
    max-width: 720px;
    width: 100%;
    margin: 0 auto;
    padding: 10px 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
  }

  .attachment-composer label { display: grid; gap: 4px; }
  .attachment-composer label span { color: var(--mdx-text-muted); font-size: var(--mdx-text-xs); }
  .attachment-composer input { min-width: 0; padding: 7px 9px; border: 1px solid var(--mdx-border-default); border-radius: var(--mdx-radius-md); background: var(--mdx-surface-base); color: var(--mdx-text-primary); font: inherit; font-size: var(--mdx-text-sm); }
  .attachment-composer button { padding: 7px 10px; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-md); background: transparent; color: var(--mdx-text-secondary); cursor: pointer; }
  .attachment-error { grid-column: 1 / -1; margin: 0; color: var(--mdx-accent-warning); font-size: var(--mdx-text-xs); }

  .attach-btn {
    display: grid;
    place-items: center;
    flex: none;
    width: 30px;
    height: 30px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--mdx-text-muted);
    cursor: pointer;
  }

  .attach-btn:hover,
  .attach-btn.active { background: var(--mdx-surface-muted); color: var(--mdx-accent-primary); }

  .send-error {
    margin: 0;
    color: var(--mdx-accent-warning);
    font-size: var(--mdx-text-sm);
  }

  .input-wrapper {
    display: flex;
    align-items: flex-end;
    gap: 10px;
    max-width: 720px;
    width: 100%;
    margin: 0 auto;
    border: 1px solid var(--mdx-border-default);
    border-radius: 24px;
    padding: 11px 11px 11px 18px;
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-shadow-card);
    transition: border-color 140ms ease, box-shadow 140ms ease;
  }

  .input-wrapper:focus-within {
    border-color: var(--mdx-accent-primary);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--mdx-accent-primary) 18%, transparent), var(--mdx-shadow-card);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--mdx-accent-primary) 18%, transparent);
  }

  .input-wrapper textarea {
    flex: 1;
    border: none;
    background: transparent;
    color: var(--mdx-text-primary);
    font-family: inherit;
    font-size: 14.5px;
    line-height: 1.5;
    resize: none;
    outline: none;
    max-height: 140px;
  }

  .send-btn {
    width: 40px;
    height: 40px;
    border-radius: 50%;
    border: none;
    display: grid;
    place-items: center;
    background: var(--mdx-accent-primary);
    color: #fff;
    cursor: pointer;
    flex: none;
  }

  .send-btn:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .quiet-line {
    margin: 0 auto;
    max-width: 720px;
    display: flex;
    align-items: baseline;
    justify-content: center;
    gap: 6px;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
    line-height: 1.5;
  }

  .capture-chip {
    margin: 0 auto;
    max-width: 720px;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill, 999px);
    background: var(--mdx-surface-sunken);
    color: var(--mdx-text-secondary);
    font-size: 12px;
  }
  .capture-chip a {
    color: var(--mdx-accent-primary);
    text-decoration: none;
    font-weight: 600;
  }
  .capture-chip-dismiss {
    margin-left: auto;
    font: inherit;
    font-size: 12px;
    border: none;
    background: transparent;
    color: var(--mdx-text-tertiary);
    cursor: pointer;
  }
  .capture-chip-dismiss:hover {
    color: var(--mdx-text-primary);
  }

  .quiet-more {
    display: inline;
  }

  .quiet-more summary {
    display: inline;
    cursor: pointer;
    color: var(--mdx-text-muted);
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .quiet-more[open] summary {
    display: none;
  }

  .feed-repeat { margin-left: 8px; font-size: 11.5px; font-weight: 650; color: var(--mdx-text-muted); background: var(--mdx-surface-raised); border: 1px solid var(--mdx-border-subtle); border-radius: 999px; padding: 1px 7px; vertical-align: middle; }
  .feed-link { color: inherit; text-decoration: none; }
  .feed-link:hover { text-decoration: underline; }
  .action-links { margin: 2px 0 0; display: flex; gap: 14px; font-size: 12px; }
  .action-links a { color: var(--mdx-text-secondary); text-decoration: none; }
  .action-links a:hover { text-decoration: underline; color: var(--mdx-text-primary); }

  @media (max-width: 860px), (hover: none) {
    .msg-actions,
    .msg-proof { opacity: 1; }
  }

  @media (max-width: 860px) {
    .message-route :global(.catch-up) {
      position: absolute;
      z-index: 20;
      top: 72px;
      right: 12px;
      left: 12px;
      max-height: calc(100dvh - 150px);
      margin: 0;
      overflow: auto;
    }

    .attachment-composer { grid-template-columns: 1fr; }
    .msg-actions {
      position: static;
      margin-top: 6px;
      margin-left: 42px;
      width: fit-content;
    }
  }
</style>
