<script>
  import { onMount, tick } from "svelte";
  import { replaceState } from "$app/navigation";
  import { telemetry, answerVerdictEvent } from "../../lib/telemetry.js";
  import Provenance from "@mdx/ui/components/Provenance.svelte";
  import Markdown from "@mdx/ui/components/Markdown.svelte";
  import TwinProposalCard from "./TwinProposalCard.svelte";
  import TwinArtifactCard from "./TwinArtifactCard.svelte";
  import MarketplacePackPrompt from "../../lib/MarketplacePackPrompt.svelte";
  import {
    createTwinClient,
    draftsPath,
    projectionPath,
    conversationFromProjection,
    sessionsFromProjection,
    answerTurn,
    answerFirstPromptShape,
    streamAnswer
  } from "../../lib/twinSession.js";
  import { loadProfileWithServer, personaIdFrom, promptShapeFrom, personaContextFrom } from "../../lib/youProfile.js";
  import { loadPersonas, companionFrom, companionLayerFrom } from "../../lib/twinPersonas.js";
  import { companionColor } from "../../lib/twinCompanions.js";
  import { routePanel, autoCompanionId } from "../../lib/twinRouting.js";
  import { createMessageClient, sendPath } from "../../lib/messageSession.js";
  import { traceMode } from "../../lib/traceMode.js";
  import { kernelReachability } from "../../lib/kernelReachability.js";
  import {
    applyConversationPreference,
    activeDraftReceiptId,
    attachmentDisposition,
    copiedConversationContext,
    conversationMarkdown,
    emptyWorkspace,
    normalizeWorkspace,
    researchPreflightState,
    sortedVisibleSessions,
    workspaceContext,
    workspaceStorageKey
  } from "../../lib/twinWorkspace.js";

  let { data } = $props();
  // The shared honest signal: when the kernel is not answering, the composer
  // says so and holds the send instead of failing silently. The shell banner
  // carries Retry; here we just keep the composer from lying about liveness.
  const kernelOnline = $derived($kernelReachability.online);
  // Your custom companions (made on the You surface, this machine only)
  // stand next to the built-in five everywhere - the picker, the history,
  // the openers. Their layer travels with the question, never silently.
  let customPersonas = $state([]);
  const companions = $derived([
    ...data.companions,
    ...customPersonas.map(companionFrom)
  ]);

  // Runtime conversation state. Everything rendered here is either restored
  // from the kernel's projection or returned by a governed write with a
  // receipt; the welcome below is the contract-owned first viewport.
  let conversation = $state([]);
  let restoredCount = $state(0);
  let restoreLoaded = $state(false);
  // Conversations: the kernel's own session_id field gives us history
  // for free. The sidebar groups by session; switching re-reads the
  // cached projection; a new conversation just mints a fresh id.
  const SESSION_KEY = "mdx-twin-session";
  let sessions = $state([]);
  let activeSessionId = $state("");
  let projectionCache = $state(null);
  // Conversation list management, kept on this machine: a rename or a delete
  // is a local view choice over the kernel's append-only session record (the
  // receipts themselves never change). Search filters the list as you type.
  let sessionQuery = $state("");
  let showArchived = $state(false);
  let convoOverrides = $state({});
  let renamingId = $state("");
  let renameDraft = $state("");
  const CONVOS_KEY = "mdx-twin-convos";
  let workspace = $state({ ...emptyWorkspace });
  let workspaceOpen = $state(false);
  let workspaceFileDraft = $state("");
  let researchState = $state("idle");
  let actionsMenuId = $state("");
  let conversationNotice = $state("");
  let copiedTranscriptContext = $state("");
  let noticeTimer;
  let draft = $state("");
  // An attached document the person brings into the conversation for Twin to
  // reason over. Read as text on this machine, capped, and relayed with each
  // question as a prompt layer - it never uploads anywhere but the kernel.
  // Marked sensitive routes it to a sovereign model (and stays unanswered
  // until one is connected), so private material never reaches a frontier.
  let attachment = $state(null);
  let attachInput = $state(null);
  const DOC_CHAR_CAP = 16000;
  // Text we read directly, plus PDFs read locally through the parser rail.
  // Office formats, images, and scanned PDFs still need trusted readers. They
  // remain selectable so Twin can explain the safety gate, but are never read.
  const DOC_ACCEPT = ".pdf,.txt,.md,.markdown,.csv,.tsv,.json,.log,.yaml,.yml,.html,.xml,.js,.ts,.py,.rs,.go,.java,.rb,.sh,.sql,.docx,.xlsx,.pptx,image/*";

  function base64FromBuffer(buffer) {
    const bytes = new Uint8Array(buffer);
    let binary = "";
    const chunk = 0x8000;
    for (let i = 0; i < bytes.length; i += chunk) {
      binary += String.fromCharCode.apply(null, bytes.subarray(i, i + chunk));
    }
    return btoa(binary);
  }

  async function onAttachPick(event) {
    const file = event.target?.files?.[0];
    if (!file) return;
    const disposition = attachmentDisposition(file);
    if (disposition.startsWith("blocked_")) {
      sendError = disposition === "blocked_image"
        ? "Image and OCR reading is not live until the sandboxed reader passes its safety gate. Nothing was uploaded."
        : "I can't safely read Word, Excel, or PowerPoint files yet. Nothing was uploaded. Paste the text or attach a PDF instead.";
      sendState = "error";
      if (attachInput) attachInput.value = "";
      return;
    }
    if (disposition === "unsupported") {
      sendError = "Twin does not have a trusted reader for that file type. Nothing was uploaded.";
      sendState = "error";
      if (attachInput) attachInput.value = "";
      return;
    }
    const isPdf = disposition === "pdf";
    if (isPdf) {
      let base64 = "";
      try {
        base64 = base64FromBuffer(await file.arrayBuffer());
      } catch (error) {
        base64 = "";
      }
      attachment = { name: file.name, size: file.size, isPdf: true, base64, sensitive: false };
    } else {
      let text = "";
      try {
        text = await file.text();
      } catch (error) {
        text = "";
      }
      const truncated = text.length > DOC_CHAR_CAP;
      attachment = {
        name: file.name,
        size: file.size,
        text: truncated ? text.slice(0, DOC_CHAR_CAP) : text,
        truncated,
        sensitive: false
      };
    }
    // Let the same file be picked again after removal.
    if (attachInput) attachInput.value = "";
    composerEl?.focus();
  }

  function removeAttachment() {
    attachment = null;
    if (attachInput) attachInput.value = "";
  }

  function prettySize(bytes) {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  // The receipts/provenance/trust depth stays hidden by default so the chat
  // reads clean; the system-wide trust shield in the shell ($traceMode) reveals
  // it on demand.

  // The person's name comes from their You profile (set in "Make MDx yours");
  // a friendly neutral stands in until they've told Twin who they are.
  // A few casual, rotating openers so the empty state never feels stale, with a
  // time-of-day flavor. Picked client-side on mount (server render stays stable).
  let greetLine = $state("Hey there.");

  // The "How MDx works" overlay: Auto routes each question to the right one.
  let agentsOpen = $state(false);
  const AGENTS = [
    { name: "Advisor", line: "Org design, domain modeling, strategy" },
    { name: "Coach", line: "1:1s, feedback, difficult conversations" },
    { name: "Architect", line: "Tech strategy, build vs buy, architecture" },
    { name: "Problem Solver", line: "First-principles breakdown of complex problems" },
    { name: "Compliance", line: "Regulatory guidance - OSFI, HIPAA, policy" }
  ];
  let companionId = $state(autoCompanionId);
  let sendState = $state("idle");
  let sendError = $state("");
  let threadEl = $state(null);
  // Scroll pinning: auto-follow the stream only while the person is at
  // the bottom. A real wheel/touch gesture unpins; content growth never
  // does (the classic chat-scroll bug).
  let pinned = $state(true);
  let hasBelow = $state(false);
  let streamAbort = null;
  let answeringName = $state("");

  // The slash menu: "/" as the composer's first character opens a quiet
  // switcher - companions and quick actions, type-to-filter, keyboard
  // first. The same unified pattern leading products converged on.
  let slashIndex = $state(0);
  const slashOpen = $derived(draft.startsWith("/") && !draft.includes("\n"));
  const slashQuery = $derived(slashOpen ? draft.slice(1).toLowerCase() : "");
  const slashItems = $derived.by(() => {
    if (!slashOpen) return [];
    const entries = [
      { kind: "auto", id: autoCompanionId, name: "Auto", meta: "the right specialist" },
      ...companions.map((entry) => ({
        kind: "companion",
        id: entry.id,
        name: entry.name,
        meta: entry.custom ? "yours" : entry.stance
      })),
      { kind: "action", id: "new", name: "New conversation", meta: "fresh thread" }
    ];
    return entries.filter(
      (entry) =>
        !slashQuery ||
        `${entry.name} ${entry.meta}`.toLowerCase().includes(slashQuery)
    );
  });

  function chooseSlash(item) {
    if (!item) return;
    if (item.kind === "action") {
      newConversation();
    } else {
      companionId = item.id;
    }
    draft = "";
    slashIndex = 0;
    composerEl?.focus();
  }

  function checkBelow() {
    if (!threadEl) return;
    hasBelow = threadEl.scrollHeight - threadEl.scrollTop - threadEl.clientHeight > 60;
  }

  const client = $derived(createTwinClient({ session: data.session, catalogSlice: data.catalogSlice }));
  // Auto is the default: the right specialist answers each question
  // (v1's router, on this machine, instant). An explicit pick always
  // wins until they hand it back to Auto.
  const autoMode = $derived(companionId === autoCompanionId || companionId === "");
  const companion = $derived(companions.find((entry) => entry.id === companionId) ?? companions[0]);
  const empty = $derived(conversation.length === 0);
  // The "what's moving" panels stream in after first paint (deferred in the
  // server load) so the greeting and composer never wait on the ten-way
  // projection fan-out. Until it resolves, `moving` is null and the panels
  // show a light skeleton.
  let moving = $state(null);
  let movingPromise = null;

  async function loadMoving() {
    if (moving) return moving;
    if (!movingPromise) {
      movingPromise = fetch("/twin/moving")
        .then((response) => (response.ok ? response.json() : null))
        .then((packet) => packet ?? { acrossMdx: null, capabilityAwareness: null, activeLessons: [], briefing: null })
        .catch(() => ({ acrossMdx: null, capabilityAwareness: null, activeLessons: [], briefing: null }))
        .then((packet) => {
          moving = packet;
          return packet;
        });
    }
    return movingPromise;
  }

  async function ensureCompanyContext() {
    const packet = moving ?? (await loadMoving());
    return packet?.acrossMdx ?? { worldContext: "", signals: [] };
  }

  $effect(() => {
    let live = true;
    loadMoving().then((packet) => { if (live) moving = packet; });
    return () => { live = false; };
  });
  const hasWorkspaceMotion = $derived(
    sessions.length > 0 ||
    (moving?.acrossMdx?.signals?.length ?? 0) > 0 ||
    (moving?.capabilityAwareness?.installedCount ?? 0) > 0 ||
    (moving?.capabilityAwareness?.pendingCount ?? 0) > 0
  );

  // Who is on point, made visible: a quiet marker lands in the flow
  // whenever the answering companion changes - v1's switching card,
  // calmer. Reads only kind/name/id, so streaming body updates never
  // re-run this.
  const renderList = $derived.by(() => {
    const out = [];
    let lastName = "";
    let panelIds = null;
    for (const turn of conversation) {
      if (turn.kind === "you") {
        panelIds = null;
      }
      if (turn.kind === "panel") {
        panelIds = new Set(turn.ids ?? []);
      }
      if (turn.kind === "companion") {
        // Inside a panel the header already said who is on it - no
        // switch markers between the panelists.
        const inPanel = panelIds?.has(turn.companionId);
        if (lastName && turn.companionName !== lastName && !inPanel) {
          out.push({ kind: "switch", name: turn.companionName, companionId: turn.companionId ?? "" });
        }
        lastName = turn.companionName;
      }
      out.push(turn);
    }
    return out;
  });

  // Your profile (the You surface, this machine only): a preferred
  // companion wins over the contract default, and your style rides every
  // write through the contract's own persona fields - shown in each
  // answer's provenance, never injected silently.
  let profile = $state(null);

  let composerEl = $state(null);
  // Thumbs on an answer: verdict-only telemetry (never the text). A down
  // verdict also opens the report affordance prefilled, because "what was
  // wrong" is worth more than the count alone.
  let answerVerdicts = $state({});
  function recordAnswerVerdict(turn, index, helpful) {
    answerVerdicts = { ...answerVerdicts, [index]: helpful ? "up" : "down" };
    telemetry.record(answerVerdictEvent({ helpful, personaId: String(turn.personaId ?? "") }));
    if (!helpful) {
      // "That's not right" is a real correction, not just a count. Record it
      // as a governed learning signal the model can learn from, then open the
      // report affordance so the person can say what was wrong.
      recordAnswerCorrection(turn);
      window.dispatchEvent(
        new CustomEvent("mdx:report", { detail: { note: "A Twin answer missed the mark: " } })
      );
    }
  }
  // The one user-initiated implicit learning signal: a person marks a Twin
  // answer as not right. It cites the answer's own receipt so the correction is
  // traceable. Fire-and-forget and fail-soft - marking the answer never blocks
  // on the trail write, and no memory or behavior changes from it.
  async function recordAnswerCorrection(turn) {
    const answerReceiptId = turn?.answerReceiptId ?? "";
    if (!answerReceiptId) return;
    try {
      await fetch("/api/kernel/learning/implicit-signals.json", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          signal_kind: "twin_answer_corrected",
          source_receipt_id: answerReceiptId,
          work_item: turn.sessionId ?? activeSessionId,
          actor_id: personaIdFrom(profile) || "local_user"
        })
      });
    } catch (error) {
      // The person's correction still stands; only the trail write failed.
    }
  }

  // Handoff: carry an answer into the team room as a real governed write.
  const messageClient = $derived(createMessageClient(data.session));

  async function shareTurn(turn) {
    if (turn.handoff === "sending" || turn.handoff === "sent") {
      return;
    }
    turn.handoff = "sending";
    conversation = [...conversation];
    try {
      const intro = turn.question ? `On \u201c${turn.question.slice(0, 80)}\u201d - ` : "";
      await messageClient.write(
        sendPath,
        { body: `${intro}${turn.companionName} says: ${turn.body.slice(0, 240)}` },
        { receiptIntent: "message_thread_message" }
      );
      turn.handoff = "sent";
    } catch (error) {
      turn.handoff = "idle";
    }
    conversation = [...conversation];
  }

  // Auto is the default of defaults; the person's saved preference (the
  // You surface) wins over Auto in onMount. The contract default only
  // ever mattered when nothing else did - Auto now fills that role.

  async function scrollToEnd() {
    await tick();
    if (threadEl) {
      threadEl.scrollTop = threadEl.scrollHeight;
    }
  }

  function onThreadGesture() {
    if (!threadEl) return;
    const fromBottom = threadEl.scrollHeight - threadEl.scrollTop - threadEl.clientHeight;
    // Unpin the moment they scroll up; re-pin when they return near the
    // bottom (a generous threshold, never hair-trigger).
    pinned = fromBottom < 160;
    checkBelow();
  }

  function jumpToLatest() {
    pinned = true;
    hasBelow = false;
    scrollToEnd();
  }

  // After an answer lands, hand the keyboard back - but never steal
  // focus from something the person is doing elsewhere.
  function refocusComposer() {
    if (document.activeElement === document.body) {
      composerEl?.focus();
    }
  }

  function mintSessionId() {
    const stamp = crypto?.randomUUID ? crypto.randomUUID().slice(0, 8) : String(Math.floor(Math.random() * 1e8));
    return `sess_${stamp}`;
  }

  // Reflect the open conversation in the URL (/twin?c=<id>) so a refresh or a
  // second tab restores it and the link is shareable, instead of destroying
  // the URL back to a bare /twin after every send.
  function syncConvoUrl(id) {
    if (typeof window === "undefined") return;
    const url = new URL(window.location.href);
    url.search = "";
    if (id) url.searchParams.set("c", id);
    replaceState(url, {});
  }

  function rememberSession(id) {
    activeSessionId = id;
    syncConvoUrl(id);
    try {
      localStorage.setItem(SESSION_KEY, id);
    } catch (error) {
      // storage unavailable; the session lives for this tab only
    }
  }

  async function restore() {
    try {
      const packet = await client.read(projectionPath);
      projectionCache = packet;
      sessions = sessionsFromProjection(packet);
    } catch (error) {
      // No saved session, or the local kernel is not answering. The composer
      // stays usable; sending will report honestly if it fails.
    }
    // A shareable conversation link (/twin?c=<id>) reopens that thread on load.
    const wanted = typeof window !== "undefined" ? new URLSearchParams(window.location.search).get("c") : "";
    if (wanted && sessions.some((s) => s.id === wanted)) {
      restoreLoaded = true;
      await openSession(wanted);
      return;
    }
    // Otherwise open to home, the way leading assistants do: landing on Twin
    // shows the welcome - what's moving across MDx and the openers - on a fresh
    // thread. Past conversations wait in the sidebar, one click to pick up.
    activeSessionId = mintSessionId();
    conversation = [];
    restoredCount = 0;
    restoreLoaded = true;
  }

  function newConversation() {
    rememberSession(mintSessionId());
    conversation = [];
    restoredCount = 0;
    draft = "";
    copiedTranscriptContext = "";
    showArchived = false;
    composerEl?.focus();
  }

  // Conversation list: rename and delete are local view choices (the kernel's
  // session receipts are append-only and never change), persisted per machine.
  function hydrateConvoOverrides() {
    try {
      convoOverrides = JSON.parse(localStorage.getItem(CONVOS_KEY) || "{}");
    } catch (error) {
      convoOverrides = {};
    }
  }
  function persistConvoOverrides() {
    try {
      localStorage.setItem(CONVOS_KEY, JSON.stringify(convoOverrides));
    } catch (error) {
      // best effort
    }
  }
  function updateConvo(id, patch) {
    convoOverrides = applyConversationPreference(convoOverrides, id, patch);
    persistConvoOverrides();
  }
  function convoTitle(session) {
    return convoOverrides[session.id]?.title || session.title || "Conversation";
  }
  function startRename(session) {
    renamingId = session.id;
    renameDraft = convoTitle(session);
  }
  function commitRename() {
    if (renamingId) {
      const title = renameDraft.trim();
      updateConvo(renamingId, { title: title || undefined });
    }
    renamingId = "";
    renameDraft = "";
  }
  function deleteConvo(id) {
    updateConvo(id, { hidden: true });
    if (id === activeSessionId) {
      newConversation();
    }
  }
  function forkConvo(session) {
    const source = session.id === activeSessionId
      ? conversation
      : projectionCache ? conversationFromProjection(companions, projectionCache, session.id) : [];
    rememberSession(mintSessionId());
    conversation = source.map((turn) => ({ ...turn, restored: true }));
    restoredCount = conversation.filter((turn) => turn.kind === "companion").length;
    copiedTranscriptContext = copiedConversationContext(convoTitle(session), source);
    draft = "";
    showArchived = false;
    conversationNotice = "Transcript copied into a new conversation. Your next question will carry that context.";
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (conversationNotice = ""), 3200);
    composerEl?.focus();
  }
  function archiveConvo(id) {
    updateConvo(id, { archived: true });
    if (id === activeSessionId) newConversation();
  }
  function unarchiveConvo(id) {
    updateConvo(id, { archived: false });
  }
  function downloadConversation(session) {
    const turns = session.id === activeSessionId
      ? conversation
      : projectionCache ? conversationFromProjection(companions, projectionCache, session.id) : [];
    const blob = new Blob([conversationMarkdown(convoTitle(session), turns)], { type: "text/markdown" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `${convoTitle(session).replace(/[^a-z0-9]+/gi, "-").toLowerCase() || "twin-conversation"}.md`;
    link.click();
    URL.revokeObjectURL(url);
  }
  async function shareConversation(id) {
    const url = new URL(window.location.href);
    url.search = "";
    url.searchParams.set("c", id);
    await navigator.clipboard?.writeText(url.toString());
    conversationNotice = "Conversation link copied";
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (conversationNotice = ""), 2200);
  }
  function conversationAction(action, session) {
    actionsMenuId = "";
    if (action === "pin") updateConvo(session.id, { pinned: !session.pinned });
    else if (action === "rename") startRename(session);
    else if (action === "fork") forkConvo(session);
    else if (action === "share") shareConversation(session.id);
    else if (action === "export") downloadConversation(session);
    else if (action === "archive") archiveConvo(session.id);
    else if (action === "unarchive") unarchiveConvo(session.id);
    else if (action === "remove") deleteConvo(session.id);
  }
  function hydrateWorkspace() {
    try { workspace = normalizeWorkspace(JSON.parse(localStorage.getItem(workspaceStorageKey) || "{}")); }
    catch (error) { workspace = { ...emptyWorkspace }; }
  }
  function persistWorkspace() {
    workspace = normalizeWorkspace(workspace);
    try { localStorage.setItem(workspaceStorageKey, JSON.stringify(workspace)); } catch (error) { /* tab-only */ }
  }
  function addWorkspaceFile() {
    const file = workspaceFileDraft.trim();
    if (!file) return;
    workspace.files = [...workspace.files, file];
    workspaceFileDraft = "";
    persistWorkspace();
  }
  async function requestResearch() {
    if (researchState === "checking") return;
    researchState = "checking";
    try {
      const response = await fetch("/api/kernel/twin/connector-preflights.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          session_id: activeSessionId,
          item_id: "external_tools",
          source_draft_receipt_id: activeDraftReceiptId(
            conversation,
            projectionCache,
            activeSessionId
          )
        })
      });
      const packet = response.ok ? await response.json() : null;
      researchState = researchPreflightState(response.ok, packet);
    } catch (error) { researchState = "unavailable"; }
  }
  function focusOnMount(node) {
    node.focus();
    node.select?.();
  }

  // The list the sidebar renders: hidden ones dropped, titles overridden,
  // filtered by the search box.
  const archivedCount = $derived(
    sessions.filter((session) => convoOverrides[session.id]?.archived && !convoOverrides[session.id]?.hidden).length
  );
  const visibleSessions = $derived(
    sortedVisibleSessions(sessions, convoOverrides, sessionQuery, showArchived)
  );

  async function openSession(id) {
    rememberSession(id);
    // Re-read the projection so turns from this visit are included - the
    // page-load cache goes stale the moment you send.
    try {
      projectionCache = await client.read(projectionPath);
      sessions = sessionsFromProjection(projectionCache);
    } catch (error) {
      // kernel not answering; fall back to whatever we have
    }
    conversation = projectionCache
      ? conversationFromProjection(companions, projectionCache, id)
      : [];
    // Bring back any documents authored in this conversation - they live on
    // this machine, not in the kernel projection, so they need rebuilding.
    const docTurn = restoredDocTurn(id);
    if (docTurn) {
      conversation = [...conversation, docTurn];
    }
    restoredCount = conversation.filter((turn) => turn.kind === "companion").length;
    scrollToEnd();
  }

  function rememberNewSession(text) {
    if (!sessions.some((session) => session.id === activeSessionId)) {
      sessions = [
        { id: activeSessionId, title: text.length > 44 ? `${text.slice(0, 44)}…` : text, count: 1, lastOrder: Number.POSITIVE_INFINITY },
        ...sessions
      ];
    }
  }

  async function send() {
    // With a document attached, Twin reads it through the governed artifact
    // path: admitted privately, grounded against your company context, and
    // answered with an inline card and receipts - not relayed as a raw prompt
    // layer. A plain question (no file) goes to the companion as before.
    if (attachment?.text || attachment?.isPdf) {
      const note = draft.trim();
      draft = "";
      await readArtifact(attachment, note);
      return;
    }
    const text = draft.trim();
    draft = "";
    await sendText(text);
  }

  // The governed read: PDFs go through the local parser rail first (bytes and
  // text stored by reference, receipts carry refs/hashes not raw text), then
  // the same artifact-context read runs on the extracted text. Text files skip
  // straight to the read. Either way nothing reaches a frontier model and the
  // result renders as one inline card.
  async function readArtifact(file, note) {
    if (sendState === "sending") return;
    sendState = "sending";
    sendError = "";
    const intent = note || `Read ${file.name} and tell me what it means for us.`;
    conversation = [
      ...conversation,
      { kind: "you", body: intent },
      { kind: "artifact-loading", name: file.name, pdf: Boolean(file.isPdf) }
    ];
    const loading = conversation[conversation.length - 1];
    removeAttachment();
    pinned = false;
    await tick();
    if (threadEl) threadEl.scrollTop = threadEl.scrollHeight;

    const dropLoading = () => {
      conversation = conversation.filter((entry) => entry !== loading);
    };

    try {
      let artifactText = file.text ?? "";
      let mimeType = "text/plain";
      let parseMeta = null;

      if (file.isPdf) {
        const parsed = await fetch("/api/kernel/twin/artifacts/liteparse-parses.json", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            session_id: activeSessionId,
            artifact_name: file.name,
            artifact_pdf_base64: file.base64
          })
        });
        if (!parsed.ok) {
          sendError =
            parsed.status === 401
              ? "Let's set up your private session first, then I can read this."
              : "I could not read that PDF. It may be scanned or image-only - those readers are next. Nothing was sent anywhere.";
          sendState = "error";
          dropLoading();
          return;
        }
        const lp = await parsed.json();
        artifactText = lp.extracted_text ?? "";
        mimeType = "application/pdf";
        parseMeta = { pageCount: lp.page_count, parserVersion: lp.parser_version };
        if (!artifactText.trim()) {
          sendError =
            "That PDF looks scanned or image-only, so there was no text to read yet. Those readers are next.";
          sendState = "error";
          dropLoading();
          return;
        }
      }

      const response = await fetch("/api/kernel/twin/artifact-contexts.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          session_id: activeSessionId,
          artifact_name: file.name,
          mime_type: mimeType,
          artifact_text: artifactText
        })
      });
      if (!response.ok) {
        sendError =
          response.status === 401
            ? "Let's set up your private session first, then I can read this."
            : "I could not read that just now. Nothing was sent anywhere.";
        sendState = "error";
        dropLoading();
        return;
      }
      const report = await response.json();
      if (parseMeta) {
        report.page_count = parseMeta.pageCount;
        report.parser_version = parseMeta.parserVersion;
      }
      const index = conversation.indexOf(loading);
      const artifactTurn = { kind: "artifact", report, deleting: false, saveState: "idle", savedRoute: "/pages" };
      if (index >= 0) conversation[index] = artifactTurn;
      else conversation = [...conversation, artifactTurn];
      conversation = [...conversation];
      sendState = "idle";
    } catch (error) {
      sendError = "I could not reach your private workspace. Nothing was sent anywhere.";
      sendState = "error";
      dropLoading();
    }
    await tick();
    if (pinned && threadEl) threadEl.scrollTop = threadEl.scrollHeight;
    refocusComposer();
  }

  // B5: turn the governed read into a Pages draft via the artifact pages-draft
  // route. Contract per docs/TWIN-B5-PAGES-DRAFT-HANDOFF.md; field reads are
  // defensive so a small naming change on Codex's side is a one-line fix.
  async function saveArtifactToPages(turn) {
    if (turn.saveState === "saving" || turn.saveState === "saved") return;
    turn.saveState = "saving";
    conversation = [...conversation];
    try {
      const response = await fetch("/api/kernel/twin/artifacts/pages-drafts.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          session_id: activeSessionId,
          artifact_id: turn.report.artifact_id
        })
      });
      if (!response.ok) {
        turn.saveState = "error";
        conversation = [...conversation];
        return;
      }
      const result = await response.json();
      turn.savedRoute = result.pages_route || "/pages";
      turn.saveState = "saved";
    } catch (error) {
      turn.saveState = "error";
    }
    conversation = [...conversation];
  }

  // Office output: generate a Word doc / workbook / deck from the read, then
  // fetch its bytes and save the file. Outputs are stored by reference and
  // receipt-backed; no raw private content leaves and no frontier is touched.
  const officeExt = { docx: "docx", xlsx: "xlsx", pptx: "pptx" };
  function officeFileName(name, kind) {
    const base = String(name || "document").replace(/\.[^.]+$/, "");
    return `${base}.${officeExt[kind] ?? "docx"}`;
  }
  function saveBase64File(base64, contentType, filename) {
    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
    const blob = new Blob([bytes], { type: contentType || "application/octet-stream" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = filename;
    document.body.appendChild(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(url);
  }
  async function makeOffice(turn, kind) {
    turn.office = turn.office ?? {};
    if (turn.office[kind] === "busy") return;
    turn.office[kind] = "busy";
    conversation = [...conversation];
    try {
      const body = JSON.stringify({ session_id: activeSessionId, artifact_id: turn.report.artifact_id, artifact_kind: kind });
      const gen = await fetch("/api/kernel/twin/office/generated-artifacts.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body
      });
      if (!gen.ok) throw new Error("generate failed");
      const dl = await fetch("/api/kernel/twin/office/downloads.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body
      });
      if (!dl.ok) throw new Error("download failed");
      const result = await dl.json();
      if (!result.output_base64) throw new Error("no bytes");
      saveBase64File(result.output_base64, result.content_type, officeFileName(turn.report.artifact_name, kind));
      turn.office[kind] = "done";
    } catch (error) {
      turn.office[kind] = "error";
    }
    conversation = [...conversation];
  }

  async function forgetArtifact(turn) {
    if (turn.deleting) return;
    turn.deleting = true;
    conversation = [...conversation];
    try {
      const response = await fetch("/api/kernel/twin/artifacts/deletions.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ artifact_id: turn.report.artifact_id })
      });
      if (response.ok) {
        const index = conversation.indexOf(turn);
        if (index >= 0) {
          conversation[index] = { kind: "companion", companionName: "Twin", body: "Removed. The file, the text I read, and the context I built from it are gone, with a receipt to prove it.", proposals: [], followUps: [] };
          conversation = [...conversation];
        }
        return;
      }
    } catch (error) {
      // fall through to clearing the flag
    }
    turn.deleting = false;
    conversation = [...conversation];
  }

  async function sendText(text) {
    if (!text || sendState === "sending") {
      return;
    }
    sendState = "sending";
    sendError = "";
    conversation = [...conversation, { kind: "you", body: text }];
    // The reading position leading products choose: your prompt rises to
    // the top and the answer grows beneath it - no chasing the bottom.
    pinned = false;
    await tick();
    threadEl
      ?.querySelectorAll(".message.user")
      ?.item(threadEl.querySelectorAll(".message.user").length - 1)
      ?.scrollIntoView({ block: "start", behavior: "auto" });

    // In Auto, the router picks who answers THIS question - and when a
    // question genuinely spans two domains, BOTH specialists take it,
    // concurrently, each with their own receipts. An explicit pick never
    // panels: you asked one person, one person answers.
    const targets = autoMode
      ? routePanel(text, customPersonas)
          .map((id) => companions.find((entry) => entry.id === id))
          .filter(Boolean)
      : [companion];
    answeringName = targets.map((entry) => entry.name).join(" + ");
    if (targets.length > 1) {
      // A session-presentation marker (like typing indicators): the two
      // answers themselves are the durable record.
      conversation = [...conversation, { kind: "panel", names: targets.map((entry) => entry.name), ids: targets.map((entry) => entry.id) }];
    }
    streamAbort = new AbortController();
    await Promise.all(targets.map((target) => answerWith(target, text, targets)));
    if (!sendError) copiedTranscriptContext = "";
    streamAbort = null;
    sendState = sendError ? "error" : "idle";
    await tick();
    checkBelow();
    refocusComposer();
  }

  async function answerWith(target, text, panel = [target]) {
    // The home digest is deliberately deferred for a fast first paint. The send
    // path performs one bounded refresh when needed so a fast first question can
    // never outrun company grounding.
    const companyContext = await ensureCompanyContext();
    const personaUsed = personaIdFrom(profile) || target.id;
    const custom = customPersonas.find((entry) => entry.id === target.id);
    // On a panel, each specialist knows who else is answering and stays
    // in their own lane - two perspectives, not two copies. The note
    // rides prompt_shape, so the panel instruction is part of the
    // recorded ask, never a hidden whisper.
    const others = panel.filter((entry) => entry.id !== target.id);
    const laneNote =
      others.length > 0
        ? `; ${others.map((entry) => entry.name).join(" and ")} ${others.length === 1 ? "is" : "are"} answering this too from their own specialty - take ONLY your lane (${target.shape}), keep it tight, and do not repeat what is theirs`
        : "";
    const body = {
      session_id: activeSessionId,
      companion_id: target.id,
      companion_stance: target.stance,
      persona_profile_id: personaUsed,
      prompt_shape: answerFirstPromptShape(promptShapeFrom(profile, target.shape), laneNote),
      persona_context: personaContextFrom(profile),
      ...(custom ? { companion_context: companionLayerFrom(custom) } : {}),
      // The cross-MDx snapshot rides with the question so an answer about
      // "what needs me" or "what's Forge doing" is grounded in real state.
      // Read from projections in the server load; prompt-only, never stored.
      ...((companyContext.worldContext || workspaceContext(workspace) || copiedTranscriptContext)
        ? {
            world_context: [
              companyContext.worldContext,
              workspaceContext(workspace),
              copiedTranscriptContext
                ? `Context the person copied from another Twin conversation:\n${copiedTranscriptContext}`
                : ""
            ].filter(Boolean).join("\n\n")
          }
        : {}),
      // An attached document rides as its own layer; its sensitivity sets the
      // routing tier so a private file only ever reaches a sovereign model.
      ...(attachment?.text
        ? {
            document_context: attachment.text,
            document_name: attachment.name,
            sensitivity_tier: attachment.sensitive ? "sensitive" : "internal"
          }
        : {}),
      draft_text: text
    };

    // Streaming first: tokens render as the model thinks. The serve layer
    // answers "fallback" when the live gates do not hold, and the send
    // drops to the ordinary governed POST - deterministic, fully labeled.
    const turn = {
      kind: "companion",
      streaming: true,
      stopped: false,
      companionId: target.id,
      companionName: target.name,
      stance: target.stance,
      body: "",
      draftReceiptId: "",
      answerReceiptId: "",
      model: "",
      trustedContextUsed: false,
      trustedContextSourceCount: 0,
      trustedContextSourceIds: "",
      trustedContextReceiptIds: "",
      trustedContextProjectionRoute: "/pages/context-sources/projection.json",
      personaId: personaUsed,
      promptLayers: [],
      recallScore: null,
      recallSourceCount: 0,
      recallReceiptId: "",
      recallSources: [],
      companySources: companyContext.worldContext ? companyContext.signals ?? [] : [],
      answerLatency: null,
      question: text,
      handoff: "",
      proposals: [],
      followUps: []
    };
    conversation = [...conversation, turn];
    // The reactive handle: $state wraps the pushed object in a proxy, so
    // mutations must go through the array's own reference - mutating the
    // raw closure object renders nothing (found in anger).
    const live = conversation[conversation.length - 1];
    // Token batching: deltas accumulate in a plain buffer and land in
    // reactive state once per animation frame - typing stays smooth at
    // any token rate.
    let pendingText = "";
    let frame = 0;
    const flush = () => {
      frame = 0;
      if (!pendingText) return;
      live.body += pendingText;
      pendingText = "";
      tick().then(() => {
        if (pinned && threadEl) threadEl.scrollTop = threadEl.scrollHeight;
        checkBelow();
      });
    };
    const answerStartedAt = performance.now();
    let firstTokenAt = 0;
    const streamed = await streamAnswer({
      body,
      signal: streamAbort?.signal,
      onDelta: (delta) => {
        if (!firstTokenAt && delta) firstTokenAt = performance.now();
        pendingText += delta;
        if (!frame) frame = requestAnimationFrame(flush);
      },
      onProposal: (tool, args, proposalId) => {
        if (tool === "suggest_follow_ups") {
          // Presentation only: tappable next questions, nothing saved.
          live.followUps = (Array.isArray(args.questions) ? args.questions : [])
            .filter((question) => typeof question === "string" && question.trim())
            .slice(0, 3);
          return;
        }
        // The model proposed a skill. Nothing happens unless the person
        // approves - and then it runs through the same governed write
        // rails as doing it by hand.
        const entry = { tool, args, proposalId, sessionId: activeSessionId, state: "offered", receiptId: "" };
        live.proposals = [...live.proposals, entry];
        // Keep an authored document on this machine so it survives navigating
        // away and back, even before it is saved to Pages.
        if (tool === "draft_document") {
          rememberDoc(entry);
        }
      },
      onDone: (event) => {
        if (frame) cancelAnimationFrame(frame);
        live.body += pendingText;
        pendingText = "";
        live.streaming = false;
        live.draftReceiptId = event.draft_receipt_id ?? "";
        live.answerReceiptId = event.answer_receipt_id ?? "";
        live.model = event.model_gateway_model_id ?? "";
        live.trustedContextUsed = event.trusted_context_used === true || event.trusted_context_used === "true";
        live.trustedContextSourceCount = Number(event.trusted_context_source_count ?? 0);
        live.trustedContextSourceIds = event.trusted_context_source_ids ?? "";
        live.trustedContextReceiptIds = event.trusted_context_receipt_ids ?? "";
        live.trustedContextProjectionRoute = event.trusted_context_projection_route ?? "/pages/context-sources/projection.json";
        live.voiceDrift = Number(event.guard_voice_drift ?? 0);
        live.promptLayers = Array.isArray(event.prompt_layers) ? event.prompt_layers : [];
        // The memory that actually shaped this answer, straight from the
        // recall receipt the save just wrote - real values, not constants.
        live.recallScore = Number(event.memory_relevance_score ?? 0);
        live.recallSourceCount = Number(event.brain_recall_source_count ?? 0);
        live.recallReceiptId = event.brain_recall_receipt_id ?? "";
        live.recallSources = Array.isArray(event.memory_recall_sources) ? event.memory_recall_sources : [];
        live.answerLatency = {
          firstTokenMs: Number(event.first_token_ms ?? (firstTokenAt ? firstTokenAt - answerStartedAt : 0)),
          totalMs: Number(event.duration_ms ?? performance.now() - answerStartedAt)
        };
        live.handoff = "idle";
        rememberNewSession(text);
      },
      onGuarded: (message) => {
        // Nothing was sent and nothing was saved - remove the empty
        // placeholder and say why, plainly.
        if (frame) cancelAnimationFrame(frame);
        conversation = conversation.filter((entry) => entry !== live);
        sendError = message;
      },
      onRedaction: (redactedText) => {
        if (frame) cancelAnimationFrame(frame);
        pendingText = "";
        live.body = redactedText;
      },
      onError: (message) => {
        if (frame) cancelAnimationFrame(frame);
        live.body += pendingText;
        pendingText = "";
        live.streaming = false;
        live.stopped = true;
        sendError = `${message[0].toUpperCase()}${message.slice(1)}.`;
      }
    });
    const aborted = streamAbort?.signal.aborted ?? false;
    if (streamed) {
      return;
    }
    if (aborted) {
      // The person hit stop: keep what streamed, say plainly that
      // nothing was saved.
      if (frame) cancelAnimationFrame(frame);
      live.body += pendingText;
      live.streaming = false;
      live.stopped = true;
      if (!live.body) {
        conversation = conversation.filter((entry) => entry !== live);
      }
      return;
    }
    // Live gates not held (or the stream never started): the ordinary
    // governed POST, deterministic and fully receipt-backed.
    conversation = conversation.filter((entry) => entry !== live);
    try {
      const packet = await client.write(draftsPath, body, { receiptIntent: "twin_session_draft" });
      rememberNewSession(text);
      // The POST response does not echo companion fields (the projection
      // does); for a fresh turn the admitted write's own values are the
      // truth - same rationale as personaId.
      conversation = [
        ...conversation,
        {
          ...answerTurn(companions, packet, false),
          companionId: target.id,
          companionName: target.name,
          stance: target.stance,
          personaId: personaUsed,
          question: text,
          handoff: "idle"
        }
      ];
    } catch (error) {
      sendError = "Not saved - nothing was sent. Try again.";
    }
  }

  function stopStreaming() {
    streamAbort?.abort();
  }

  async function copyTurn(turn) {
    try {
      await navigator.clipboard.writeText(turn.body);
      turn.copied = true;
      setTimeout(() => (turn.copied = false), 1400);
    } catch (error) {
      // Clipboard unavailable: the quiet button simply does nothing.
    }
  }

  // The prompt-layer tags from the done event, said like a person would:
  // pack tags show name and version, private layers show presence only.
  function layersLine(tags) {
    if (!Array.isArray(tags) || tags.length === 0) return "";
    return tags
      .map((tag) => {
        if (tag === "persona_context:yours") return "your preferences";
        if (tag === "companion_context:yours") return "your companion";
        const [id, version] = tag.split("@");
        const name = id.replace("companion:twin_", "").replace(/_/g, " ");
        return version ? `${name} v${version}` : name;
      })
      .join(", ");
  }

  function editQuestion(text) {
    draft = text;
    composerEl?.focus();
  }

  // The skill payload, composed in the open: the card shows exactly this
  // text before the person approves, and this is exactly what posts.
  function proposalBody(proposal) {
    if (proposal.tool === "record_decision") {
      const parts = [`Decision: ${proposal.args.title ?? ""}`, proposal.args.decision ?? ""];
      if (proposal.args.reasoning) {
        parts.push(`Why: ${proposal.args.reasoning}`);
      }
      return parts.filter(Boolean).join("\n");
    }
    if (proposal.tool === "draft_document") {
      return String(proposal.args.body ?? "");
    }
    return String(proposal.args.summary ?? "");
  }

  function docSlug(text) {
    return (
      String(text ?? "")
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "-")
        .replace(/^-+|-+$/g, "")
        .slice(0, 40) || "document"
    );
  }

  // Keep the draft, no governed write: a plain markdown file, built and
  // saved entirely on this machine.
  function downloadDocument(proposal) {
    const title = proposal.args.title ?? "Document";
    const text = `# ${title}\n\n${proposal.args.body ?? ""}\n`;
    const blob = new Blob([text], { type: "text/markdown" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `${docSlug(title)}.md`;
    document.body.appendChild(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(url);
    recordProposalEvent(proposal, "downloaded");
  }

  // An authored document is ephemeral client state - the model proposed it
  // this turn, and the kernel only records that a proposal happened, never
  // its text. So the doc itself is kept on this machine, per session, and
  // rebuilt as a card when you reopen that conversation. Without this, a
  // drafted one-pager vanishes the moment you navigate away. Capped per
  // session; this is local convenience state, not the company record (that
  // is Pages, once you save).
  const DOCS_KEY = "mdx-twin-docs";

  function loadDocStore() {
    try {
      return JSON.parse(localStorage.getItem(DOCS_KEY) || "{}");
    } catch (error) {
      return {};
    }
  }

  function rememberDoc(proposal) {
    try {
      const store = loadDocStore();
      const sid = proposal.sessionId || activeSessionId;
      const list = Array.isArray(store[sid]) ? store[sid] : [];
      const entry = {
        proposalId: proposal.proposalId,
        title: proposal.args?.title ?? "Untitled document",
        body: proposal.args?.body ?? "",
        state: proposal.state === "done" ? "done" : "offered",
        receiptId: proposal.receiptId || ""
      };
      const idx = list.findIndex((d) => d.proposalId === proposal.proposalId);
      if (idx >= 0) list[idx] = entry;
      else list.push(entry);
      store[sid] = list.slice(-12);
      localStorage.setItem(DOCS_KEY, JSON.stringify(store));
    } catch (error) {
      // best effort - the live card still works this session
    }
  }

  // Rebuild persisted documents for a session as a quiet trailing turn, so
  // reopening the conversation shows the cards again (saved ones with their
  // receipt, unsaved ones still savable or downloadable).
  function restoredDocTurn(sid) {
    const docs = (() => {
      const store = loadDocStore();
      return Array.isArray(store[sid]) ? store[sid] : [];
    })();
    if (!docs.length) return null;
    return {
      kind: "companion",
      restored: true,
      companionId,
      companionName: "Twin",
      body: "",
      proposals: docs.map((d) => ({
        tool: "draft_document",
        args: { title: d.title, body: d.body },
        proposalId: d.proposalId,
        sessionId: sid,
        state: d.state === "done" ? "done" : "offered",
        receiptId: d.receiptId || ""
      }))
    };
  }

  function proposalChannel(proposal) {
    return proposal.tool === "record_decision" ? "decisions" : "local-ops";
  }

  // The human side of a proposal's life lands in the ledger: approved,
  // declined, executed with its receipt, or failed. Sizes and ids only -
  // the text itself becomes company record ONLY through the approved
  // governed write. A ledger hiccup never blocks the person.
  async function recordProposalEvent(proposal, event, executedReceiptId = "") {
    if (!proposal.proposalId) return;
    try {
      await fetch("/api/kernel/twin/skill-proposal-events.json", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          event,
          proposal_id: proposal.proposalId,
          skill: proposal.tool,
          session_id: proposal.sessionId ?? activeSessionId,
          actor_id: personaIdFrom(profile) || "local_user",
          args_chars: proposalBody(proposal).length,
          executed_receipt_id: executedReceiptId
        })
      });
    } catch (error) {
      // The decision still happened; only the trail write failed.
    }
  }

  async function approveProposal(proposal) {
    if (proposal.state === "sending" || proposal.state === "done") return;
    proposal.state = "sending";
    recordProposalEvent(proposal, "approved");
    try {
      if (proposal.tool === "draft_document") {
        // Save to Pages as a DRAFT for review, not a direct publish. Twin
        // carries the same review semantics as the Pages editor: it saves an
        // edit draft (the body is stored) and asks for review, so the document
        // waits on a human in Pages before it becomes a published page.
        // Nothing Twin writes reaches the library without the approval gate.
        // A fresh document id per draft keeps the write-once body from
        // colliding, and ids must be lowercase [a-z0-9_-] to store.
        const sid =
          (proposal.proposalId || activeSessionId || "doc")
            .toLowerCase()
            .replace(/[^a-z0-9]/g, "")
            .slice(-10) || "doc";
        const slug = docSlug(proposal.args.title);
        const docId = `page_twin_${slug}_${sid}`;
        const draftId = `draft_twin_${sid}`;
        const draftResponse = await fetch("/api/kernel/pages/edit-drafts.json", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            document_id: docId,
            draft_id: draftId,
            title: proposal.args.title ?? "Untitled document",
            body_text: proposal.args.body ?? "",
            revision_id: `rev_twin_${sid}`
          })
        });
        if (!draftResponse.ok) throw new Error("pages draft failed");
        const draftPacket = await draftResponse.json();
        // Ask for review - the document now waits on a human in Pages.
        const reviewResponse = await fetch("/api/kernel/pages/approval-requests.json", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ document_id: docId, draft_id: draftId })
        });
        const reviewPacket = reviewResponse.ok ? await reviewResponse.json() : {};
        proposal.state = "done";
        proposal.reviewPending = true;
        proposal.receiptId =
          reviewPacket.approval_request_receipt_id ?? draftPacket.edit_draft_receipt_id ?? "";
        proposal.savedDocId = draftPacket.document_id ?? docId;
        rememberDoc(proposal);
        recordProposalEvent(proposal, "executed", proposal.receiptId);
        return;
      }
      const packet = await messageClient.write(
        sendPath,
        { body: proposalBody(proposal), channel_id: proposalChannel(proposal) },
        { receiptIntent: "message_thread_message" }
      );
      proposal.state = "done";
      proposal.receiptId = packet.message_receipt_id ?? "";
      recordProposalEvent(proposal, "executed", proposal.receiptId);
    } catch (error) {
      proposal.state = "error";
      recordProposalEvent(proposal, "failed");
    }
  }

  function declineProposal(turn, proposal) {
    recordProposalEvent(proposal, "declined");
    turn.proposals = turn.proposals.filter((entry) => entry !== proposal);
    // A dismissed document should not come back on reopen.
    if (proposal.tool === "draft_document") {
      forgetDoc(proposal);
    }
  }

  function forgetDoc(proposal) {
    try {
      const store = loadDocStore();
      const sid = proposal.sessionId || activeSessionId;
      if (Array.isArray(store[sid])) {
        store[sid] = store[sid].filter((d) => d.proposalId !== proposal.proposalId);
        localStorage.setItem(DOCS_KEY, JSON.stringify(store));
      }
    } catch (error) {
      // best effort
    }
  }

  function onComposerKeydown(event) {
    if (slashOpen) {
      if (event.key === "ArrowDown") {
        event.preventDefault();
        slashIndex = (slashIndex + 1) % Math.max(slashItems.length, 1);
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        slashIndex = (slashIndex - 1 + Math.max(slashItems.length, 1)) % Math.max(slashItems.length, 1);
        return;
      }
      if (event.key === "Enter") {
        event.preventDefault();
        chooseSlash(slashItems[Math.min(slashIndex, slashItems.length - 1)]);
        return;
      }
      if (event.key === "Escape") {
        draft = "";
        slashIndex = 0;
        return;
      }
    }
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      send();
    }
  }

  // Esc stops a streaming answer from anywhere on the page - unless the
  // slash menu is open, where Esc means "close the menu".
  function onWindowKeydown(event) {
    if (event.key === "Escape" && actionsMenuId) {
      actionsMenuId = "";
      return;
    }
    if (event.key === "Escape" && streamAbort && !slashOpen) {
      stopStreaming();
    }
  }

  onMount(async () => {
    profile = await loadProfileWithServer();
    // One calm, professional greeting - the name from the profile when it is
    // set, and a graceful no-name line when it is not. We render whatever the
    // profile says and never invent a name.
    const me = profile?.name?.trim();
    const hour = new Date().getHours();
    const tod = hour < 12 ? "Good morning" : hour < 18 ? "Good afternoon" : "Good evening";
    greetLine = me ? `${tod}, ${me}.` : `${tod}.`;
    // The Ask verb: a known companion id preselects that thinker; any other
    // text is what was handed over - prefill it so the visitor lands ready to send.
    const params = new URLSearchParams(window.location.search);
    const asked = params.get("ask");
    const autoSend = params.get("send") === "1";
    if (asked && companions.some((entry) => entry.id === asked)) {
      companionId = asked;
    } else if (asked) {
      draft = asked;
    }
    customPersonas = loadPersonas();
    hydrateConvoOverrides();
    hydrateWorkspace();
    if (
      profile.defaultCompanionId &&
      companions.some((entry) => entry.id === profile.defaultCompanionId)
    ) {
      companionId = profile.defaultCompanionId;
    }
    restore().then(async () => {
      if (!asked || !autoSend || companions.some((entry) => entry.id === asked)) {
        return;
      }
      const text = draft.trim();
      if (!text) return;
      draft = "";
      // Clear only the one-shot ask/send params; keep the conversation in the
      // URL so a refresh does not lose the thread and never auto-resends.
      const u = new URL(window.location.href);
      u.searchParams.delete("ask");
      u.searchParams.delete("send");
      replaceState(u, {});
      await sendText(text);
    });
  });
</script>

<svelte:head><title>Twin - MDx</title></svelte:head>

<svelte:window onkeydown={onWindowKeydown} onclick={() => (actionsMenuId = "")} />

<section class="front-door" class:workspace-open={workspaceOpen} data-route-state="ready">
  <header class="twin-head">
    <h1 class="twin-mark">Twin</h1>
    <div class="head-actions">
    <button type="button" class="how-it-works" onclick={() => (workspaceOpen = !workspaceOpen)} aria-expanded={workspaceOpen}>
      <span aria-hidden="true">◎</span>
      {workspace.name || "Set project"}
    </button>
    <button type="button" class="how-it-works" onclick={() => (agentsOpen = true)} aria-haspopup="dialog">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M12 16v-4"/><path d="M12 8h.01"/></svg>
      How MDx works
    </button>
    </div>
  </header>

  {#if workspaceOpen}
    <section class="workspace-panel" aria-label="Active project context">
      <div><strong>Project context</strong><span>Included explicitly with each question on this device.</span></div>
      <label>Name <input bind:value={workspace.name} onblur={persistWorkspace} placeholder="Launch, customer, or repo" /></label>
      <label>Instructions <textarea bind:value={workspace.instructions} onblur={persistWorkspace} rows="2" placeholder="What Twin should keep in mind"></textarea></label>
      <label>Scoped references <span class="workspace-file"><input bind:value={workspaceFileDraft} onkeydown={(event) => event.key === "Enter" && addWorkspaceFile()} placeholder="brief.md or a folder" /><button type="button" onclick={addWorkspaceFile}>Add</button></span></label>
      {#if workspace.files.length}<div class="workspace-files">{#each workspace.files as file}<button type="button" onclick={() => { workspace.files = workspace.files.filter((item) => item !== file); persistWorkspace(); }} aria-label={`Remove ${file}`}>{file} ×</button>{/each}</div>{/if}
    </section>
  {/if}

  {#if agentsOpen}
    <div class="agents-scrim" role="presentation" onclick={() => (agentsOpen = false)}></div>
    <div class="agents-modal" role="dialog" aria-modal="true" aria-label="How MDx works">
      <h2>How MDx works</h2>
      <p class="agents-lead">Ask in plain words - MDx routes your question to the right specialist.</p>
      <ul class="agents-list">
        {#each AGENTS as agent}
          <li>
            <strong>{agent.name}</strong>
            <span>{agent.line}</span>
          </li>
        {/each}
      </ul>
      <p class="agents-build">Build mode: I can help shape a Forge-ready build request - try "build me a new endpoint" or "add a feature that...".</p>
      <button type="button" class="mdx-btn primary agents-got" onclick={() => (agentsOpen = false)}>Got it</button>
    </div>
  {/if}

  <div class="door-body">
  {#if sessions.length > 0 || conversation.length > 0}
    <aside class="history" aria-label="Conversations">
      <button type="button" class="new-convo" onclick={newConversation}>New conversation</button>
      {#if archivedCount > 0 || showArchived}
        <button type="button" class="archive-toggle" onclick={() => (showArchived = !showArchived)}>
          {showArchived ? "Back to conversations" : `Archived (${archivedCount})`}
        </button>
      {/if}
      {#if sessions.length > 3}
        <div class="convo-search">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="11" cy="11" r="7"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
          <input type="search" placeholder="Search conversations" bind:value={sessionQuery} aria-label="Search conversations" />
        </div>
      {/if}
      <ul class="session-list">
        {#each visibleSessions as session}
          <li class="session-row">
            {#if renamingId === session.id}
              <input
                class="session-rename"
                bind:value={renameDraft}
                use:focusOnMount
                onkeydown={(event) => {
                  if (event.key === "Enter") commitRename();
                  else if (event.key === "Escape") { renamingId = ""; renameDraft = ""; }
                }}
                onblur={commitRename}
                aria-label="Rename conversation"
              />
            {:else}
              <button
                type="button"
                class="session"
                class:active={session.id === activeSessionId}
                onclick={() => openSession(session.id)}
              >
                <span class="session-title">{session.displayTitle}</span>
              </button>
              <span class="session-actions">
                <button type="button" class="session-menu-trigger" aria-label={`Actions for ${session.displayTitle}`} aria-haspopup="menu" aria-expanded={actionsMenuId === session.id} onclick={(event) => { event.stopPropagation(); actionsMenuId = actionsMenuId === session.id ? "" : session.id; }}>•••</button>
                {#if actionsMenuId === session.id}
                  <span class="session-menu" role="menu" aria-label={`Actions for ${session.displayTitle}`}>
                    <button type="button" role="menuitem" onclick={() => conversationAction("pin", session)}>{session.pinned ? "Unpin" : "Pin"}</button>
                    <button type="button" role="menuitem" onclick={() => conversationAction("rename", session)}>Rename</button>
                    <button type="button" role="menuitem" onclick={() => conversationAction("fork", session)}>Copy into new conversation</button>
                    <button type="button" role="menuitem" onclick={() => conversationAction("share", session)}>Copy link</button>
                    <button type="button" role="menuitem" onclick={() => conversationAction("export", session)}>Export Markdown</button>
                    {#if session.archived}
                      <button type="button" role="menuitem" onclick={() => conversationAction("unarchive", session)}>Unarchive</button>
                    {:else}
                      <button type="button" role="menuitem" onclick={() => conversationAction("archive", session)}>Archive on this device</button>
                    {/if}
                    <button type="button" role="menuitem" class="danger" onclick={() => conversationAction("remove", session)}>Remove on this device</button>
                  </span>
                {/if}
              </span>
            {/if}
          </li>
        {/each}
        {#if visibleSessions.length === 0}
          <li class="convo-empty">{sessionQuery ? "No conversations match that." : showArchived ? "No archived conversations." : "No conversations yet."}</li>
        {/if}
      </ul>
    </aside>
  {/if}
  <div class="door-main">
  <!-- The transcript is a live log region; the wheel/touch listeners only
       read scroll intent (unpin detection), they make nothing interactive. -->
  <div
    class="thread"
    role="log"
    bind:this={threadEl}
    aria-live="polite"
    onwheel={onThreadGesture}
    ontouchmove={onThreadGesture}
    onscroll={checkBelow}
  >
    {#if !restoreLoaded}
      <!-- Cold read: the surface introduces itself before hydration instead
           of a bare loading line - h1 and the greeting SSR immediately;
           the saved session streams in underneath. -->
      <div class="welcome" aria-busy="true">
        <h2 class="welcome-title">Opening Twin</h2>
        <p class="welcome-sub">Opening your saved session...</p>
      </div>
    {:else if empty}
      <!-- The contract-owned first viewport is the welcome: current signal,
           human judgment, safe next move, boundary, evidence. -->
      <div class="welcome">
        <p class="welcome-greet">{greetLine}</p>
        <h2 class="welcome-title">
          {hasWorkspaceMotion ? "What needs your attention?" : "What do you wanna "}
          {#if !hasWorkspaceMotion}<span class="welcome-hl">build</span> next?{/if}
        </h2>
        <p class="welcome-sub">
          {hasWorkspaceMotion
            ? "Your workspace is already moving. Ask about the current work, catch up on what changed, or hand Twin a new thing to reason through."
            : "It's me, Twin - you know, your helper to just get stuff done. I've got specialist agents ready to help across strategy, coaching, architecture, problem solving, compliance, and coding."}
        </p>
        <div class="welcome-chips">
          {#if profile?.name}
            <button type="button" class="welcome-chip welcome-chip-you" onclick={() => sendText("What do you know about how I like to work?")}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="8" r="4"/><path d="M4 21c0-4 4-6 8-6s8 2 8 6"/></svg>
              <span>What do you know about me?</span>
            </button>
          {/if}
          <button type="button" class="welcome-chip" onclick={() => sendText("Help me think something through")}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 18h6"/><path d="M10 22h4"/><path d="M12 2a7 7 0 0 0-4 12.7c.6.5 1 1.2 1 2h6c0-.8.4-1.5 1-2A7 7 0 0 0 12 2Z"/></svg>
            <span>Help me think something through</span>
          </button>
          <button type="button" class="welcome-chip" onclick={() => sendText("What needs me right now?")}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 8a6 6 0 0 0-12 0c0 7-3 9-3 9h18s-3-2-3-9"/><path d="M13.7 21a2 2 0 0 1-3.4 0"/></svg>
            <span>What needs me right now?</span>
          </button>
          <button type="button" class="welcome-chip" onclick={() => sendText("Get a stuck build moving")}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>
            <span>Get a stuck build moving</span>
          </button>
          <button type="button" class="welcome-chip" onclick={() => sendText("Catch me up on what's changed")}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 12a9 9 0 1 0 9-9 9 9 0 0 0-6.4 2.6L3 8"/><path d="M3 4v4h4"/></svg>
            <span>Catch me up on what's changed</span>
          </button>
        </div>
        {#if !moving}
          <!-- The panels stream in after first paint; a light skeleton holds
               the space so the greeting does not jump when they arrive. -->
          <div class="across-skeleton" aria-hidden="true">
            <span class="mdx-skeleton across-sk-head"></span>
            <span class="mdx-skeleton across-sk-row"></span>
            <span class="mdx-skeleton across-sk-row"></span>
          </div>
        {/if}
        {#if moving?.acrossMdx?.signals?.length}
          <!-- What's moving across the other surfaces, read from real
               projections. Each row hands Twin a question; the arrow opens
               the surface itself. Quiet by default - this is the "useful,
               not noise" the home is meant to feel like. -->
          <section class="across" aria-label="Across MDx">
            <p class="across-head">Here's what's moving</p>
            <ul class="across-list">
              {#each moving.acrossMdx.signals as signal}
                <li class="across-row">
                  <button type="button" class="across-ask" onclick={() => sendText(signal.ask)}>
                    <span class="across-app" class:attention={signal.attention}>{signal.app}</span>
                    <span class="across-line">{signal.line}</span>
                  </button>
                  <a class="across-open" href={signal.href} aria-label={`Open ${signal.app}`} title={`Open ${signal.app}`}>
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 18l6-6-6-6"/></svg>
                  </a>
                </li>
              {/each}
            </ul>
          </section>
        {/if}
        {#if moving?.capabilityAwareness && (moving.capabilityAwareness.installedCount > 0 || moving.capabilityAwareness.pendingCount > 0)}
          <!-- Capability awareness: Twin names what the team can lean on,
               read from real Marketplace state, so it suggests a capability
               that exists instead of inventing one. -->
          <section class="across" aria-label="What you can lean on">
            <p class="across-head">What you can lean on</p>
            <ul class="across-list">
              {#if moving.capabilityAwareness.installedCount > 0}
                <li class="across-row">
                  <button type="button" class="across-ask" onclick={() => sendText("What can my installed capabilities help me get done right now?")}>
                    <span class="across-app">Marketplace</span>
                    <span class="across-line">{moving.capabilityAwareness.installedCount} {moving.capabilityAwareness.installedCount === 1 ? "capability" : "capabilities"} ready to draw on{moving.capabilityAwareness.installed.length ? ` - ${moving.capabilityAwareness.installed.map((c) => c.name).slice(0, 3).join(", ")}` : ""}</span>
                  </button>
                  <a class="across-open" href="/marketplace" aria-label="Open Marketplace" title="Open Marketplace">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 18l6-6-6-6"/></svg>
                  </a>
                </li>
              {/if}
              {#if moving.capabilityAwareness.pendingCount > 0}
                <li class="across-row">
                  <button type="button" class="across-ask" onclick={() => sendText("Should I add the capability that is waiting for review?")}>
                    <span class="across-app attention">Marketplace</span>
                    <span class="across-line">{moving.capabilityAwareness.pendingCount} waiting for a look{moving.capabilityAwareness.pendingNames.length ? `: ${moving.capabilityAwareness.pendingNames.join(", ")}` : ""} - add one to do more</span>
                  </button>
                  <a class="across-open" href="/marketplace" aria-label="Open Marketplace" title="Open Marketplace">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 18l6-6-6-6"/></svg>
                  </a>
                </li>
              {/if}
            </ul>
          </section>
        {/if}
        <MarketplacePackPrompt app="twin" objectId={activeSessionId || "new-conversation"} task={draft} compact />
        {#if moving?.activeLessons?.length}
          <!-- Lessons a human promoted. Twin may cite these; it cannot change
               them, grant anything, or act on its own. The receipt is the
               proof, one tap away. -->
          <section class="across" aria-label="What I've learned to apply">
            <p class="across-head">What I've learned to apply</p>
            <ul class="lesson-cite-list">
              {#each moving.activeLessons.slice(0, 3) as lesson (lesson.receiptId)}
                <li class="lesson-cite">
                  <span class="lesson-cite-text">{lesson.summary}</span>
                  {#if lesson.owner}<span class="lesson-cite-owner">promoted by {lesson.owner}</span>{/if}
                </li>
              {/each}
            </ul>
            <p class="lesson-cite-note">Lessons a human promoted. I can cite these; I can't change them or act on my own.</p>
          </section>
        {/if}
        {#if $traceMode}
          <details class="viewport-facts">
            <summary>What Twin can do today, and what it never will</summary>
            <dl>
              <div><dt>Right now</dt><dd>{data.viewport.currentSignal}</dd></div>
              <div><dt>Human judgment</dt><dd>{data.viewport.judgment}</dd></div>
              <div><dt>Safe next move</dt><dd>{data.viewport.safeNext}</dd></div>
              <div><dt>Boundary</dt><dd>{data.viewport.boundary.join(" · ")}</dd></div>
              <div><dt>Evidence</dt><dd>{data.viewport.evidence.join(", ")}</dd></div>
            </dl>
          </details>
        {/if}
      </div>
    {:else}
      {#if restoredCount > 0 && $traceMode}
        <p class="restore-note">
          {restoredCount} earlier {restoredCount === 1 ? "answer" : "answers"} restored from this
          machine's saved session.
        </p>
      {/if}
      {#each renderList as turn, turnIndex}
        {#if turn.kind === "you"}
          <div class="you-row">
            <button
              type="button"
              class="msg-edit"
              onclick={() => editQuestion(turn.body)}
              aria-label="Edit and ask again"
              title="Edit and ask again"
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="13" height="13" aria-hidden="true">
                <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
              </svg>
            </button>
            <div class="message user">{turn.body}</div>
          </div>
        {:else if turn.kind === "artifact-loading"}
          <div class="artifact-turn artifact-loading">
            <span class="glyph-dot" aria-hidden="true"></span>
            <span>{turn.pdf ? "Reading PDF locally. No copy leaves your machine." : `Reading ${turn.name} here, on your machine. No copy leaves.`}</span>
          </div>
        {:else if turn.kind === "artifact"}
          <div class="artifact-turn">
            <TwinArtifactCard
              report={turn.report}
              deleting={turn.deleting}
              onForget={() => forgetArtifact(turn)}
              onSave={data.pagesDraftRouteReady ? () => saveArtifactToPages(turn) : undefined}
              saveState={turn.saveState ?? "idle"}
              savedRoute={turn.savedRoute ?? "/pages"}
              onMakeOffice={data.officeGenerateRouteReady ? (kind) => makeOffice(turn, kind) : undefined}
              officeState={turn.office ?? {}}
            />
          </div>
        {:else if turn.kind === "switch"}
          <div class="switch-marker" role="separator">
            <span class="companion-dot" style="background: {companionColor(turn.companionId)}" aria-hidden="true"></span>
            <span>{turn.name} picks this up</span>
          </div>
        {:else if turn.kind === "panel"}
          <div class="switch-marker" role="separator">
            {#each turn.ids as id}
              <span class="companion-dot" style="background: {companionColor(id)}" aria-hidden="true"></span>
            {/each}
            <span>{turn.names.join(" and ")} are both on this</span>
          </div>
        {:else}
          <div class="turn">
            <p class="turn-who">
              <span class="companion-dot" style="background: {companionColor(turn.companionId)}" aria-hidden="true"></span>
              {turn.companionName}
            </p>
            {#if turn.stubAnswer}
              <!-- No live model behind this turn: a quiet notice with the
                   path to connect one, never a companion pretending. -->
              <div class="message notice">
                <p class="notice-line">{turn.body}</p>
                <a class="notice-cta" href="/welcome">Connect a model →</a>
              </div>
            {:else if turn.body || turn.streaming}
              <div class="message assistant"><Markdown text={turn.body} streaming={turn.streaming} />{#if turn.streaming}<span class="stream-cursor" aria-hidden="true"></span>{/if}</div>
            {/if}
            {#if turn.stopped}
              <p class="turn-stopped">Stopped - this answer was not saved. Ask again to keep it.</p>
            {/if}
            {#if !turn.streaming && turn.trustedContextUsed}
              <p class="turn-context">
                checked {turn.trustedContextSourceCount}
                {turn.trustedContextSourceCount === 1 ? "trusted company source" : "trusted company sources"}
              </p>
            {/if}
            {#if !turn.streaming && (turn.followUps?.length ?? 0) > 0}
              <div class="follow-ups">
                {#each turn.followUps as question}
                  <button
                    type="button"
                    class="follow-up"
                    onclick={() => {
                      draft = question;
                      composerEl?.focus();
                    }}
                  >
                    {question}
                  </button>
                {/each}
              </div>
            {/if}
            {#each turn.proposals ?? [] as proposal}
              {#if proposal.tool === "draft_document"}
                <!-- An authored document: Twin drafted it, you decide what
                     happens to it - keep it (download) or save it to Pages,
                     which rides the same recorded write as doing it by hand. -->
                <div class="doc-card" class:done={proposal.state === "done"}>
                  <div class="doc-head">
                    <svg class="doc-glyph" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/><line x1="8" y1="13" x2="16" y2="13"/><line x1="8" y1="17" x2="13" y2="17"/></svg>
                    <span class="doc-title">{proposal.args.title ?? "Untitled document"}</span>
                  </div>
                  <div class="doc-body"><Markdown text={proposal.args.body ?? ""} /></div>
                  {#if proposal.state === "done"}
                    <div class="doc-saved">
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg>
                      <span>Saved as a draft for review - it's waiting on you in Pages before it publishes.</span>
                      <a class="doc-open" href="/pages">Review in Pages &rarr;</a>
                      {#if $traceMode && proposal.receiptId}<span class="doc-receipt">{proposal.receiptId}</span>{/if}
                    </div>
                  {:else}
                    <div class="doc-actions">
                      <button type="button" class="mdx-btn primary small" onclick={() => approveProposal(proposal)} disabled={proposal.state === "sending"}>
                        {proposal.state === "sending" ? "Saving..." : "Save to Pages"}
                      </button>
                      <button type="button" class="mdx-btn secondary small" onclick={() => downloadDocument(proposal)}>Download</button>
                      <button type="button" class="doc-dismiss" onclick={() => declineProposal(turn, proposal)}>Dismiss</button>
                    </div>
                    {#if proposal.state === "error"}
                      <p class="doc-error">That did not save. Your draft is still here - try again or download it.</p>
                    {/if}
                  {/if}
                </div>
              {:else}
                <TwinProposalCard
                  {proposal}
                  companionName={turn.companionName}
                  channel={proposalChannel(proposal)}
                  body={proposalBody(proposal)}
                  skillFact={data.skillFacts?.[proposal.tool]}
                  onApprove={() => approveProposal(proposal)}
                  onDecline={() => declineProposal(turn, proposal)}
                />
              {/if}
            {/each}
            {#if turn.streaming}
              <p class="turn-meta-live">thinking live...</p>
            {:else}
              <div class="turn-meta">
              {#if $traceMode}
                <Provenance
                  label="Details"
                  items={[
                    { label: "Your message", value: turn.draftReceiptId },
                    { label: "This answer", value: turn.answerReceiptId },
                    { label: "Answered by", value: turn.stubAnswer ? "" : turn.model },
                    { label: "Persona", value: turn.personaId?.startsWith("you_v1") ? turn.personaId : "" },
                    { label: "Saved context", value: turn.recallSourceCount > 0 ? `Used ${turn.recallSourceCount} ${turn.recallSourceCount === 1 ? "source" : "sources"}` : "" },
                    { label: "Recall receipt", value: turn.recallSourceCount > 0 ? turn.recallReceiptId : "" },
                    { label: "Trusted sources", value: turn.trustedContextSourceIds },
                    { label: "Trust receipts", value: turn.trustedContextReceiptIds },
                    { label: "Source review", value: turn.trustedContextProjectionRoute },
                    { label: "Voice check", value: turn.voiceDrift > 0 ? `${turn.voiceDrift} drifted ${turn.voiceDrift === 1 ? "phrase" : "phrases"}` : "" },
                    { label: "Composed from", value: layersLine(turn.promptLayers) }
                  ]}
                />
                {#if turn.recallSourceCount > 0 && !turn.stubAnswer}
                  <!-- Memory shaped this answer: say so only when it did,
                       and show which memories with one click to the receipt. -->
                  <details class="turn-recall">
                    <summary>used {turn.recallSourceCount} saved context {turn.recallSourceCount === 1 ? "source" : "sources"}</summary>
                    <div class="recall-detail">
                      {#if turn.recallSources?.length}
                        <ul>
                          {#each turn.recallSources as source (source.memory_id)}
                            <li>{source.snippet}</li>
                          {/each}
                        </ul>
                      {/if}
                      {#if turn.recallReceiptId}<span class="doc-receipt">{turn.recallReceiptId}</span>{/if}
                    </div>
                  </details>
                {/if}
              {/if}
              {#if (turn.companySources?.length ?? 0) > 0 || turn.trustedContextUsed || turn.recallSourceCount > 0}
                <details class="turn-sources">
                  <summary>view sources</summary>
                  <div class="source-groups">
                    {#if (turn.companySources?.length ?? 0) > 0}
                      <section>
                        <strong>Company right now</strong>
                        <ul>
                          {#each turn.companySources as source (source.key)}
                            <li><a href={source.href}>{source.app}</a>: {source.line}</li>
                          {/each}
                        </ul>
                      </section>
                    {/if}
                    {#if turn.recallSourceCount > 0}
                      <section>
                        <strong>Saved context</strong>
                        <p>Used {turn.recallSourceCount} saved context {turn.recallSourceCount === 1 ? "source" : "sources"}.</p>
                        {#if turn.recallSources?.length}
                          <ul>
                            {#each turn.recallSources as source (source.memory_id)}<li>{source.snippet}</li>{/each}
                          </ul>
                        {/if}
                      </section>
                    {/if}
                    {#if turn.trustedContextUsed}
                      <section>
                        <strong>Trusted Pages context</strong>
                        <p>{turn.trustedContextSourceCount} active {turn.trustedContextSourceCount === 1 ? "source" : "sources"}. <a href={turn.trustedContextProjectionRoute}>Review source decisions</a>.</p>
                      </section>
                    {/if}
                  </div>
                </details>
              {/if}
              {#if turn.answerLatency?.totalMs > 0}
                <span class="answer-latency" title={`First token in ${Math.round(turn.answerLatency.firstTokenMs)} ms; complete in ${Math.round(turn.answerLatency.totalMs)} ms`}>
                  answered in {(turn.answerLatency.totalMs / 1000).toFixed(1)}s
                </span>
              {/if}
              <button type="button" class="turn-act" onclick={() => copyTurn(turn)} aria-label="Copy this answer">
                {turn.copied ? "copied" : "copy"}
              </button>
              {#if turn.question && !turn.restored}
                <button
                  type="button"
                  class="turn-act"
                  onclick={() => sendText(turn.question)}
                  disabled={sendState === "sending"}
                  aria-label="Ask this again"
                >
                  retry
                </button>
              {/if}
              {#if turn.handoff === "sent"}
                <a class="handoff done" href="/message">sent to the team →</a>
              {:else if turn.handoff}
                <button type="button" class="handoff" onclick={() => shareTurn(turn)} disabled={turn.handoff === "sending"}>
                  {turn.handoff === "sending" ? "sharing…" : "share with the team"}
                </button>
              {/if}
              <!-- The road out: thinking becomes work - but only when the ask
                   reads like buildable work (walkthrough finding: the verb on a
                   memory question makes no sense). Same doctrine as
                   controlAllowed: no verb where it cannot mean it. -->
              {#if /\b(build|fix|add|refactor|create|implement|write|make|update|improve|migrate|rename|remove|clean|ship)\b/i.test(String(turn.question ?? ""))}
                <a class="handoff" href={`/forge?intent=${encodeURIComponent(String(turn.question ?? "").slice(0, 500))}`}>
                  build this in Forge →
                </a>
              {/if}
              <span class="verdict" class:decided={answerVerdicts[turnIndex]}>
                <button
                  type="button"
                  class="verdict-btn"
                  class:on={answerVerdicts[turnIndex] === "up"}
                  onclick={() => recordAnswerVerdict(turn, turnIndex, true)}
                  aria-label="This answer was helpful"
                  title="Helpful"
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" width="13" height="13" aria-hidden="true"><path d="M7 10v12"/><path d="M15 5.88 14 10h5.83a2 2 0 0 1 1.92 2.56l-2.33 8A2 2 0 0 1 17.5 22H4a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2h2.76a2 2 0 0 0 1.79-1.11L12 2a3.13 3.13 0 0 1 3 3.88Z"/></svg>
                </button>
                <button
                  type="button"
                  class="verdict-btn"
                  class:on={answerVerdicts[turnIndex] === "down"}
                  onclick={() => recordAnswerVerdict(turn, turnIndex, false)}
                  aria-label="This answer missed the mark"
                  title="Missed the mark"
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" width="13" height="13" aria-hidden="true"><path d="M17 14V2"/><path d="M9 18.12 10 14H4.17a2 2 0 0 1-1.92-2.56l2.33-8A2 2 0 0 1 6.5 2H20a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-2.76a2 2 0 0 0-1.79 1.11L12 22a3.13 3.13 0 0 1-3-3.88Z"/></svg>
                </button>
              </span>
              </div>
            {/if}
          </div>
        {/if}
      {/each}
      {#if sendState === "sending" && !conversation.some((turn) => turn.streaming)}
        <p class="restore-note">{answeringName || companion.name} is thinking...</p>
      {/if}
      {#if sendState === "sending"}
        <!-- Scroll room so the prompt can rise to the top while the
             answer grows beneath it; collapses below the viewport when
             the answer completes. -->
        <div class="stream-spacer" aria-hidden="true"></div>
      {/if}
    {/if}
  </div>

  {#if !pinned && hasBelow}
    <button type="button" class="jump-latest" onclick={jumpToLatest} aria-label="Jump to the latest">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="16" height="16" aria-hidden="true">
        <polyline points="6 9 12 15 18 9" />
      </svg>
    </button>
  {/if}

  <footer class="composer-area">
    {#if conversationNotice}<p class="conversation-notice" role="status">{conversationNotice}</p>{/if}
    <div class="composer-tools" aria-label="Twin tools">
      <button type="button" class:on={researchState !== "idle"} onclick={requestResearch} disabled={researchState === "checking"}>◉ Research</button>
      <span>{researchState === "checking" ? "Checking whether fresh web sources are available..." : researchState === "blocked" ? "Fresh web research is not available yet. The request was recorded." : researchState === "unavailable" ? "I couldn't check web research. Nothing was recorded." : researchState === "ready" ? "Fresh web sources are ready for your next question." : "Use fresh web sources when the connector is available."}</span>
    </div>
    {#if sendState === "error"}
      <p class="send-error" role="alert">{sendError}</p>
    {/if}
    {#if !kernelOnline}
      <p class="composer-offline" role="status">Twin can't reach the kernel right now, so it won't answer until it's back. Type if you like - it just won't send yet.</p>
    {/if}
    {#if slashOpen && slashItems.length > 0}
      <div class="slash-menu" role="listbox" aria-label="Quick switch">
        {#each slashItems as item, index}
          <button
            type="button"
            class="slash-item"
            class:active={index === Math.min(slashIndex, slashItems.length - 1)}
            role="option"
            aria-selected={index === Math.min(slashIndex, slashItems.length - 1)}
            onclick={() => chooseSlash(item)}
          >
            {#if item.kind === "companion"}
              <span class="companion-dot" style="background: {companionColor(item.id)}" aria-hidden="true"></span>
            {:else}
              <span class="slash-glyph" aria-hidden="true">{item.kind === "auto" ? "✦" : "+"}</span>
            {/if}
            <span class="slash-name">{item.name}</span>
            <span class="slash-meta">{item.meta}</span>
          </button>
        {/each}
      </div>
    {/if}
    {#if attachment}
      <div class="attach-chip" class:sensitive={attachment.sensitive}>
        <svg class="attach-doc" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/></svg>
        <span class="attach-name">{attachment.name}</span>
        <span class="attach-size">{prettySize(attachment.size)}{attachment.truncated ? " · trimmed" : ""}</span>
        <button
          type="button"
          class="attach-flag"
          class:on={attachment.sensitive}
          onclick={() => (attachment.sensitive = !attachment.sensitive)}
          aria-pressed={attachment.sensitive}
          title={attachment.sensitive ? "Sensitive: only a sovereign model reads this (not connected yet)" : "Mark sensitive to keep this off frontier models"}
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 2 4 5v6c0 5 3.4 8.6 8 11 4.6-2.4 8-6 8-11V5l-8-3Z"/></svg>
          <span>{attachment.sensitive ? "Sensitive" : "Mark sensitive"}</span>
        </button>
        <button type="button" class="attach-remove" onclick={removeAttachment} aria-label="Remove attachment" title="Remove">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
        </button>
      </div>
      {#if attachment.sensitive}
        <p class="attach-note">Sensitive files only go to a sovereign model running on your own infrastructure. None is connected yet, so this stays unanswered until one is - your data never touches a frontier model.</p>
      {/if}
    {/if}
    <div class="input-wrapper">
      <input
        type="file"
        accept={DOC_ACCEPT}
        bind:this={attachInput}
        onchange={onAttachPick}
        class="attach-input"
        aria-hidden="true"
        tabindex="-1"
      />
      <button type="button" class="attach-btn" onclick={() => attachInput?.click()} aria-label="Attach a document" title="Attach a document">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" width="17" height="17" aria-hidden="true"><path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"/></svg>
      </button>
      <textarea
        rows="1"
        placeholder={attachment ? "Ask about this document, or just hit send..." : autoMode ? "Ask MDx anything..." : `Ask ${companion?.name ?? "MDx"} anything...`}
        bind:this={composerEl}
        bind:value={draft}
        onkeydown={onComposerKeydown}
        aria-label="Message your twin"
      ></textarea>
      {#if sendState === "sending" && conversation.some((turn) => turn.streaming)}
        <button type="button" class="send-btn stop" onclick={stopStreaming} aria-label="Stop the answer">
          <svg viewBox="0 0 24 24" fill="currentColor" width="12" height="12" aria-hidden="true">
            <rect x="5" y="5" width="14" height="14" rx="2" />
          </svg>
        </button>
      {:else}
        <button
          type="button"
          class="send-btn"
          onclick={send}
          disabled={(draft.trim().length === 0 && !attachment) || sendState === "sending" || !kernelOnline}
          aria-label="Send"
          title={!kernelOnline ? "Waiting for the kernel to come back" : "Send"}
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="16" height="16" aria-hidden="true">
            <line x1="12" y1="19" x2="12" y2="5" />
            <polyline points="5 12 12 5 19 12" />
          </svg>
        </button>
      {/if}
    </div>
    {#if $traceMode}
      <div class="quiet-line">
        <span class="trace-detail">
          This conversation uses {data.session?.verified ? "this signed-in cloud workspace" : "MDx on this Mac"}, where questions and answers stay on record. Twin can call a connected model; tools, memory promotion, worker spawn, and production writes keep their own governed gates. Twin advises and remembers; it never acts. Work happens in Forge. <a href="/changelog">What shipped &rarr;</a>
        </span>
      </div>
    {/if}
  </footer>
  </div>
  </div>
</section>

<style>
  /* Chat anatomy carried from the taste reference (apps/mdx Twin, itself
     ported from v1 chat-inline.css): user bubble accent, assistant raised
     with a left border, 720px pill composer. */
  .door-body {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 0 18px;
    min-height: 0;
    flex: 1;
  }

  .door-body:has(.history) {
    grid-template-columns: 190px minmax(0, 1fr);
  }

  .door-main {
    display: grid;
    grid-template-rows: minmax(0, 1fr) auto;
    min-height: 0;
    min-width: 0;
  }

  .history {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px 0;
    border-right: 1px solid var(--mdx-border-subtle);
    min-height: 0;
  }

  .convo-search {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-right: 14px;
    padding: 6px 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
  }
  .convo-search svg {
    width: 13px;
    height: 13px;
    flex: none;
    color: var(--mdx-text-tertiary);
  }
  .convo-search input {
    flex: 1;
    min-width: 0;
    border: none;
    background: transparent;
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: 12px;
    outline: none;
  }

  .new-convo {
    margin-right: 14px;
    padding: 8px 12px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font-size: 12.5px;
    cursor: pointer;
    transition: border-color 160ms ease-out;
  }

  .new-convo:hover {
    border-color: var(--mdx-accent-primary);
  }

  .archive-toggle {
    margin-right: 14px;
    border: 0;
    background: transparent;
    color: var(--mdx-text-tertiary);
    font: inherit;
    font-size: var(--mdx-text-xs);
    text-align: left;
    cursor: pointer;
  }

  .archive-toggle:hover {
    color: var(--mdx-text-primary);
  }

  .session-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    margin: 0;
    padding: 0 14px 0 0;
    list-style: none;
    overflow-y: auto;
    overflow-x: hidden;
    min-height: 0;
  }

  .session-row {
    position: relative;
    display: flex;
    align-items: center;
  }

  .session {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    padding: 8px 10px;
    border: none;
    border-radius: var(--mdx-radius-md);
    background: transparent;
    color: var(--mdx-text-muted);
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }

  .session:hover,
  .session.active {
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
  }

  .session-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  /* One quiet action trigger replaces a row of tiny icon targets. */
  .session-actions {
    position: absolute;
    right: 4px;
    top: 50%;
    transform: translateY(-50%);
    display: none;
    width: 32px;
  }
  .session-row:hover .session-actions,
  .session-row:focus-within .session-actions {
    display: flex;
  }
  .session-menu-trigger {
    width: 32px;
    height: 26px;
    border: 0;
    border-radius: 7px;
    background: var(--mdx-surface-sunken);
    color: var(--mdx-text-tertiary);
    cursor: pointer;
    letter-spacing: 1px;
  }
  .session-menu {
    position: absolute;
    z-index: 12;
    top: 30px;
    right: 0;
    display: grid;
    width: 168px;
    padding: 5px;
    border: 1px solid var(--mdx-border-default);
    border-radius: 10px;
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-shadow-card);
  }
  .session-menu button {
    border: 0;
    border-radius: 6px;
    padding: 7px 9px;
    background: transparent;
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }
  .session-menu button:hover,
  .session-menu button:focus-visible { background: var(--mdx-surface-base); }
  .session-menu button.danger { color: var(--mdx-accent-warning); }

  .head-actions { display: flex; align-items: center; gap: 8px; }
  .workspace-panel {
    display: grid;
    grid-template-columns: minmax(150px, .8fr) minmax(150px, .8fr) minmax(240px, 1.4fr);
    gap: 12px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--mdx-border-subtle);
    background: color-mix(in srgb, var(--mdx-accent-primary) 4%, var(--mdx-surface-base));
  }
  .workspace-panel > div:first-child { display: flex; flex-direction: column; gap: 3px; }
  .workspace-panel strong { font-size: 13px; }
  .workspace-panel span, .workspace-panel label { color: var(--mdx-text-muted); font-size: 11.5px; }
  .workspace-panel label { display: grid; gap: 4px; }
  .workspace-panel input, .workspace-panel textarea {
    width: 100%; box-sizing: border-box; border: 1px solid var(--mdx-border-subtle);
    border-radius: 8px; padding: 7px 9px; background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary); font: inherit; resize: vertical;
  }
  .workspace-file { display: flex; gap: 5px; }
  .workspace-file button, .workspace-files button { border: 1px solid var(--mdx-border-subtle); border-radius: 7px; background: var(--mdx-surface-raised); color: var(--mdx-text-muted); cursor: pointer; }
  .workspace-files { grid-column: 2 / -1; display: flex; flex-wrap: wrap; gap: 5px; }
  .workspace-files button { padding: 4px 7px; }

  .session-rename {
    width: 100%;
    padding: 7px 10px;
    border: 1px solid var(--mdx-accent-primary);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: 12px;
    outline: none;
  }

  .convo-empty {
    padding: 10px;
    color: var(--mdx-text-tertiary);
    font-size: 12px;
  }

  @media (max-width: 860px) {
    .door-body:has(.history) {
      grid-template-columns: minmax(0, 1fr);
    }

    .history {
      display: none;
    }
  }

  .front-door {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    height: 100%;
    min-height: 480px;
    max-width: 1040px;
  }
  .front-door.workspace-open { grid-template-rows: auto auto minmax(0, 1fr); }

  .twin-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
    padding-bottom: 14px;
    border-bottom: 1px solid var(--mdx-border-subtle);
  }

  .handoff {
    border: none;
    background: none;
    padding: 0;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
    cursor: pointer;
  }

  .handoff:hover {
    color: var(--mdx-accent-primary);
  }

  .handoff.done {
    color: var(--mdx-accent-success);
    text-decoration: none;
  }

  .twin-mark {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: 15px;
    font-weight: 650;
    line-height: 1;
    color: var(--mdx-text-primary);
  }

  .how-it-works {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-full);
    background: transparent;
    color: var(--mdx-text-muted);
    font: inherit;
    font-size: 12.5px;
    padding: 5px 11px;
    cursor: pointer;
  }
  .how-it-works svg {
    width: 14px;
    height: 14px;
  }
  .how-it-works:hover {
    color: var(--mdx-text-primary);
    border-color: var(--mdx-border-default);
  }

  /* How MDx works overlay - the specialist companions, the v1 way. */
  .agents-scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    z-index: 40;
  }
  .agents-modal {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    z-index: 41;
    width: min(480px, calc(100vw - 32px));
    display: grid;
    gap: 12px;
    padding: 24px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-xl);
    background: var(--mdx-surface-overlay);
    box-shadow: var(--mdx-shadow-panel);
  }
  .agents-modal h2 {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: 19px;
    font-weight: 650;
  }
  .agents-lead {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: 13.5px;
    line-height: 1.5;
  }
  .agents-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 6px;
  }
  .agents-list li {
    display: grid;
    gap: 2px;
    padding: 11px 13px;
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
  }
  .agents-list strong {
    font-size: 13.5px;
  }
  .agents-list span {
    color: var(--mdx-text-muted);
    font-size: 12.5px;
  }
  .agents-build {
    margin: 2px 0 0;
    padding: 11px 13px;
    border: 1px dashed var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    line-height: 1.5;
  }
  .agents-got {
    justify-self: stretch;
    text-align: center;
  }

  .thread {
    overflow-y: auto;
    padding: 18px 4px;
    display: grid;
    gap: 14px;
    align-content: start;
  }

  .restore-note {
    margin: 0;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
    text-align: center;
  }

  .welcome {
    display: grid;
    gap: 16px;
    max-width: 720px;
    margin: 8vh auto 0;
    text-align: center;
  }

  .welcome-greet {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: 15px;
  }

  .welcome-title {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: clamp(28px, 3.4vw, 38px);
    font-weight: 650;
    line-height: 1.15;
    letter-spacing: -0.01em;
  }
  .welcome-hl {
    color: var(--mdx-accent-primary);
  }

  .welcome-sub {
    margin: -6px auto 0;
    max-width: 520px;
    color: var(--mdx-text-muted);
    font-size: 14.5px;
    line-height: 1.55;
  }

  .welcome-chips {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    text-align: left;
    margin-top: 4px;
  }

  .welcome-chip {
    display: flex;
    align-items: center;
    gap: 11px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    padding: 14px 16px;
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-secondary);
    font: inherit;
    font-size: 13.5px;
    cursor: pointer;
    transition: border-color var(--mdx-motion-fast) var(--mdx-ease), color var(--mdx-motion-fast) var(--mdx-ease);
  }
  .welcome-chip svg {
    width: 18px;
    height: 18px;
    flex: none;
    color: var(--mdx-text-muted);
  }
  .welcome-chip:hover {
    border-color: var(--mdx-border-strong);
    color: var(--mdx-text-primary);
  }
  .welcome-chip:hover svg {
    color: var(--mdx-accent-primary);
  }
  .welcome-chip-you {
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 35%, var(--mdx-border-subtle));
    background: color-mix(in srgb, var(--mdx-accent-primary) 7%, var(--mdx-surface-raised));
    color: var(--mdx-text-primary);
  }
  .welcome-chip-you svg { color: var(--mdx-accent-primary); }
  @media (max-width: 620px) {
    .welcome-chips {
      grid-template-columns: 1fr;
    }
  }

  /* Across MDx: the quiet awareness strip. Real signals from the other
     surfaces, each a question Twin can pick up, with an arrow to the
     surface. Calm by default - it should read like a glance, not a feed. */
  .across {
    text-align: left;
    margin-top: 6px;
  }
  .across-skeleton {
    display: grid;
    gap: 8px;
    margin-top: 10px;
  }
  .across-sk-head { height: 12px; width: 120px; border-radius: var(--mdx-radius-sm, 6px); }
  .across-sk-row { height: 44px; width: 100%; border-radius: var(--mdx-radius-md); }
  .across-head {
    margin: 0 0 8px;
    padding: 0 2px;
    color: var(--mdx-text-tertiary);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.02em;
  }
  .lesson-cite-list {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .lesson-cite {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 10px;
    padding: 8px 10px;
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
    font-size: 13px;
    color: var(--mdx-text-secondary);
  }
  .lesson-cite-owner {
    font-size: 11px;
    color: var(--mdx-text-tertiary);
  }
  .lesson-cite-note {
    margin: 8px 2px 0;
    font-size: 11.5px;
    color: var(--mdx-text-tertiary);
  }
  .across-list {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .across-row {
    display: flex;
    align-items: stretch;
    gap: 2px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    overflow: hidden;
    transition: border-color var(--mdx-motion-fast) var(--mdx-ease);
  }
  .across-row:hover {
    border-color: var(--mdx-border-strong);
  }
  .across-ask {
    display: flex;
    align-items: baseline;
    gap: 10px;
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    padding: 11px 4px 11px 14px;
    color: var(--mdx-text-secondary);
    font: inherit;
    font-size: 13.5px;
    text-align: left;
    cursor: pointer;
  }
  .across-app {
    position: relative;
    flex: none;
    color: var(--mdx-text-muted);
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.02em;
    min-width: 58px;
  }
  .across-app.attention::before {
    content: "";
    position: absolute;
    left: -9px;
    top: 50%;
    transform: translateY(-50%);
    width: 5px;
    height: 5px;
    border-radius: 999px;
    background: var(--mdx-accent-primary);
  }
  .across-line {
    min-width: 0;
    color: var(--mdx-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .across-ask:hover .across-line {
    color: var(--mdx-text-primary);
  }
  .across-open {
    display: flex;
    align-items: center;
    padding: 0 12px;
    color: var(--mdx-text-tertiary);
    text-decoration: none;
    border-left: 1px solid var(--mdx-border-subtle);
  }
  .across-open svg {
    width: 16px;
    height: 16px;
  }
  .across-open:hover {
    color: var(--mdx-accent-primary);
    background: var(--mdx-surface-base);
  }

  .viewport-facts {
    text-align: left;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    padding: 10px 14px;
    background: var(--mdx-surface-raised);
  }

  .viewport-facts summary {
    cursor: pointer;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
  }

  .viewport-facts dl {
    margin: 10px 0 0;
    display: grid;
    gap: 8px;
  }

  .viewport-facts dt {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .viewport-facts dd {
    margin: 2px 0 0;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
    line-height: 1.5;
    overflow-wrap: anywhere;
  }

  .message {
    max-width: 78%;
    padding: 12px 16px;
    border-radius: var(--mdx-radius-lg);
    line-height: 1.55;
    font-size: 14.5px;
    white-space: pre-wrap;
  }

  .artifact-turn {
    justify-self: start;
    width: 100%;
    max-width: 88%;
  }

  .artifact-loading {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-secondary);
    font-size: 13.5px;
  }
  .glyph-dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--mdx-accent-primary);
    animation: pulse 1.1s ease-in-out infinite;
  }
  @keyframes pulse { 0%, 100% { opacity: 0.35; } 50% { opacity: 1; } }
  @media (prefers-reduced-motion: reduce) { .glyph-dot { animation: none; } }

  .message.user {
    justify-self: end;
    background: var(--mdx-accent-primary);
    color: #fff;
    border-bottom-right-radius: var(--mdx-radius-sm);
  }

  .message.assistant {
    justify-self: start;
    background: var(--mdx-surface-raised);
    border-left: 3px solid var(--mdx-accent-primary);
    color: var(--mdx-text-primary);
  }

  /* The not-connected notice: visibly not a companion speaking. Dashed,
     muted, with one clear way forward. */
  .message.notice {
    justify-self: start;
    display: grid;
    gap: 8px;
    background: var(--mdx-surface-sunken);
    border: 1.5px dashed var(--mdx-border-default);
    color: var(--mdx-text-secondary);
  }

  .notice-line {
    margin: 0;
  }

  .notice-cta {
    color: var(--mdx-accent-primary);
    font-weight: 600;
    font-size: 13.5px;
    text-decoration: none;
  }

  .notice-cta:hover {
    text-decoration: underline;
  }

  .turn {
    display: grid;
    gap: 6px;
    justify-items: start;
  }

  /* A name, not a system label: body face, human case. */
  .turn-who {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: 12px;
    font-weight: 600;
  }

  .companion-dot {
    display: inline-block;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    margin-right: 6px;
    vertical-align: baseline;
  }

  .switch-marker {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    margin: 4px 0;
    color: var(--mdx-text-tertiary);
    font-size: 12px;
  }

  .switch-marker .companion-dot {
    margin-right: 0;
  }

  .turn-meta {
    margin: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }

  .turn-recall {
    display: inline-block;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
  }
  .turn-recall summary {
    cursor: pointer;
    list-style: none;
  }
  .turn-recall summary::-webkit-details-marker {
    display: none;
  }
  .turn-recall[open] summary,
  .turn-recall summary:hover {
    color: var(--mdx-text-secondary);
  }
  .recall-detail {
    margin-top: 4px;
    padding: 6px 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-sm);
    background: var(--mdx-surface-sunken);
    color: var(--mdx-text-secondary);
  }
  .recall-detail ul {
    margin: 0;
    padding-left: 16px;
  }
  .recall-detail .doc-receipt {
    margin-left: 0;
  }

  .turn-sources {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
  }
  .turn-sources > summary {
    cursor: pointer;
    list-style: none;
  }
  .turn-sources > summary::-webkit-details-marker { display: none; }
  .turn-sources > summary:hover,
  .turn-sources[open] > summary { color: var(--mdx-text-secondary); }
  .source-groups {
    display: grid;
    gap: 10px;
    width: min(520px, calc(100vw - 72px));
    margin-top: 6px;
    padding: 10px 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-sunken);
    color: var(--mdx-text-secondary);
  }
  .source-groups section,
  .source-groups p { margin: 0; }
  .source-groups ul { margin: 4px 0 0; padding-left: 18px; }
  .source-groups a { color: var(--mdx-text-secondary); }
  .answer-latency {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
  }

  .turn-context {
    margin: -2px 0 0;
    padding: 4px 8px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-sm);
    background: var(--mdx-surface-sunken);
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-xs);
    font-weight: 600;
  }

  .turn-act {
    padding: 1px 8px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    background: transparent;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
    cursor: pointer;
    opacity: 0;
    transition: opacity 160ms ease-out;
  }

  .turn:hover .turn-act,
  .turn:focus-within .turn-act {
    opacity: 1;
  }

  .turn-act:hover {
    color: var(--mdx-text-primary);
    border-color: var(--mdx-border-default);
  }

  .turn-act:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .you-row {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
  }

  .msg-edit {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--mdx-text-muted);
    cursor: pointer;
    opacity: 0;
    transition: opacity 160ms ease-out;
  }

  .you-row:hover .msg-edit,
  .you-row:focus-within .msg-edit {
    opacity: 1;
  }

  .msg-edit:hover {
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
  }

  @media (prefers-reduced-motion: reduce) {
    .turn-act,
    .msg-edit { transition: none; }
  }

  .stream-cursor {
    display: inline-block;
    width: 2px;
    height: 1.05em;
    margin-left: 2px;
    vertical-align: text-bottom;
    background: var(--mdx-accent-primary);
    animation: cursor-blink 1s step-end infinite;
  }

  @keyframes cursor-blink {
    0%, 50% { opacity: 1; }
    51%, 100% { opacity: 0; }
  }

  .turn-meta-live {
    margin: 0;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
    font-style: italic;
  }

  .turn-stopped {
    margin: 0;
    color: var(--mdx-accent-warning);
    font-size: var(--mdx-text-sm);
  }

  .follow-ups {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
  }

  .follow-up {
    padding: 6px 13px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-pill);
    background: transparent;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    text-align: left;
    cursor: pointer;
  }

  .follow-up:hover {
    border-color: var(--mdx-accent-primary);
    color: var(--mdx-text-primary);
  }

  /* The authored-document card: a drafted artifact, distinct from a chat
     answer, with the two things you can do with it. */
  .doc-card {
    margin-top: 10px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-lg);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
    background: var(--mdx-surface-raised);
    overflow: hidden;
  }
  .doc-card.done {
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 35%, transparent);
  }
  .doc-head {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--mdx-border-subtle);
  }
  .doc-glyph {
    width: 17px;
    height: 17px;
    flex: none;
    color: var(--mdx-accent-primary);
  }
  .doc-title {
    font-family: var(--mdx-font-display);
    font-size: 15px;
    font-weight: 600;
    color: var(--mdx-text-primary);
  }
  .doc-body {
    max-height: 280px;
    overflow: auto;
    padding: 6px 16px;
    color: var(--mdx-text-secondary);
    font-size: 13.5px;
    line-height: 1.55;
  }
  .doc-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 14px;
    border-top: 1px solid var(--mdx-border-subtle);
  }
  .doc-dismiss {
    margin-left: auto;
    border: none;
    background: transparent;
    color: var(--mdx-text-tertiary);
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
  }
  .doc-dismiss:hover {
    color: var(--mdx-text-primary);
  }
  .doc-saved {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 14px;
    border-top: 1px solid var(--mdx-border-subtle);
    color: var(--mdx-tone-ok-text, var(--mdx-accent-primary));
    font-size: 13px;
  }
  .doc-saved svg {
    width: 15px;
    height: 15px;
    flex: none;
  }
  .doc-open {
    color: var(--mdx-accent-primary);
    text-decoration: none;
    font-weight: 550;
  }
  .doc-receipt {
    margin-left: auto;
    color: var(--mdx-text-tertiary);
    font-size: 11px;
    font-family: var(--mdx-font-mono, monospace);
  }
  .doc-error {
    margin: 0;
    padding: 0 14px 12px;
    color: var(--mdx-accent-warning);
    font-size: 12.5px;
  }

  .stream-spacer {
    height: 50vh;
    flex: none;
  }

  .jump-latest {
    position: absolute;
    left: 50%;
    bottom: 150px;
    transform: translateX(-50%);
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border: 1px solid var(--mdx-border-default);
    border-radius: 50%;
    background: var(--mdx-surface-sunken);
    color: var(--mdx-text-primary);
    cursor: pointer;
  }

  .send-btn.stop {
    background: var(--mdx-surface-sunken);
    color: var(--mdx-text-primary);
    border: 1px solid var(--mdx-border-default);
  }

  @media (prefers-reduced-motion: reduce) {
    .stream-cursor { animation: none; }
  }

  .slash-menu {
    position: absolute;
    bottom: 100%;
    left: 50%;
    transform: translateX(-50%);
    width: min(440px, 90%);
    margin-bottom: 8px;
    display: grid;
    gap: 1px;
    padding: 6px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-lg, 12px);
    background: var(--mdx-surface-raised);
    box-shadow: 0 12px 32px rgb(0 0 0 / 0.18);
    z-index: 5;
  }

  .slash-item {
    display: grid;
    grid-template-columns: 18px minmax(0, auto) 1fr;
    align-items: baseline;
    gap: 8px;
    padding: 8px 10px;
    border: none;
    border-radius: var(--mdx-radius-md);
    background: transparent;
    color: var(--mdx-text-primary);
    font-size: 13.5px;
    text-align: left;
    cursor: pointer;
  }

  .slash-item.active,
  .slash-item:hover {
    background: var(--mdx-surface-sunken);
  }

  .slash-glyph {
    color: var(--mdx-text-muted);
    font-size: 12px;
    text-align: center;
  }

  .slash-name {
    font-weight: 600;
  }

  .slash-meta {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .composer-area {
    position: relative;
    display: grid;
    gap: 8px;
    padding-top: 12px;
  }
  .conversation-notice {
    position: absolute;
    right: 0;
    bottom: calc(100% + 8px);
    margin: 0;
    padding: 7px 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: 999px;
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-shadow-card);
    color: var(--mdx-text-muted);
    font-size: 12px;
  }
  .composer-tools { max-width: 720px; width: 100%; margin: 0 auto; display: flex; align-items: center; gap: 9px; color: var(--mdx-text-tertiary); font-size: 11.5px; }
  .composer-tools button { border: 1px solid var(--mdx-border-subtle); border-radius: 999px; padding: 5px 10px; background: transparent; color: var(--mdx-text-muted); cursor: pointer; }
  .composer-tools button.on { color: var(--mdx-accent-primary); border-color: color-mix(in srgb, var(--mdx-accent-primary) 40%, transparent); }

  .send-error {
    margin: 0;
    color: var(--mdx-accent-warning);
    font-size: var(--mdx-text-sm);
  }
  .composer-offline {
    margin: 0 0 2px;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-sm);
    line-height: 1.5;
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
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
    padding: 10px;
    background: var(--mdx-surface-raised);
  }

  .input-wrapper:focus-within {
    border-color: var(--mdx-accent-primary);
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

  /* Attach: a quiet paperclip that leads the composer, the file input
     hidden behind it. Bring a document in and Twin reasons over it. */
  .attach-input {
    display: none;
  }
  .attach-btn {
    width: 38px;
    height: 38px;
    border-radius: 50%;
    border: none;
    display: grid;
    place-items: center;
    background: transparent;
    color: var(--mdx-text-muted);
    cursor: pointer;
    flex: none;
    transition: color var(--mdx-motion-fast) var(--mdx-ease), background var(--mdx-motion-fast) var(--mdx-ease);
  }
  .attach-btn:hover {
    color: var(--mdx-accent-primary);
    background: var(--mdx-surface-base);
  }

  /* The attached-document chip sits just above the composer, with a flag
     to mark it sensitive and an x to drop it. */
  .attach-chip {
    display: flex;
    align-items: center;
    gap: 9px;
    max-width: 720px;
    width: 100%;
    margin: 0 auto;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    padding: 8px 10px;
    background: var(--mdx-surface-raised);
    font-size: 13px;
  }
  .attach-chip.sensitive {
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 38%, transparent);
    background: color-mix(in srgb, var(--mdx-accent-primary) 7%, transparent);
  }
  .attach-doc {
    width: 16px;
    height: 16px;
    flex: none;
    color: var(--mdx-text-muted);
  }
  .attach-name {
    color: var(--mdx-text-primary);
    font-weight: 550;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .attach-size {
    color: var(--mdx-text-tertiary);
    font-size: 11.5px;
    flex: none;
  }
  .attach-flag {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    margin-left: auto;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: 999px;
    padding: 4px 9px;
    background: transparent;
    color: var(--mdx-text-muted);
    font: inherit;
    font-size: 11.5px;
    cursor: pointer;
    flex: none;
  }
  .attach-flag svg {
    width: 12px;
    height: 12px;
  }
  .attach-flag:hover {
    color: var(--mdx-text-primary);
    border-color: var(--mdx-border-default);
  }
  .attach-flag.on {
    color: var(--mdx-accent-primary);
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 40%, transparent);
    background: color-mix(in srgb, var(--mdx-accent-primary) 10%, transparent);
  }
  .attach-remove {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--mdx-text-tertiary);
    cursor: pointer;
    flex: none;
  }
  .attach-remove svg {
    width: 14px;
    height: 14px;
  }
  .attach-remove:hover {
    color: var(--mdx-text-primary);
    background: var(--mdx-surface-base);
  }
  .attach-note {
    max-width: 720px;
    width: 100%;
    margin: 0 auto;
    color: var(--mdx-text-tertiary);
    font-size: 11.5px;
    line-height: 1.45;
    padding: 0 4px;
  }

  .quiet-line {
    margin: 0 auto;
    max-width: 720px;
    display: flex;
    align-items: baseline;
    justify-content: center;
    gap: 8px;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
    line-height: 1.5;
  }

  .trace-detail {
    color: var(--mdx-text-muted);
  }
  .trace-detail a {
    color: var(--mdx-text-muted);
  }

  @media (max-width: 860px) {
    .message {
      max-width: 92%;
    }
  }
  @media (max-width: 760px) {
    .workspace-panel { grid-template-columns: 1fr; }
    .workspace-files { grid-column: 1; }
    .composer-tools span { display: none; }
  }
  .verdict { display: inline-flex; gap: 2px; margin-left: 6px; opacity: 0.55; transition: opacity 120ms ease; }
  .verdict:hover, .verdict.decided { opacity: 1; }
  .verdict-btn { border: none; background: none; cursor: pointer; color: var(--mdx-text-muted); width: 24px; height: 24px; display: grid; place-items: center; border-radius: 7px; padding: 0; }
  .verdict-btn:hover { background: color-mix(in srgb, var(--mdx-accent-primary) 10%, transparent); color: var(--mdx-text-primary); }
  .verdict-btn.on { color: var(--mdx-accent-primary); }
</style>
