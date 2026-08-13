<script>
  import { onMount } from "svelte";
  import { createMdxClient, routeCatalogFromGenerated } from "@mdx/client";
  import Provenance from "@mdx/ui/components/Provenance.svelte";
  import FirstUseHint from "../../lib/FirstUseHint.svelte";
  import PagesConnections from "./PagesConnections.svelte";
  import PagesComments from "./PagesComments.svelte";
  import MarketplacePackPrompt from "../../lib/MarketplacePackPrompt.svelte";
  import {
    PAGES_DRAFT_INDEX_KEY,
    approvalCoversRecordedDraft,
    normalizeDraftIndex,
    upsertDraftIndex,
    removeDraftFromIndex
  } from "./pagesDrafts.js";
  import { decisionRows, decisionSummary } from "../../lib/pagesDecisions.js";
  import { connectorView, connectorItems } from "../../lib/pagesConnectors.js";
  import { pagesMarkdownBlocks } from "../../lib/pagesMarkdown.js";
  import {
    createApprovalClient,
    requestsProjectionPath,
    approvePath,
    rejectPath,
    normalizeRequest
  } from "../../lib/pagesApproval.js";

  let { data } = $props();

  // Exact-path reads go through the catalog-gated client; pattern routes
  // (/pages/{id}/body) and query routes (search.json?q=) are looked up with
  // placeholder awareness by the kernel proxy itself, which is the
  // enforcement point for everything on this page either way.
  const gated = $derived(createMdxClient({
    baseUrl: "/api/kernel",
    routeCatalog: routeCatalogFromGenerated(data.catalogSlice)
  }));
  const proxied = createMdxClient({ baseUrl: "/api/kernel" });
  const approvals = $derived(createApprovalClient(data.session));
  const contextWrites = $derived(createMdxClient({ baseUrl: "/api/kernel", session: data.session }));

  let pages = $state([]);
  let loaded = $state(false);

  // Approval state. The requests projection carries both the pending queue and
  // the per-document decision, so one read drives the "waiting on you" strip
  // and the small note beside each library entry. deciding tracks the in-flight
  // document so its buttons disable; outcomes holds the human result line + the
  // exact receipt id for quiet disclosure.
  let approvalRows = $state([]);
  let deciding = $state("");
  let outcomes = $state({});
  // The capture chip after a publish: quiet, dismissible, links to Memory.
  let captureChip = $state(null);
  let localDrafts = $state([]);

  const pendingRows = $derived(approvalRows.filter((row) => row.pending));
  let contextOverride = $state(null);
  let contextArtifactsOverride = $state(null);
  let contextFreshnessOverride = $state(null);
  let connectorHealthOverride = $state(null);
  let contextDeciding = $state("");
  let contextOutcomes = $state({});
  const contextPacket = $derived(contextOverride ?? data.contextSources ?? null);
  const contextArtifactsPacket = $derived(contextArtifactsOverride ?? data.contextArtifacts ?? null);
  const contextFreshnessPacket = $derived(contextFreshnessOverride ?? data.contextFreshness ?? null);
  const connectorHealthPacket = $derived(connectorHealthOverride ?? data.connectorHealth ?? null);
  const contextSources = $derived(Array.isArray(contextPacket?.sources) ? contextPacket.sources : []);
  const contextReviewSources = $derived(contextSources.filter((source) => source.review_queue_state === "needs_review"));
  const trustedContextSources = $derived(contextSources.filter((source) => source.review_queue_state === "trusted"));
  const contextArtifactCount = $derived(Number(contextArtifactsPacket?.artifact_count ?? 0));
  const contextStaleCount = $derived(Number(contextFreshnessPacket?.stale_source_count ?? 0));
  const connectorReadyCount = $derived(
    Array.isArray(connectorHealthPacket?.connectors)
      ? connectorHealthPacket.connectors.filter((connector) => String(connector.health ?? "").includes("READY")).length
      : 0
  );
  const resolvedSourceMime = $derived(
    sourceMode === "csv" ? "text/csv" : sourceMode === "pdf" ? "application/pdf" : sourceMime
  );
  let sourceComposerOpen = $state(false);
  let sourceMode = $state("text");
  let sourceTitle = $state("");
  let sourceText = $state("");
  let sourceRef = $state("");
  let sourceKind = $state("github");
  let sourceMime = $state("text/markdown");
  let sourceBusy = $state(false);
  let sourceNote = $state("");
  let sourceReceiptId = $state("");
  const decisionByDoc = $derived(
    new Map(approvalRows.filter((row) => row.outcome).map((row) => [row.documentId, row]))
  );
  function titleFor(documentId) {
    return pages.find((page) => page.id === documentId)?.title ?? humanToken(documentId);
  }
  // Decision outcomes in plain words, never a raw enum in the disclosure.
  function humanTerminal(state) {
    const map = {
      PUBLISHED: "published",
      RECORDED: "recorded",
      APPROVED: "approved",
      REFUSED: "not allowed",
      REJECTED: "turned down",
      HELD: "held for review"
    };
    return map[String(state ?? "").toUpperCase()] ?? String(state ?? "").toLowerCase().replace(/_/g, " ");
  }
  let selectedId = $state("");
  let detail = $state(null);
  let bodyBlocks = $state(null);
  let rawBody = $state("");
  let bodyProvenance = $state(null);
  let bodyState = $state("idle");
  let searchQuery = $state("");
  let searchResults = $state(null);
  // Search filters: trust-aware, joined from the lifecycle projection.
  let filterState = $state("any");
  let filterCitable = $state(false);
  let filterOwner = $state("");

  const filteredResults = $derived.by(() => {
    if (!searchResults) return null;
    return searchResults.filter((result) => {
      const life = lifecycleByDoc[result.document_id];
      if (filterState !== "any" && (life?.state ?? "published") !== filterState) return false;
      if (filterCitable && !life?.trust?.trusted_for_ai) return false;
      if (filterOwner.trim()) {
        const owner = (life?.trust?.owner ?? result.author_actor_id ?? "").toLowerCase();
        if (!owner.includes(filterOwner.trim().toLowerCase())) return false;
      }
      return true;
    });
  });

  function resultChips(result) {
    const life = lifecycleByDoc[result.document_id];
    const chips = [];
    chips.push(life?.state ?? "published");
    chips.push(life?.trust?.trusted_for_ai ? "available to agents" : "not for agents yet");
    if (life?.freshness?.stale) chips.push("stale");
    return chips;
  }
  let searchBusy = $state(false);
  let searchSeq = 0;
  let searchTimer = null;

  const selected = $derived(pages.find((page) => page.id === selectedId) ?? pages[0]);

  function humanToken(value) {
    return String(value ?? "")
      .replace(/[_-]+/g, " ")
      .replace(/\b\w/g, (ch) => ch.toUpperCase());
  }

  // Inline marks, safe by construction: escape everything first, then allow
  // exactly bold/italic/code/links back in. Raw asterisks were showing in
  // published pages (walkthrough finding).
  function proseInline(text) {
    const escaped = String(text)
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;");
    return escaped
      .replace(/`([^`]+)`/g, "<code>$1</code>")
      .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
      .replace(/(^|[\s(])\*([^*\s][^*]*)\*/g, "$1<em>$2</em>")
      .replace(/\[([^\]]+)\]\((https?:[^)\s]+)\)/g, '<a href="$2" rel="noopener noreferrer">$1</a>');
  }

  async function loadLibrary() {
    try {
      const packet = await gated.read("/pages.json");
      const raw = Array.isArray(packet.documents) ? packet.documents : Array.isArray(packet.pages) ? packet.pages : [];
      const visibilityTopic = { tenant_only: "Internal", public: "Public" };
      pages = raw.map((page) => ({
        id: page.document_id ?? "page",
        title: page.title ?? humanToken(page.document_id ?? "page"),
        category: humanToken(page.category ?? visibilityTopic[page.visibility] ?? page.visibility ?? "memory"),
        summary: page.summary ?? "",
        receiptIds: Array.isArray(page.source_receipt_ids) ? page.source_receipt_ids : []
      }));
      // Default to the first page only when nothing is open - a deep
      // link's selection survives the library load.
      selectedId = selectedId || (pages[0]?.id ?? "");
    } catch (error) {
      pages = [];
    }
    loaded = true;
  }

  async function loadApprovals() {
    try {
      const packet = await approvals.read(requestsProjectionPath);
      const raw = Array.isArray(packet.requests) ? packet.requests : [];
      approvalRows = raw.map(normalizeRequest).filter((row) => row.documentId);
    } catch (error) {
      approvalRows = [];
    }
  }

  async function loadContextSources() {
    try {
      contextOverride = await proxied.read("/pages/context-sources/projection.json");
    } catch (error) {
      contextOverride = null;
    }
    try {
      contextArtifactsOverride = await proxied.read("/pages/context-artifacts/projection.json");
    } catch (error) {
      contextArtifactsOverride = null;
    }
    try {
      contextFreshnessOverride = await proxied.read("/pages/context-freshness/projection.json");
    } catch (error) {
      contextFreshnessOverride = null;
    }
    try {
      connectorHealthOverride = await proxied.read("/connectors/health.json");
    } catch (error) {
      connectorHealthOverride = null;
    }
  }

  async function addContextSource() {
    if (sourceBusy || !sourceTitle.trim()) return;
    if (sourceMode === "link" && !sourceRef.trim()) return;
    if (sourceMode !== "link" && !sourceText.trim()) return;
    sourceBusy = true;
    sourceNote = "";
    sourceReceiptId = "";
    try {
      const payload = sourceMode === "link"
        ? {
            title: sourceTitle,
            source_kind: sourceKind,
            source_id: `${sourceKind}:manual`,
            external_ref: sourceRef,
            summary: "Added from Memory.",
            scope: "tenant"
          }
        : {
            title: sourceTitle,
            mime_type: resolvedSourceMime,
            artifact_text: sourceText,
            company_context_source: "Context:Add source"
          };
      const result = await contextWrites.write(
        "/pages/context-sources.json",
        payload,
        { receiptIntent: "pages_context_source_admission" }
      );
      if (result?.status === "REFUSED") {
        throw new Error(result?.reason ?? "refused");
      }
      sourceNote = "Added for review";
      sourceReceiptId = String(result?.source_admission_receipt_id ?? "");
      sourceTitle = "";
      sourceText = "";
      sourceRef = "";
      await loadContextSources();
    } catch (error) {
      sourceNote = error?.message ?? "Nothing was recorded. Try again.";
    }
    sourceBusy = false;
  }

  async function trustContextSource(source) {
    const sourceId = source?.source_id ?? source?.artifact_id;
    if (!sourceId || contextDeciding) return;
    contextDeciding = sourceId;
    const next = { ...contextOutcomes };
    delete next[sourceId];
    contextOutcomes = next;
    try {
      const result = await contextWrites.write(
        "/pages/context-sources/trust-decisions.json",
        {
          source_id: sourceId,
          decision_note: `Trusted ${source?.title ?? "source"} for governed AI use.`
        },
        { receiptIntent: "pages_context_source_trust_decision" }
      );
      if (result?.status === "REFUSED") {
        throw new Error(result?.reason ?? "refused");
      }
      contextOutcomes = {
        ...contextOutcomes,
        [sourceId]: {
          ok: true,
          line: "Trusted for AI",
          receiptId: String(result?.trust_decision_receipt_id ?? "")
        }
      };
      await loadContextSources();
    } catch (error) {
      contextOutcomes = {
        ...contextOutcomes,
        [sourceId]: {
          ok: false,
          line: "Nothing was recorded - the source is still private."
        }
      };
    }
    contextDeciding = "";
  }

  async function revokeContextSource(source) {
    const sourceId = source?.source_id ?? source?.artifact_id;
    if (!sourceId || contextDeciding) return;
    contextDeciding = sourceId;
    const next = { ...contextOutcomes };
    delete next[sourceId];
    contextOutcomes = next;
    try {
      const result = await contextWrites.write(
        "/pages/context-sources/trust-decisions.json",
        {
          source_id: sourceId,
          decision_outcome: "revoke_trusted_ai",
          decision_note: `Revoked trusted AI use for ${source?.title ?? "source"}.`
        },
        { receiptIntent: "pages_context_source_trust_decision" }
      );
      if (result?.status === "REFUSED") {
        throw new Error(result?.reason ?? "refused");
      }
      contextOutcomes = {
        ...contextOutcomes,
        [sourceId]: {
          ok: true,
          line: "Trust revoked",
          receiptId: String(result?.trust_decision_receipt_id ?? "")
        }
      };
      await loadContextSources();
    } catch (error) {
      contextOutcomes = {
        ...contextOutcomes,
        [sourceId]: {
          ok: false,
          line: "Nothing was changed - the source is still trusted."
        }
      };
    }
    contextDeciding = "";
  }

  function contextUsageLine(source) {
    const usage = source?.usage ?? {};
    const parts = [];
    if (usage.used_by_twin) parts.push(`Twin ${usage.twin_usage_count ?? 1}`);
    if (usage.used_by_forge) parts.push(`Forge ${usage.forge_usage_count ?? 1}`);
    if (usage.cited_in_message) parts.push(`Message ${usage.message_citation_count ?? 1}`);
    return parts.length ? parts.join(" · ") : "Not used yet";
  }

  function contextLastUsedLine(source) {
    const usage = source?.usage ?? {};
    if (!usage.last_used_surface) return "";
    // The human signal stays on the row; the receipt is evidence, kept one
    // quiet step away rather than printed as first-read text.
    return `Last used by ${humanToken(usage.last_used_surface)}`;
  }

  function contextLastUsedReceipt(source) {
    return String(source?.usage?.last_used_receipt_id ?? "");
  }

  async function decide(documentId, verb) {
    if (deciding) return;
    deciding = documentId;
    // Clear any prior outcome line for this doc so a retry reads cleanly.
    const next = { ...outcomes };
    delete next[documentId];
    outcomes = next;
    const path = verb === "approve" ? approvePath : rejectPath;
    const intent = verb === "approve" ? "pages_approval_approve" : "pages_approval_reject";
    const request = pendingRows.find((row) => row.documentId === documentId);
    try {
      const result = await approvals.write(
        path,
        {
          document_id: documentId,
          approval_request_receipt_id: request?.requestReceiptId ?? ""
        },
        { receiptIntent: intent }
      );
      const approved = result?.decision_outcome === "approved";
      outcomes = {
        ...outcomes,
        [documentId]: {
          ok: true,
          approved,
          line: approved
            ? "Approved - publishing is its own step."
            : "Declined - this draft will not publish.",
          receiptId: String(result?.approval_decision_receipt_id ?? ""),
          terminalState: String(result?.status ?? "")
        }
      };
      // Re-read so the strip drops the now-decided row and the library note
      // reflects the saved decision, not optimistic local state. Lifecycle
      // too - an open editor's Publish gate must see the approval land.
      await loadApprovals();
      await loadLifecycle();
    } catch (error) {
      outcomes = {
        ...outcomes,
        [documentId]: {
          ok: false,
          line: "Nothing was recorded - that decision did not save. You can try again."
        }
      };
    }
    deciding = "";
  }

  // The editor: words autosave on THIS machine; the record of the draft,
  // the review ask, and the publish are receipt-backed kernel writes; a
  // published body lands write-once in the local store and the kernel
  // records the reference - the file is the recorded source.
  let editorOpen = $state(false);
  let editorDoc = $state("");
  let editorTitle = $state("");
  let editorText = $state("");
  // The page kind the author is declaring. First-class, not parsed from the
  // title - a published page carries its type on the receipt.
  let editorType = $state("knowledge");
  const PAGE_TYPES = ["knowledge", "spec", "decision", "standard", "signal", "changelog"];
  const PAGE_TYPE_LABEL = {
    knowledge: "Knowledge",
    spec: "Spec",
    decision: "Decision",
    standard: "Standard",
    signal: "Signal",
    changelog: "Changelog"
  };
  let editorBaseline = $state("");
  let compareOpen = $state(false);
  let editorNote = $state("");
  let editorBusy = $state(false);
  let editorSavedAt = $state(null);
  let lastDraftId = $state("");
  let lastDraftReceiptId = $state("");
  let lastRecordedText = $state("");
  let lastRecordedTitle = $state("");
  let editorOrigin = $state(["", ""]);
  let editorAutosaveID = $state("");
  let autosaveTimer = null;

  function persistDraftIndex() {
    try {
      localStorage.setItem(PAGES_DRAFT_INDEX_KEY, JSON.stringify(localDrafts));
    } catch (error) {
      // Draft bodies still remain in their per-document autosave keys.
    }
  }

  function loadDraftWorkspace() {
    try {
      const indexed = normalizeDraftIndex(JSON.parse(localStorage.getItem(PAGES_DRAFT_INDEX_KEY) ?? "[]"));
      const discovered = [];
      for (let index = 0; index < localStorage.length; index += 1) {
        const key = localStorage.key(index);
        if (!key?.startsWith("mdx-pages-editor:")) continue;
        const documentId = key.slice("mdx-pages-editor:".length);
        const stored = JSON.parse(localStorage.getItem(key) ?? "null");
        if (!stored?.text) continue;
        discovered.push({
          documentId,
          title: stored.title ?? humanToken(documentId),
          pageType: stored.pageType ?? "knowledge",
          updatedAt: Number(stored.updatedAt ?? 0),
          draftId: stored.draftId ?? "",
          draftReceiptId: stored.draftReceiptId ?? ""
        });
      }
      localDrafts = normalizeDraftIndex([...indexed, ...discovered]);
      persistDraftIndex();
    } catch (error) {
      localDrafts = [];
    }
  }

  function rememberDraft(documentId, draftId = lastDraftId) {
    if (!documentId || !editorText.trim()) return;
    localDrafts = upsertDraftIndex(localDrafts, {
      documentId,
      title: editorTitle.trim() || humanToken(documentId),
      pageType: editorType,
      updatedAt: Date.now(),
      draftId,
      draftReceiptId: lastDraftReceiptId
    });
    persistDraftIndex();
  }

  function forgetDraft(documentId) {
    localDrafts = removeDraftFromIndex(localDrafts, documentId);
    persistDraftIndex();
    try {
      localStorage.removeItem(`mdx-pages-editor:${documentId}`);
    } catch (error) {
      // The published page still exists even if local cleanup is unavailable.
    }
  }

  // Templates: nine declared shapes from the generated registry. Picking
  // one seeds the scaffold; the must-have sections stay ADVISORY - the
  // editor names what is still to cover, it never blocks words. The
  // shape's suggested review cadence seeds stewardship at publish.
  const templates = $derived(data.templates ?? []);
  let editorTemplate = $state(null);

  function pickTemplate(template) {
    editorTemplate = template;
    if (!editorText.trim()) {
      // The scaffold minus its title line - the title field owns the name.
      editorText = template.scaffold.replace(/^# .*\n+/, "");
      autosaveEditor();
    }
  }

  // A section counts as covered once its heading has real words under it;
  // a bare list marker left from the scaffold does not count.
  function sectionCovered(text, section) {
    const lines = text.split("\n");
    for (let i = 0; i < lines.length; i += 1) {
      const heading = lines[i].trim();
      if (!heading.startsWith("#")) continue;
      if (!heading.replace(/^#+\s*/, "").toLowerCase().startsWith(section.toLowerCase())) continue;
      for (let j = i + 1; j < lines.length; j += 1) {
        const next = lines[j].trim();
        if (next.startsWith("#")) break;
        if (next.replace(/^([-*]|\d+\.)\s*/, "")) return true;
      }
    }
    return false;
  }

  const missingSections = $derived(
    editorTemplate
      ? editorTemplate.required_sections.filter((section) => !sectionCovered(editorText, section))
      : []
  );

  function slugifyTitle(title) {
    const slug = title.toLowerCase().replace(/[^a-z0-9]+/g, "_").replace(/^_+|_+$/g, "").slice(0, 48);
    return `page_${slug || "untitled"}`;
  }

  function mintId(prefix) {
    return `${prefix}_${Date.now().toString(36)}${Math.floor(Math.random() * 1296).toString(36)}`;
  }

  function titleRepeatLine(page, index) {
    const title = page?.title ?? "";
    if (!title) return "";
    const matches = pages.filter((entry) => entry.title === title);
    if (matches.length < 2) return "";
    const occurrence = pages.slice(0, index + 1).filter((entry) => entry.title === title).length;
    return `${occurrence} of ${matches.length} with this title`;
  }

  function openEditor(documentId) {
    editorOpen = true;
    compareOpen = false;
    editorNote = "";
    lastDraftId = "";
    lastDraftReceiptId = "";
    lastRecordedText = "";
    lastRecordedTitle = "";
    editorOrigin = ["", ""];
    editorTemplate = null;
    editorAutosaveID = documentId ?? "";
    if (documentId) {
      editorDoc = documentId;
      editorBaseline = selectedId === documentId ? rawBody : "";
      editorType = typeByDoc[documentId] ?? "knowledge";
      try {
        const stored = JSON.parse(localStorage.getItem(`mdx-pages-editor:${documentId}`) ?? "null");
        editorTitle = stored?.title ?? detail?.title ?? humanToken(documentId);
        editorText = stored?.text ?? editorBaseline;
        lastDraftId = stored?.draftId ?? "";
        lastDraftReceiptId = stored?.draftReceiptId ?? "";
        lastRecordedText = stored?.recordedText ?? "";
        lastRecordedTitle = stored?.recordedTitle ?? "";
        editorOrigin = stored?.origin ?? ["", ""];
        editorTemplate = templates.find((t) => t.id === stored?.template) ?? null;
        if (stored?.pageType) editorType = stored.pageType;
      } catch (error) {
        editorTitle = detail?.title ?? humanToken(documentId);
        editorText = editorBaseline;
      }
    } else {
      editorDoc = "";
      editorTitle = "";
      editorText = "";
      editorBaseline = "";
      editorType = "knowledge";
    }
    if (typeof requestAnimationFrame === "function") {
      requestAnimationFrame(() => {
        document.querySelector(".library.editing")?.scrollIntoView({ block: "start", behavior: "smooth" });
      });
    }
  }

  function autosaveEditor() {
    clearTimeout(autosaveTimer);
    autosaveTimer = setTimeout(() => {
      // A blank body never overwrites saved words - titles get typed
      // before bodies, and the machine must not forget the draft.
      if (!editorText.trim()) return;
      const id = editorDoc || slugifyTitle(editorTitle);
      try {
        if (!editorDoc && editorAutosaveID && editorAutosaveID !== id) {
          localStorage.removeItem(`mdx-pages-editor:${editorAutosaveID}`);
          localDrafts = removeDraftFromIndex(localDrafts, editorAutosaveID);
        }
        editorAutosaveID = id;
        localStorage.setItem(
          `mdx-pages-editor:${id}`,
          JSON.stringify({
            title: editorTitle,
            text: editorText,
            template: editorTemplate?.id ?? "",
            pageType: editorType,
            draftId: lastDraftId,
            draftReceiptId: lastDraftReceiptId,
            recordedText: lastRecordedText,
            recordedTitle: lastRecordedTitle,
            origin: editorOrigin,
            updatedAt: Date.now()
          })
        );
        rememberDraft(id);
        editorSavedAt = new Date();
      } catch (error) {
        // tab-local only
      }
    }, 400);
  }

  // Typing a title that matches an unfinished page brings its words back.
  function maybeRestoreAutosave() {
    if (editorText.trim() || !editorTitle.trim()) return;
    try {
      const stored = JSON.parse(
        localStorage.getItem(`mdx-pages-editor:${slugifyTitle(editorTitle)}`) ?? "null"
      );
      if (stored?.text) {
        editorText = stored.text;
        editorTemplate = templates.find((t) => t.id === stored?.template) ?? editorTemplate;
        editorSavedAt = new Date();
      }
    } catch (error) {
      // tab-local only
    }
  }

  // A naive honest diff: lines only in the draft are additions, lines
  // only in the published body are removals - enough to see the change.
  const compareLines = $derived.by(() => {
    if (!compareOpen) return [];
    const baseline = new Set(editorBaseline.split("\n"));
    const draft = new Set(editorText.split("\n"));
    const out = [];
    for (const line of editorBaseline.split("\n")) {
      if (!draft.has(line) && line.trim()) out.push({ kind: "removed", line });
    }
    for (const line of editorText.split("\n")) {
      if (!baseline.has(line) && line.trim()) out.push({ kind: "added", line });
    }
    return out;
  });

  async function saveDraftToRecord() {
    if (editorBusy || !editorText.trim() || !editorTitle.trim()) return;
    editorBusy = true;
    const documentId = editorDoc || slugifyTitle(editorTitle);
    const draftId = mintId("draft");
    try {
      const packet = await approvals.write(
        "/pages/edit-drafts.json",
        {
          document_id: documentId,
          draft_id: draftId,
          title: editorTitle,
          body_text: editorText,
          origin_receipt_id: editorOrigin[0],
          origin_surface: editorOrigin[1],
          revision_id: mintId("rev")
        },
        { receiptIntent: "pages_edit_draft" }
      );
      if (packet?.status === "REFUSED") {
        editorNote = packet.reason;
      } else {
        editorDoc = documentId;
        lastDraftId = draftId;
        lastDraftReceiptId = String(packet?.edit_draft_receipt_id ?? "");
        lastRecordedText = editorText;
        lastRecordedTitle = editorTitle;
        editorNote = "Draft saved";
        rememberDraft(documentId, draftId);
        autosaveEditor();
        loadLifecycle();
      }
    } catch (error) {
      editorNote = "The draft record did not save - the words are still in this browser.";
    }
    editorBusy = false;
  }

  async function askForReview() {
    if (editorBusy || !editorDocId || !lastDraftId || !lastDraftReceiptId) return;
    editorBusy = true;
    try {
      const packet = await approvals.write(
        "/pages/approval-requests.json",
        {
          document_id: editorDocId,
          draft_id: lastDraftId,
          source_edit_draft_receipt_id: lastDraftReceiptId
        },
        { receiptIntent: "pages_approval_request" }
      );
      if (packet?.status === "REFUSED") {
        editorNote = packet.reason ?? "The review ask was refused.";
      } else {
        editorNote = "Review asked";
        rememberDraft(editorDocId, lastDraftId);
        loadLifecycle();
        loadApprovals();
      }
    } catch (error) {
      editorNote = "The review ask did not record. Try again.";
    }
    editorBusy = false;
  }

  async function publishPage() {
    if (editorBusy || !editorDocId || !editorApprovalCoversCurrentDraft) return;
    editorBusy = true;
    try {
      const packet = await approvals.write(
        "/pages/publications.json",
        {
          document_id: editorDocId,
          page_type: editorType,
          approval_decision_receipt_id: editorApprovedRow.decisionReceiptId
        },
        { receiptIntent: "pages_publication" }
      );
      if (packet?.status === "REFUSED") {
        editorNote = packet.reason;
      } else {
        if (editorTemplate) {
          // The shape's suggested cadence seeds stewardship - recorded as
          // an audited change like any other; a human can retune it.
          try {
            await approvals.write(
              "/pages/stewardship.json",
              {
                document_id: editorDocId,
                review_interval_days: editorTemplate.review_interval_days,
                actor_id: data.session?.user_id ?? "local_user"
              },
              { receiptIntent: "pages_stewardship" }
            );
          } catch (error) {
            // The publish stands; the cadence can be set by hand later.
          }
        }
        editorNote = "Published";
        forgetDraft(editorDocId);
        editorOpen = false;
        // The quiet capture chip: a published page becomes a company note MDx
        // can recall once a second person clears it. Join the publish to that
        // note through the receipt it just minted, and offer a look.
        captureChip = {
          receiptId: String(packet?.publication_receipt_id ?? packet?.source_receipt_id ?? "")
        };
        await loadLibrary();
        await loadLifecycle();
        await loadRevisions();
        await loadPageTypes();
        bodyFor = "";
        openDocument(editorDocId);
      }
    } catch (error) {
      editorNote = "The publish did not record. Nothing changed.";
    }
    editorBusy = false;
  }

  // The editor's document identity: explicit once a record exists, the
  // title's slug until then - so reopening an unfinished page keeps its
  // place in the lifecycle without re-recording anything.
  const editorDocId = $derived(editorDoc || (editorTitle.trim() ? slugifyTitle(editorTitle) : ""));
  const editorLifecycleState = $derived(lifecycleByDoc[editorDocId]?.state ?? "");
  const editorReviewRow = $derived(pendingRows.find((row) => row.documentId === editorDocId) ?? null);
  const editorApprovedRow = $derived(decisionByDoc.get(editorDocId) ?? null);
  const editorApprovalCoversCurrentDraft = $derived(
    approvalCoversRecordedDraft({
      lifecycleState: editorLifecycleState,
      approval: editorApprovedRow,
      draftId: lastDraftId,
      draftReceiptId: lastDraftReceiptId,
      currentTitle: editorTitle,
      currentText: editorText,
      recordedTitle: lastRecordedTitle,
      recordedText: lastRecordedText
    })
  );

  // Revision history for the open document, from the publication receipts.
  let revisionsByDoc = $state({});
  async function loadRevisions() {
    try {
      const packet = await proxied.read("/pages/publications/projection.json");
      const map = {};
      for (const row of packet.publications ?? packet.documents ?? []) {
        const id = row.document_id;
        if (!id) continue;
        (map[id] = map[id] ?? []).push(row);
      }
      for (const id of Object.keys(map)) map[id].reverse();
      revisionsByDoc = map;
    } catch (error) {
      revisionsByDoc = {};
    }
  }

  async function restoreRevision(row) {
    if (editorBusy) return;
    editorBusy = true;
    try {
      const historical = await proxied.read(
        `/pages/publications/${row.publication_receipt_id}/body.json`
      );
      if (historical?.status === "REFUSED" || !String(historical?.body ?? "").trim()) {
        editorNote = historical?.reason ?? "The exact historical words are not available.";
        editorBusy = false;
        return;
      }
      const draftId = mintId("draft");
      const packet = await approvals.write(
        "/pages/edit-drafts.json",
        {
          document_id: row.document_id,
          draft_id: draftId,
          title: historical.title ?? row.title,
          body_text: historical.body,
          revision_id: mintId("rev")
        },
        { receiptIntent: "pages_edit_draft" }
      );
      if (packet?.status === "REFUSED") {
        editorNote = packet.reason ?? "The historical revision could not be reopened.";
        editorBusy = false;
        return;
      }
      editorOpen = true;
      editorDoc = row.document_id;
      editorTitle = String(historical.title ?? row.title ?? humanToken(row.document_id));
      editorText = String(historical.body);
      editorType = typeByDoc[row.document_id] ?? "knowledge";
      lastDraftId = draftId;
      lastDraftReceiptId = String(packet?.edit_draft_receipt_id ?? "");
      lastRecordedText = editorText;
      lastRecordedTitle = editorTitle;
      editorNote = `Revision ${row.revision_id} reopened as a draft. Review is required before publishing.`;
      rememberDraft(row.document_id, draftId);
      autosaveEditor();
      await loadLifecycle();
      await loadApprovals();
    } catch (error) {
      editorNote = "The historical revision did not reopen. Nothing changed.";
    }
    editorBusy = false;
  }

  // The page's life, derived from its own receipts - never a status
  // field that could drift. Loaded once, refreshed after any decision.
  let lifecycleByDoc = $state({});
  async function loadLifecycle() {
    try {
      const packet = await proxied.read("/pages/lifecycle/projection.json");
      const map = {};
      for (const entry of packet.documents ?? []) map[entry.document_id] = entry;
      lifecycleByDoc = map;
    } catch (error) {
      lifecycleByDoc = {};
    }
  }

  // The World Model panel: relationships derived from recorded bodies
  // and receipts - what this page rests on, what rests on it, where the
  // company is talking about it.
  let worldByDoc = $state({});
  async function loadWorldModel() {
    try {
      const packet = await proxied.read("/pages/world-model/projection.json");
      const map = {};
      for (const entry of packet.documents ?? []) map[entry.document_id] = entry;
      worldByDoc = map;
    } catch (error) {
      worldByDoc = {};
    }
  }

  // The declared kind per published document, read from the publications
  // projection. Joined into the library so a page shows what it IS without
  // touching the broader /pages.json read shape. Pages with no published
  // type (onboarding statics) simply show none.
  let typeByDoc = $state({});
  async function loadPageTypes() {
    try {
      const packet = await proxied.read("/pages/publications/projection.json");
      const map = {};
      for (const entry of packet.publications ?? []) {
        if (entry?.document_id && entry?.page_type) map[entry.document_id] = entry.page_type;
      }
      typeByDoc = map;
    } catch (error) {
      typeByDoc = {};
    }
  }

  // The decision graph: the company's recorded judgment, projected from the
  // receipt ledger into decision records - each with the outcome it led to.
  // Corpus-wide, not per-page; this is the precedent surface that turns the
  // library into a world model of what was decided and what happened next.
  let decisions = $state([]);
  let decisionRollup = $state({ count: 0, withOutcome: 0, classes: [] });
  let decisionsOpen = $state(false);
  async function loadDecisions() {
    try {
      const packet = await proxied.read("/pages/decision-graph/projection.json");
      decisions = decisionRows(packet);
      decisionRollup = decisionSummary(packet);
    } catch (error) {
      decisions = [];
      decisionRollup = { count: 0, withOutcome: 0, classes: [] };
    }
  }

  const hasPageWork = $derived(
    pages.length > 0 ||
      contextReviewSources.length > 0 ||
      trustedContextSources.length > 0 ||
      pendingRows.length > 0 ||
      decisionRollup.count > 0
  );
  const showPagesFirstUseHint = $derived(loaded && !hasPageWork);

  // External sources: content a connector brought in from outside MDx, the
  // graph's third origin. Read-only and clearly marked external - never mixed
  // with the internal library.
  let connectorRollup = $state({ itemCount: 0, tenantItems: 0, personalWithheld: 0, sources: [] });
  let externalItems = $state([]);
  let connectorsOpen = $state(false);
  async function loadConnectors() {
    try {
      const packet = await proxied.read("/connectors/projection.json");
      connectorRollup = connectorView(packet);
      externalItems = connectorItems(packet);
    } catch (error) {
      connectorRollup = { itemCount: 0, tenantItems: 0, personalWithheld: 0, sources: [] };
      externalItems = [];
    }
  }

  // Stewardship: owner, reviewer, cadence - audited config; freshness is
  // derived by the lifecycle projection and a stale page says so here.
  let stewardOpen = $state(false);
  let stewardOwner = $state("");
  let stewardReviewer = $state("");
  let stewardDays = $state("90");
  let stewardBusy = $state(false);
  let stewardNote = $state("");

  function openSteward(life) {
    stewardOpen = !stewardOpen;
    stewardOwner = life?.trust?.owner ?? "";
    stewardReviewer = life?.trust?.reviewer ?? "";
    stewardDays = String(life?.freshness?.review_interval_days ?? 90);
    stewardNote = "";
  }

  async function saveSteward(extra = {}) {
    if (stewardBusy || !selectedId) return;
    stewardBusy = true;
    try {
      const packet = await approvals.write(
        "/pages/stewardship.json",
        {
          document_id: selectedId,
          owner: stewardOwner,
          reviewer: stewardReviewer,
          review_interval_days: Number(stewardDays) || 90,
          actor_id: data.session?.user_id ?? "local_user",
          ...extra
        },
        { receiptIntent: "pages_stewardship" }
      );
      stewardNote = packet.change_recorded
        ? "Saved"
        : "No change to record.";
      await loadLifecycle();
    } catch (error) {
      stewardNote = "The change did not record. Try again.";
    }
    stewardBusy = false;
  }

  function freshnessLine(life) {
    const fresh = life?.freshness;
    if (!fresh) return "";
    if (!fresh.last_reviewed_epoch_seconds) {
      return `Reviews expected every ${fresh.review_interval_days} days - not yet reviewed.`;
    }
    const last = new Date(fresh.last_reviewed_epoch_seconds * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" });
    const due = new Date(fresh.next_due_epoch_seconds * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" });
    return fresh.stale
      ? `Last reviewed ${last} - past its ${fresh.review_interval_days}-day review window.`
      : `Last reviewed ${last} - next due ${due}.`;
  }

  // Comments live on the Message contract: a comment is a recorded write
  // with thread "page:<document id>", resolving one is a recorded write
  // with thread "resolve:<comment receipt id>" - folded at render. The
  // same receipts power both this panel and the Message surface.
  let commentsByDoc = $state({});
  let commentDraft = $state("");
  let commentBusy = $state(false);
  let commentNote = $state("");

  async function loadComments(documentId) {
    if (!documentId) return;
    try {
      const packet = await proxied.read(
        `/messages/thread-messages/projection.json?thread_id=${encodeURIComponent(`page:${documentId}`)}`
      );
      const comments = (packet.messages ?? []).map((row) => ({
        receiptId: row.message_receipt_id ?? "",
        actor: String(row.actor_display_name ?? "someone"),
        body: row.body ?? "",
        createdAt: row.created_at ?? ""
      }));
      const resolves = await proxied.read(
        `/messages/thread-messages/projection.json?q=${encodeURIComponent("resolved this comment")}`
      );
      const resolvedSet = new Set(
        (resolves.messages ?? [])
          .map((row) => String(row.thread_id ?? ""))
          .filter((thread) => thread.startsWith("resolve:"))
          .map((thread) => thread.slice("resolve:".length))
      );
      commentsByDoc = {
        ...commentsByDoc,
        [documentId]: comments.map((comment) => ({ ...comment, resolved: resolvedSet.has(comment.receiptId) }))
      };
    } catch (error) {
      // the panel stays quiet when the kernel is away
    }
  }

  async function postComment() {
    const text = commentDraft.trim();
    if (!text || commentBusy || !selectedId) return;
    commentBusy = true;
    try {
      const packet = await approvals.write(
        "/messages/thread-messages.json",
        {
          body: text,
          channel_id: "pages",
          thread_id: `page:${selectedId}`,
          message_id: `msg_pc${Date.now().toString(36)}`
        },
        { receiptIntent: "message_thread_message" }
      );
      commentDraft = "";
      commentNote = "Saved";
      await loadComments(selectedId);
    } catch (error) {
      commentNote = "The comment did not save. Try again.";
    }
    commentBusy = false;
  }

  async function resolveComment(comment) {
    if (commentBusy) return;
    commentBusy = true;
    try {
      await approvals.write(
        "/messages/thread-messages.json",
        {
          body: "resolved this comment",
          channel_id: "pages",
          thread_id: `resolve:${comment.receiptId}`,
          message_id: `msg_pr${Date.now().toString(36)}`
        },
        { receiptIntent: "message_thread_message" }
      );
      await loadComments(selectedId);
    } catch (error) {
      commentNote = "The resolve did not save.";
    }
    commentBusy = false;
  }

  async function commentToDecision(comment) {
    if (commentBusy) return;
    commentBusy = true;
    try {
      const packet = await approvals.write(
        "/messages/thread-messages.json",
        {
          body: `Decision: ${detail?.title ?? selectedId}\n${comment.body}\nWhy: raised in a page comment by ${comment.actor}.`,
          channel_id: "decisions",
          message_id: `msg_pd${Date.now().toString(36)}`
        },
        { receiptIntent: "message_thread_message" }
      );
      commentNote = "Decision recorded in #decisions";
      await loadComments(selectedId);
      await loadWorldModel();
    } catch (error) {
      commentNote = "The decision did not record.";
    }
    commentBusy = false;
  }

  const STATE_LINES = {
    draft: "Draft in motion",
    in_review: "Waiting on a review",
    approved: "Approved, not yet published",
    needs_work: "Sent back for another pass",
    published: "Published"
  };

  function lifecycleLabel(state) {
    return STATE_LINES[state] ?? humanToken(state || "draft");
  }

  let bodyFor = "";
  async function openDocument(documentId) {
    selectedId = documentId;
    if (bodyFor === documentId) return;
    bodyFor = documentId;
    bodyBlocks = null;
    bodyProvenance = null;
    detail = null;
    bodyState = "loading";
    try {
      const packet = await proxied.read(`/pages/${documentId}/body`);
      if (bodyFor !== documentId) return;
      rawBody = String(packet.body ?? "");
      bodyBlocks = pagesMarkdownBlocks(rawBody);
      bodyProvenance = {
        bodyRef: String(packet.body_ref ?? ""),
        allowlisted: packet.body_ref_allowlisted === true,
        standaloneStore: packet.standalone_store_allowed === true,
        sourceReceiptIds: Array.isArray(packet.source_receipt_ids) ? packet.source_receipt_ids : [],
        charterEvidenceIds: Array.isArray(packet.charter_evidence_ids) ? packet.charter_evidence_ids : []
      };
      detail = { title: packet.title ?? humanToken(documentId), revision: packet.revision_id ?? "" };
      bodyState = "loaded";
      loadComments(documentId);
    } catch (error) {
      if (bodyFor === documentId) bodyState = "unavailable";
    }
  }

  async function runSearch() {
    const q = searchQuery.trim();
    const seq = ++searchSeq;
    if (!q) {
      searchResults = null;
      return;
    }
    searchBusy = true;
    try {
      const packet = await proxied.read(`/pages/search.json?q=${encodeURIComponent(q)}`);
      if (seq === searchSeq) {
        searchResults = Array.isArray(packet.results) ? packet.results : [];
      }
    } catch (error) {
      if (seq === searchSeq) searchResults = [];
    }
    if (seq === searchSeq) searchBusy = false;
  }

  function onSearchInput() {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(runSearch, 250);
  }

  function openResult(result) {
    searchQuery = "";
    searchResults = null;
    openDocument(result.document_id);
  }

  onMount(() => {
    loadDraftWorkspace();
    loadLifecycle();
    loadRevisions();
    loadWorldModel();
    loadDecisions();
    loadPageTypes();
    loadConnectors();
    try {
      const params = new URLSearchParams(location.search);
      const wanted = params.get("draft");
      if (wanted) {
        const stored = JSON.parse(localStorage.getItem(`mdx-pages-editor:${wanted}`) ?? "null");
        if (stored) {
          openEditor(wanted);
        }
      }
      // The admin dashboard's deep link: land on the page that needs the eye.
      const doc = params.get("doc");
      if (doc) openDocument(doc);
    } catch (error) {
      // hand-off is best effort
    }
    loadLibrary();
    loadApprovals();
    loadContextSources();
  });
</script>

<svelte:head><title>Pages - MDx</title></svelte:head>

<section class="pages-route" data-route-state="ready">
  <header class="pages-head">
    <div class="mdx-page-head">
      <h1>Pages</h1>
      <p class="mdx-page-sub">What your company knows, written down so your agents can cite it once you trust it.</p>
      {#if pages.length > 0}
        <p class="memory-pulse">
          {pages.length} {pages.length === 1 ? "page" : "pages"}{#if data.pulse?.citable != null}&nbsp;· {data.pulse.citable} citable by agents right now{/if}{#if data.pulse?.heldBack != null && data.pulse.heldBack > 0}&nbsp;· {data.pulse.heldBack} held back until they earn trust{/if}
          {#if data.pulse.sourceReviews != null && data.pulse.sourceReviews > 0}&nbsp;· {data.pulse.sourceReviews} {data.pulse.sourceReviews === 1 ? "source" : "sources"} need review{/if}
        </p>
      {/if}
    </div>
    <div class="context-actions">
      <button type="button" class="mdx-btn primary" onclick={() => (sourceComposerOpen = !sourceComposerOpen)}>Add source</button>
      <button type="button" class="mdx-btn" onclick={() => openEditor(null)}>Write a page</button>
    </div>
    <label class="ask">
      <span class="ask-label">Search your pages</span>
      <input
        class="pages-search"
        type="text"
        placeholder="What do we know about..."
        bind:value={searchQuery}
        oninput={onSearchInput}
      />
    </label>
  </header>

  <div class="pages-pack-prompt">
    <MarketplacePackPrompt app="pages" objectId={selectedId || "library"} task={searchQuery} compact />
  </div>

  {#if captureChip}
    <div class="capture-chip" role="status">
      <span class="capture-chip-text">MDx will remember this page. <a href="/memory">See it in Memory</a></span>
      <details class="capture-chip-more">
        <summary>view the record</summary>
        <code>{captureChip.receiptId}</code>
      </details>
      <button type="button" class="capture-chip-dismiss" aria-label="Dismiss" onclick={() => (captureChip = null)}>Dismiss</button>
    </div>
  {/if}

  {#if searchQuery.trim()}
    <div class="search-results" aria-label="Search results">
      <p class="search-mode">
        Word-for-word search of the pages saved on this machine
        {searchBusy ? " · searching..." : searchResults ? ` · ${searchResults.length} ${searchResults.length === 1 ? "match" : "matches"}` : ""}
      </p>
      <div class="search-filters">
        <select bind:value={filterState} aria-label="Filter by state">
          <option value="any">any state</option>
          <option value="published">published</option>
          <option value="draft">draft</option>
          <option value="in_review">in review</option>
          <option value="approved">approved</option>
          <option value="needs_work">needs work</option>
        </select>
        <label class="filter-citable">
          <input type="checkbox" bind:checked={filterCitable} />
          available to agents
        </label>
        <input
          class="filter-owner"
          type="text"
          placeholder="owner contains..."
          bind:value={filterOwner}
          aria-label="Filter by owner"
        />
      </div>
      {#if filteredResults && filteredResults.length === 0 && !searchBusy}
        <p class="search-empty">No saved page matches that. Nothing is invented to fill the gap.</p>
      {:else if filteredResults}
        {#each filteredResults as result}
          <button type="button" class="search-hit" onclick={() => openResult(result)}>
            <strong>{result.title ?? humanToken(result.document_id)}</strong>
            <span class="hit-chips">
              {#each resultChips(result) as chip (chip)}
                <span class="hit-chip" data-chip={chip}>{chip}</span>
              {/each}
            </span>
            {#if result.snippet}<span>{result.snippet}</span>{/if}
            {#if result.why?.length}
              <span class="hit-why">why: {result.why.join("; ")}</span>
            {/if}
          </button>
        {/each}
      {/if}
    </div>
  {:else}
    <div class="work" class:editing={editorOpen}>
    {#if showPagesFirstUseHint}
      <FirstUseHint
        surface="pages"
        title="First time in Pages?"
        body="Add source material, review what should count for the company, and let agents use only what has earned trust."
      />
    {/if}
    {#if sourceComposerOpen}
      <section class="context-add" aria-label="Add source">
        <div class="context-review-head">
          <div>
            <p class="eyebrow">Add source</p>
            <h2>Bring company context into review</h2>
          </div>
          <span class="context-state">{contextArtifactCount} artifacts · {contextStaleCount} stale · {connectorReadyCount} connectors ready</span>
        </div>
        <div class="context-add-grid">
          <label>
            <span>Source title</span>
            <input type="text" bind:value={sourceTitle} placeholder="Architecture standard" />
          </label>
          <label>
            <span>Type</span>
            <select bind:value={sourceMode}>
              <option value="text">Text or markdown</option>
              <option value="csv">CSV preview</option>
              <option value="pdf">PDF text</option>
              <option value="link">Link reference</option>
            </select>
          </label>
          {#if sourceMode === "link"}
            <label>
              <span>Connector</span>
              <select bind:value={sourceKind}>
                <option value="github">GitHub</option>
                <option value="web">Web link</option>
              </select>
            </label>
            <label class="context-add-wide">
              <span>Reference</span>
              <input type="url" bind:value={sourceRef} placeholder="https://github.com/company/repo/docs/standard.md" />
            </label>
          {:else}
            <label>
              <span>Format</span>
              <select bind:value={sourceMime}>
                <option value="text/markdown">Markdown</option>
                <option value="text/plain">Plain text</option>
                <option value="text/csv">CSV</option>
                <option value="application/pdf">PDF text</option>
              </select>
            </label>
            <label class="context-add-wide">
              <span>Source text</span>
              <textarea bind:value={sourceText} rows="5" placeholder="Paste the text or markdown you want agents to be able to cite."></textarea>
            </label>
          {/if}
        </div>
        <div class="context-add-foot">
          <button type="button" class="mdx-btn primary" onclick={addContextSource} disabled={sourceBusy}>
            {sourceBusy ? "Adding..." : "Add to review"}
          </button>
          {#if sourceNote}
            <p class:bad={sourceNote !== "Added for review"} class="context-source-outcome">{sourceNote}</p>
            {#if sourceReceiptId}
              <details class="quiet-more context-receipt">
                <summary>view record</summary>
                <span>{sourceReceiptId}</span>
              </details>
            {/if}
          {/if}
        </div>
      </section>
    {/if}
    {#if contextReviewSources.length > 0}
      <section class="context-review" aria-label="Sources waiting for review">
        <div class="context-review-head">
          <div>
            <p class="eyebrow">Sources to review</p>
            <h2>{contextReviewSources.length} {contextReviewSources.length === 1 ? "source" : "sources"} need a trust decision</h2>
          </div>
          <span class="context-state">You decide what agents can use</span>
        </div>
        <div class="context-source-rows">
          {#each contextReviewSources.slice(0, 3) as source}
            <article class="context-source-row">
              <div>
                <p class="context-source-title">{source.title}</p>
                <p class="context-source-meta">{source.visible_trust_state} · {source.company_context_source || "company context pending"}</p>
                {#if contextOutcomes[source.source_id]?.line}
                  <p class:bad={!contextOutcomes[source.source_id].ok} class="context-source-outcome">
                    {contextOutcomes[source.source_id].line}
                  </p>
                  {#if contextOutcomes[source.source_id].receiptId}
                    <details class="quiet-more context-receipt">
                      <summary>view record</summary>
                      <span>{contextOutcomes[source.source_id].receiptId}</span>
                    </details>
                  {/if}
                {/if}
              </div>
              <button
                type="button"
                class="context-source-next"
                onclick={() => trustContextSource(source)}
                disabled={contextDeciding === source.source_id}
              >
                {contextDeciding === source.source_id ? "Saving..." : "Allow agents to use"}
              </button>
            </article>
          {/each}
        </div>
      </section>
    {/if}
    {#if trustedContextSources.length > 0}
      <section class="context-review" aria-label="Trusted company context usage">
        <div class="context-review-head">
          <div>
            <p class="eyebrow">Trusted company context</p>
            <h2>{trustedContextSources.length} {trustedContextSources.length === 1 ? "source" : "sources"} ready for AI work</h2>
          </div>
          <span class="context-state">Usage from receipts</span>
        </div>
        <div class="context-source-rows">
          {#each trustedContextSources.slice(0, 4) as source}
            <article class="context-source-row">
              <div>
                <p class="context-source-title">{source.title}</p>
                <p class="context-source-meta">Trusted for AI · {contextUsageLine(source)}</p>
                {#if contextLastUsedLine(source)}
                  <p class="context-source-usage">{contextLastUsedLine(source)}</p>
                  {#if contextLastUsedReceipt(source)}
                    <details class="quiet-more context-receipt">
                      <summary>view record</summary>
                      <span>{contextLastUsedReceipt(source)}</span>
                    </details>
                  {/if}
                {/if}
              </div>
              <button
                type="button"
                class="context-source-next"
                onclick={() => revokeContextSource(source)}
                disabled={contextDeciding === source.source_id}
              >
                {contextDeciding === source.source_id ? "Saving..." : "Revoke"}
              </button>
            </article>
          {/each}
        </div>
      </section>
    {/if}
    {#if contextReviewSources.length === 0 && trustedContextSources.length === 0}
      <section class="context-review context-empty" aria-label="Trusted company context">
        <div class="context-review-head">
          <div>
            <p class="eyebrow">Trusted company context</p>
            <h2>No trusted sources yet</h2>
          </div>
          <span class="context-state">You decide what agents can use</span>
        </div>
        <p class="context-empty-body">
          Add a source and review it to give your agents company context they can cite. Until a source is trusted, agents work without it.
        </p>
      </section>
    {/if}
    {#if pendingRows.length > 0}
      <section class="waiting" aria-label="Pages waiting on your decision">
        <p class="waiting-head">
          Waiting on you
          <span class="waiting-count">{pendingRows.length} {pendingRows.length === 1 ? "page" : "pages"}</span>
        </p>
        <ul class="waiting-list">
          {#each pendingRows as row}
            <li class="waiting-row">
              <button type="button" class="waiting-title" onclick={() => openDocument(row.documentId)}>
                {titleFor(row.documentId)}
              </button>
              <div class="waiting-actions">
                <button
                  type="button"
                  class="decide approve"
                  onclick={() => decide(row.documentId, "approve")}
                  disabled={deciding === row.documentId}
                >
                  {deciding === row.documentId ? "Saving..." : "Approve"}
                </button>
                <button
                  type="button"
                  class="decide reject"
                  onclick={() => decide(row.documentId, "reject")}
                  disabled={deciding === row.documentId}
                >
                  Reject
                </button>
              </div>
            </li>
          {/each}
        </ul>
        <p class="waiting-cue">Your decision is saved. Publishing is its own step.</p>
      </section>
    {/if}

    {#if decisionRollup.count > 0}
      <section class="decisions" aria-label="Decisions the company has recorded">
        <button
          type="button"
          class="decisions-head"
          aria-expanded={decisionsOpen}
          onclick={() => (decisionsOpen = !decisionsOpen)}
        >
          <span class="decisions-title">Decisions</span>
          <span class="decisions-count">
            {decisionRollup.count} recorded · {decisionRollup.withOutcome} led to a tracked outcome{decisionRollup.asserted > 0 ? ` · ${decisionRollup.asserted} written by a person` : ""}
          </span>
          <span class="decisions-toggle">{decisionsOpen ? "hide" : "show"}</span>
        </button>
        <p class="decisions-sub">
          The decisions the company has on the record. Most are read back from receipts - the system's own trail of what happened. Some are written by a person as a decision-typed page; those say so, and link to the full record.
        </p>
        {#if decisionsOpen}
          <ul class="decision-list">
            {#each decisions.slice(0, 12) as row (row.id)}
              <li class="decision-row" class:superseded={row.supersededBy} class:asserted={row.origin === "asserted"}>
                <div class="decision-main">
                  <span class="decision-class">{row.classLabel}</span>
                  <strong class="decision-name">{row.title}</strong>
                  {#if row.origin === "asserted"}
                    <span class="decision-origin">written by a person</span>
                  {/if}
                  {#if row.status === "superseded"}
                    <span class="decision-flag">superseded</span>
                  {/if}
                </div>
                {#if row.reason}
                  <p class="decision-why">"{row.reason}"</p>
                {/if}
                {#if row.origin === "asserted"}
                  {#if row.bodyRoute && row.subject}
                    <button type="button" class="decision-read" onclick={() => openDocument(row.subject)}>
                      Read the decision
                    </button>
                  {/if}
                {:else if row.outcome}
                  <p class="decision-outcome">
                    Led to: <strong>{row.outcome.event}</strong>{row.outcome.detail ? ` - ${row.outcome.detail}` : ""}
                  </p>
                {:else}
                  <p class="decision-outcome pending">No tracked outcome yet.</p>
                {/if}
                <Provenance
                  label="Details"
                  items={[
                    { label: "Decision record", value: row.id },
                    { label: "Decided by", value: row.decidedBy },
                    ...(row.outcome ? [{ label: "Outcome record", value: row.outcome.receiptId }] : []),
                    { label: "Confidence", value: row.confidence }
                  ]}
                />
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}

    {#if connectorRollup.itemCount > 0}
      <section class="connectors" aria-label="External sources brought in by connectors">
        <button
          type="button"
          class="decisions-head"
          aria-expanded={connectorsOpen}
          onclick={() => (connectorsOpen = !connectorsOpen)}
        >
          <span class="decisions-title">External sources</span>
          <span class="decisions-count">
            {connectorRollup.itemCount} brought in · {connectorRollup.sources.length} company {connectorRollup.sources.length === 1 ? "source" : "sources"}{connectorRollup.personalWithheld > 0 ? ` · ${connectorRollup.personalWithheld} personal kept in your own view` : ""}
          </span>
          <span class="decisions-toggle">{connectorsOpen ? "hide" : "show"}</span>
        </button>
        <p class="decisions-sub">
          Company sources a connector brought in from outside MDx - recon, docs, posts. Every item is marked external and traced to its source; it is never treated as the company's own truth. Personal sources stay in your own view, not here.
        </p>
        {#if connectorsOpen}
          <ul class="source-list">
            {#each connectorRollup.sources as source (source.sourceId)}
              <li class="source-row">
                <span class="source-kind">{source.kindLabel}</span>
                <strong class="source-id">{source.sourceId}</strong>
                <span class="source-scope" data-scope={source.scope}>{source.scope === "personal" ? "personal" : "company"}</span>
                <span class="source-count">{source.itemCount} {source.itemCount === 1 ? "item" : "items"}{source.sensitiveCount > 0 ? ` · ${source.sensitiveCount} sensitive` : ""}</span>
              </li>
            {/each}
          </ul>
          <ul class="external-list">
            {#each externalItems as item (item.id)}
              <li class="external-row">
                <div class="external-main">
                  <span class="external-kind">{item.kindLabel}</span>
                  <strong class="external-title">{item.title}</strong>
                  {#if item.grade}<span class="external-grade">{item.grade}</span>{/if}
                  {#if item.sensitivity === "sensitive"}
                    <span class="external-sensitive">sensitive · {item.handling}</span>
                  {/if}
                </div>
                {#if item.summary}<p class="external-summary">{item.summary}</p>{/if}
                <Provenance
                  label="Details"
                  items={[
                    { label: "Source", value: item.sourceId },
                    { label: "From", value: item.externalRef },
                    { label: "Brought in by", value: item.ingestedBy },
                    { label: "Scope", value: item.scope },
                    { label: "Record", value: item.id }
                  ]}
                />
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}

    <div class="library" class:editing={editorOpen}>
      <aside class="doc-list" aria-label="Library">
        {#if !loaded}
          <p class="quiet">Reading the library...</p>
        {:else if pages.length === 0 && localDrafts.length === 0}
          <p class="quiet">MDx is not connected right now. The library fills in the moment it is - nothing here pretends otherwise.</p>
        {:else}
          {#if localDrafts.length > 0}
            <p class="doc-section-label">Work in progress <span>{localDrafts.length}</span></p>
            {#each localDrafts as draft (draft.documentId)}
              {@const draftState = lifecycleByDoc[draft.documentId]?.state ?? (pendingRows.some((row) => row.documentId === draft.documentId) ? "in_review" : "draft")}
              <div class="doc-entry draft-entry">
                <button
                  type="button"
                  class="doc-item"
                  class:active={editorOpen && editorDocId === draft.documentId}
                  onclick={() => openEditor(draft.documentId)}
                >
                  <strong>{draft.title}</strong>
                  <span class="doc-meta">
                    <span class="doc-type" data-type={draft.pageType}>{PAGE_TYPE_LABEL[draft.pageType] ?? humanToken(draft.pageType)}</span>
                    <span class="draft-state" data-state={draftState}>{lifecycleLabel(draftState)}</span>
                  </span>
                </button>
              </div>
            {/each}
            {#if pages.length > 0}<p class="doc-section-label published-label">Published <span>{pages.length}</span></p>{/if}
          {/if}
          {#each pages as page, index}
            {@const live = outcomes[page.id]}
            {@const saved = !live ? decisionByDoc.get(page.id) : null}
            {@const repeatLine = titleRepeatLine(page, index)}
            <div class="doc-entry">
              <button
                type="button"
                class="doc-item"
                class:active={page.id === selected?.id}
                onclick={() => openDocument(page.id)}
              >
                <strong>{page.title}</strong>
                <span class="doc-meta">
                  {#if typeByDoc[page.id]}
                    <span class="doc-type" data-type={typeByDoc[page.id]}>{PAGE_TYPE_LABEL[typeByDoc[page.id]] ?? typeByDoc[page.id]}</span>
                  {/if}
                  {page.category}
                  {#if repeatLine}
                    <span class="doc-repeat">{repeatLine}</span>
                  {/if}
                </span>
              </button>
              {#if lifecycleByDoc[page.id]?.state === "published"}
                <p class="doc-note">Published, on the record</p>
              {:else if live}
                <p
                  class="doc-note"
                  class:bad={live.ok === false}
                  class:declined={live.ok && live.approved === false}
                >
                  {live.line}
                </p>
                {#if live.ok && live.receiptId}
                  <details class="quiet-more doc-note-disclose">
                    <summary>details</summary>
                    <span>Decision record: {live.receiptId} · {humanTerminal(live.terminalState)}</span>
                  </details>
                {/if}
              {:else if saved}
                <p class="doc-note" class:declined={saved.note?.kind === "rejected"}>
                  {saved.note?.line}
                </p>
                {#if saved.decisionReceiptId}
                  <details class="quiet-more doc-note-disclose">
                    <summary>details</summary>
                    <span>Decision record: {saved.decisionReceiptId} · {humanTerminal(saved.terminalState)}</span>
                  </details>
                {/if}
              {/if}
            </div>
          {/each}
        {/if}
      </aside>

      <article class="reader" aria-label="Document">
        {#if editorOpen}
          <div class="editor" aria-label="Page editor">
            <div class="editor-head">
              <input
                class="editor-title"
                type="text"
                placeholder={editorTemplate ? `${editorTemplate.title}: what is this one about?` : "What is this page called?"}
                bind:value={editorTitle}
                oninput={() => {
                  maybeRestoreAutosave();
                  autosaveEditor();
                }}
                aria-label="Page title"
              />
              <button type="button" class="editor-close" onclick={() => (editorOpen = false)} aria-label="Close the editor">Close</button>
            </div>
            <div class="editor-type" aria-label="What kind of page this is">
              <span class="editor-type-label">This is a</span>
              <div class="editor-type-chips">
                {#each PAGE_TYPES as kind (kind)}
                  <button
                    type="button"
                    class="type-chip"
                    class:active={editorType === kind}
                    aria-pressed={editorType === kind}
                    onclick={() => {
                      editorType = kind;
                      autosaveEditor();
                    }}
                  >
                    {PAGE_TYPE_LABEL[kind]}
                  </button>
                {/each}
              </div>
            </div>
            {#if !editorDoc && !editorText.trim()}
              <div class="template-row" aria-label="Start from a shape">
                <p class="quiet">Start from a shape, or just write:</p>
                <div class="template-chips">
                  {#each templates as template (template.id)}
                    <button
                      type="button"
                      class="template-chip"
                      onclick={() => pickTemplate(template)}
                      title={template.line}
                    >
                      {template.title}
                    </button>
                  {/each}
                </div>
              </div>
            {/if}
            <textarea
              class="editor-body"
              placeholder="Write in markdown. Your browser keeps a local recovery copy as you type."
              bind:value={editorText}
              oninput={autosaveEditor}
              aria-label="Page body"
            ></textarea>
            <p class="editor-hint">
              {editorSavedAt ? `Saved in this browser at ${editorSavedAt.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })}.` : "Autosaves in this browser as you type."}
              {#if editorBaseline}
                <button type="button" class="editor-compare" onclick={() => (compareOpen = !compareOpen)}>
                  {compareOpen ? "Hide changes" : "Show changes vs published"}
                </button>
              {/if}
            </p>
            {#if editorReviewRow}
              <section class="editor-review" aria-label="Review this draft">
                <div>
                  <strong>Ready for your review</strong>
                  <span>Approve to unlock publishing, or send it back for another pass.</span>
                </div>
                <div class="editor-review-actions">
                  <button type="button" class="decide reject" onclick={() => decide(editorDocId, "reject")} disabled={deciding === editorDocId}>Send back</button>
                  <button type="button" class="decide approve" onclick={() => decide(editorDocId, "approve")} disabled={deciding === editorDocId}>{deciding === editorDocId ? "Saving..." : "Approve"}</button>
                </div>
              </section>
            {:else if editorLifecycleState === "approved" && editorApprovalCoversCurrentDraft}
              <section class="editor-review approved" aria-label="Draft approved">
                <div>
                  <strong>Approved and ready to publish</strong>
                  <span>Your decision covers these exact saved words. Publishing is still a separate choice.</span>
                </div>
              </section>
            {:else if editorLifecycleState === "approved"}
              <section class="editor-review needs-work" aria-label="Approved draft changed">
                <div>
                  <strong>These words changed after approval</strong>
                  <span>Save this version and ask for another decision before publishing.</span>
                </div>
              </section>
            {:else if editorLifecycleState === "needs_work"}
              <section class="editor-review needs-work" aria-label="Draft needs work">
                <div>
                  <strong>Sent back for another pass</strong>
                  <span>Revise the page, save a new draft, and ask for review again.</span>
                </div>
              </section>
            {/if}
            {#if editorTemplate && missingSections.length > 0}
              <p class="template-advice">
                A {editorTemplate.title.toLowerCase()} usually covers: {missingSections.join(", ")}.
                Your call - this never blocks publishing.
              </p>
            {/if}
            {#if compareOpen}
              <div class="compare" aria-label="Changes vs published">
                {#if compareLines.length === 0}
                  <p class="quiet">No line changes yet.</p>
                {:else}
                  {#each compareLines as change, index (index)}
                    <p class="compare-line" data-kind={change.kind}>{change.kind === "added" ? "+" : "-"} {change.line}</p>
                  {/each}
                {/if}
              </div>
            {/if}
            <div class="editor-actions">
              <button type="button" onclick={saveDraftToRecord} disabled={editorBusy || !editorText.trim() || !editorTitle.trim()}>
                Save draft
              </button>
              <button type="button" onclick={askForReview} disabled={editorBusy || !editorDocId || !lastDraftReceiptId || Boolean(editorReviewRow) || editorLifecycleState === "approved" || editorLifecycleState === "needs_work"}>
                {editorReviewRow ? "Review requested" : editorLifecycleState === "approved" && editorApprovalCoversCurrentDraft ? "Approved" : editorLifecycleState === "approved" || editorLifecycleState === "needs_work" ? "Save changes first" : "Ask for review"}
              </button>
              <button
                type="button"
                class="editor-publish"
                onclick={publishPage}
                disabled={editorBusy || !editorApprovalCoversCurrentDraft}
                title={editorApprovalCoversCurrentDraft ? "Publish the exact approved draft" : "Save and approve these exact words before publishing"}
              >
                Publish
              </button>
            </div>
            {#if editorNote}<p class="editor-note">{editorNote}</p>{/if}
            <p class="quiet editor-rails">
              Typing keeps a recovery copy in this browser. Save draft records the exact words in
              this MDx environment. Publishing is a separate governed step, and a published page
              is never silently rewritten - every revision is kept. When you are working alone,
              you may approve your own saved draft; that decision never carries into later edits.
            </p>
          </div>
        {:else if bodyState === "loaded" && bodyBlocks}
          <div class="reader-tools">
            <button type="button" class="mdx-btn small" onclick={() => openEditor(selectedId)}>Edit this page</button>
          </div>
          <h2>{detail?.title}</h2>
          <div class="prose">
            {#each bodyBlocks as block}
              {#if block.kind === "h"}
                <p class="prose-h" data-level={block.level}>{@html proseInline(block.text)}</p>
              {:else if block.kind === "ul"}
                <ul class="prose-ul">
                  {#each block.items as item}<li>{@html proseInline(item)}</li>{/each}
                </ul>
              {:else if block.kind === "ol"}
                <ol class="prose-ul prose-ol">
                  {#each block.items as item}<li>{@html proseInline(item)}</li>{/each}
                </ol>
              {:else if block.kind === "code"}
                <pre class="prose-code">{block.text}</pre>
              {:else if block.kind === "table"}
                <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
                <div class="prose-table-wrap" role="region" tabindex="0" aria-label="Scrollable page table">
                  <table class="prose-table">
                    <thead>
                      <tr>
                        {#each block.headers as header}<th scope="col">{@html proseInline(header)}</th>{/each}
                      </tr>
                    </thead>
                    <tbody>
                      {#each block.rows as row}
                        <tr>
                          {#each row as cell}<td>{@html proseInline(cell)}</td>{/each}
                        </tr>
                      {/each}
                    </tbody>
                  </table>
                </div>
              {:else}
                <p class="prose-p">{@html proseInline(block.text)}</p>
              {/if}
            {/each}
          </div>
          {#if lifecycleByDoc[selectedId]}
            {@const life = lifecycleByDoc[selectedId]}
            <div class="lifecycle" aria-label="This page's life">
              <p class="life-state" data-state={life.state}>{STATE_LINES[life.state] ?? life.state}</p>
              <ol class="life-line">
                {#each life.events as event, index (event.receipt_id + index)}
                  <li class="life-step" data-state={event.state}>
                    <span class="life-dot" aria-hidden="true"></span>
                    <span class="life-label">{event.label}</span>
                    {#if event.note}
                      <span class="life-note">"{event.note}"</span>
                    {/if}
                  </li>
                {/each}
              </ol>
              <p class="life-evidence">
                <Provenance
                  label="Details"
                  items={life.events.map((event) => ({ label: event.label, value: event.receipt_id }))}
                />
              </p>
              <p class="life-steward" class:stale={life.freshness?.stale}>
                {life.trust.owner ? `Owned by ${life.trust.owner}` : "No owner yet"}{life.trust.reviewer ? `, reviewed by ${life.trust.reviewer}` : ""}.
                {freshnessLine(life)}
                <button type="button" class="steward-act" onclick={() => saveSteward({ mark_reviewed: true })} disabled={stewardBusy}>Mark reviewed</button>
                <button type="button" class="steward-act" onclick={() => openSteward(life)}>{stewardOpen ? "close" : "stewardship"}</button>
              </p>
              {#if stewardOpen}
                <div class="steward-form">
                  <input type="text" placeholder="Owner" bind:value={stewardOwner} aria-label="Page owner" />
                  <input type="text" placeholder="Reviewer" bind:value={stewardReviewer} aria-label="Page reviewer" />
                  <input type="number" min="7" max="730" bind:value={stewardDays} aria-label="Review interval in days" />
                  <button type="button" onclick={() => saveSteward()} disabled={stewardBusy}>Save</button>
                  {#if stewardNote}<span class="steward-note">{stewardNote}</span>{/if}
                </div>
              {/if}
              <p class="life-trust">
                {#if life.trust.trusted_for_ai}
                  Available to the company's agents{life.trust.charter_backed ? " - charter-backed" : ""}{life.trust.evidence_count > 0 ? `, ${life.trust.evidence_count} attestations` : ""}.
                {:else}
                  Not available to agents yet - it still needs {life.trust.more_trustworthy_with.join(", ")}.
                {/if}
              </p>
            </div>
          {/if}
          {#if worldByDoc[selectedId]}
            <PagesConnections world={worldByDoc[selectedId]} {titleFor} {openDocument} />
          {/if}
          <PagesComments
            comments={commentsByDoc[selectedId] ?? []}
            bind:draft={commentDraft}
            busy={commentBusy}
            note={commentNote}
            onPost={postComment}
            onResolve={resolveComment}
            onDecision={commentToDecision}
          />
          {#if (revisionsByDoc[selectedId] ?? []).length > 1}
            <details class="history" aria-label="Revision history">
              <summary>{revisionsByDoc[selectedId].length} published revisions</summary>
              <ul class="history-list">
                {#each revisionsByDoc[selectedId] as row, index (row.publication_receipt_id ?? index)}
                  <li>
                    <span class="history-rev">{row.revision_id}</span>
                    <Provenance label="Details" items={[{ label: "Published as", value: row.publication_receipt_id }]} />
                    {#if index > 0}
                      <button type="button" class="history-restore" onclick={() => restoreRevision(row)} disabled={editorBusy}>
                        Reopen as draft
                      </button>
                    {:else}
                      <span class="history-current">current</span>
                    {/if}
                  </li>
                {/each}
              </ul>
            </details>
          {/if}
          {#if bodyProvenance}
            <div class="body-evidence" aria-label="Where this body comes from">
              <Provenance
                label="Verified source"
                items={[
                  { label: "Read live from", value: bodyProvenance.bodyRef },
                  { label: "Records", value: bodyProvenance.sourceReceiptIds.join("  ·  ") },
                  { label: "Attested in Charter", value: bodyProvenance.charterEvidenceIds.map((id) => humanToken(id)).join(", ") }
                ]}
              />
              <small>
                {bodyProvenance.standaloneStore
                  ? "Read live from its recorded source."
                  : "Always the original - Pages keeps no copy, so what you read can never drift from what was proven."}
                {bodyProvenance.allowlisted ? "" : " This reference sits outside the serving allowlist."}
              </small>
            </div>
          {/if}
        {:else if bodyState === "loading"}
          <p class="quiet">Opening the document...</p>
        {:else if bodyState === "unavailable"}
          <p class="quiet">This page was published without a stored body - there is nothing to show. The record and its receipts are real; the content was never written. (Known cause: early first-mission briefs published the record only.)</p>
        {:else if loaded && pages.length > 0}
          <p class="quiet">Pick a page - it opens word for word, exactly as it was saved.</p>
        {/if}
      </article>
    </div>
    </div>
  {/if}

  <footer class="quiet-line">
    Every page reads straight from its source.
    <details class="quiet-more">
      <summary>more</summary>
      <span>Boundary: {data.viewport.boundary.join(" · ")}. Safe next move: {data.viewport.safeNext}</span>
    </details>
  </footer>
</section>

<style>
  .ask {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .ask-label {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--mdx-text-secondary);
  }
  .memory-pulse {
    font-size: 0.85rem;
    color: var(--mdx-text-secondary);
    margin: 6px 0 0;
  }

  .capture-chip {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 12px;
    padding: 8px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill, 999px);
    background: var(--mdx-surface-sunken);
    font-size: 12.5px;
    color: var(--mdx-text-secondary);
  }
  .capture-chip-text a {
    color: var(--mdx-accent-primary);
    text-decoration: none;
    font-weight: 600;
  }
  .capture-chip-more {
    font-size: 11.5px;
    color: var(--mdx-text-muted);
  }
  .capture-chip-more summary {
    cursor: pointer;
    color: var(--mdx-text-tertiary);
  }
  .capture-chip-more code {
    display: block;
    margin-top: 6px;
    font-size: 11px;
    word-break: break-all;
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

  .pages-route {
    display: grid;
    /* Header, capability prompt, optional capture receipt, workspace, footer.
       Keeping all five slots explicit prevents the prompt from being assigned
       the flexible zero-height row and clipping its actions. */
    grid-template-rows: auto auto auto minmax(0, 1fr) auto;
    gap: 16px;
    height: 100%;
    min-height: 480px;
    max-width: 1080px;
  }

  .pages-head { grid-row: 1; }
  .pages-pack-prompt { grid-row: 2; min-width: 0; }
  .capture-chip { grid-row: 3; }
  .search-results,
  .work { grid-row: 4; min-height: 0; }
  .quiet-line { grid-row: 5; }

  .pages-head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
    padding-bottom: 14px;
    border-bottom: 1px solid var(--mdx-border-subtle);
  }

  .pages-search {
    width: 260px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    padding: 8px 12px;
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font-size: var(--mdx-text-sm);
  }

  .search-results {
    overflow-y: auto;
    display: grid;
    gap: 8px;
    align-content: start;
  }

  .search-mode {
    margin: 0;
    color: var(--mdx-text-tertiary);
    font-family: var(--mdx-font-mono);
    font-size: var(--mdx-text-xs);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .search-empty {
    margin: 0;
    color: var(--mdx-text-secondary);
  }

  .search-filters {
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
    margin-bottom: 6px;
  }

  .search-filters select,
  .filter-owner {
    padding: 6px 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: var(--mdx-text-sm);
  }

  .filter-citable {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-sm);
  }

  .context-review {
    display: grid;
    gap: 10px;
    padding: 12px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
  }

  .context-actions {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .context-add {
    display: grid;
    gap: 12px;
    padding: 12px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
  }

  .context-add-grid {
    display: grid;
    grid-template-columns: minmax(180px, 1fr) minmax(140px, 180px) minmax(140px, 180px);
    gap: 10px;
  }

  .context-add label {
    display: grid;
    gap: 5px;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
  }

  .context-add input,
  .context-add select,
  .context-add textarea {
    width: 100%;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-sm);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: var(--mdx-text-sm);
    padding: 7px 8px;
  }

  .context-add textarea {
    resize: vertical;
    min-height: 108px;
  }

  .context-add-wide {
    grid-column: 1 / -1;
  }

  .context-add-foot {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .context-review-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .context-review h2 {
    margin: 2px 0 0;
    color: var(--mdx-text-primary);
    font-size: 1rem;
    line-height: 1.25;
  }

  .context-state {
    flex: none;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
  }

  .context-source-rows {
    display: grid;
    gap: 8px;
  }

  .context-source-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 9px 0 0;
    border-top: 1px solid var(--mdx-border-subtle);
  }

  .context-source-title,
  .context-source-meta {
    margin: 0;
  }

  .context-source-title {
    color: var(--mdx-text-primary);
    font-weight: 650;
    font-size: var(--mdx-text-sm);
  }

  .context-source-meta {
    margin-top: 2px;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-xs);
  }

  .context-source-usage {
    margin: 2px 0 0;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
  }

  .context-source-outcome {
    margin: 4px 0 0;
    color: var(--mdx-accent-success);
    font-size: var(--mdx-text-xs);
  }

  .context-source-outcome.bad {
    color: var(--mdx-accent-warning);
  }

  .context-empty-body {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
    max-width: 60ch;
  }

  .context-receipt {
    margin: 2px 0 0;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
  }

  .context-receipt summary {
    cursor: pointer;
    color: var(--mdx-text-tertiary);
  }

  .context-receipt span {
    display: block;
    margin-top: 2px;
    font-family: var(--mdx-font-mono, monospace);
    word-break: break-all;
  }

  .context-source-next {
    flex: none;
    padding: 4px 8px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-xs);
    cursor: pointer;
  }

  .context-source-next:hover:not(:disabled) {
    border-color: var(--mdx-accent-primary);
    color: var(--mdx-text-primary);
  }

  .context-source-next:disabled {
    opacity: 0.55;
    cursor: default;
  }

  @media (max-width: 760px) {
    .context-add-grid {
      grid-template-columns: 1fr;
    }

    .context-actions {
      width: 100%;
      justify-content: flex-start;
    }
  }

  .hit-chips {
    display: inline-flex;
    gap: 6px;
  }

  .hit-chip {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    padding: 0 8px;
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-tertiary);
  }

  .hit-chip[data-chip="citable"] {
    color: var(--mdx-accent-success, #2f9e6e);
    border-color: var(--mdx-accent-success, #2f9e6e);
  }

  .hit-chip[data-chip="not citable"],
  .hit-chip[data-chip="stale"] {
    color: var(--mdx-accent-danger, #d4183d);
  }

  .hit-why {
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .search-hit {
    display: grid;
    gap: 4px;
    text-align: left;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    padding: 12px 14px;
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    cursor: pointer;
  }

  .search-hit:hover {
    border-color: var(--mdx-border-strong);
  }

  .search-hit span {
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
  }

  .work {
    /* A column of optional top panels (waiting, decisions, external sources)
       above the library, which fills the rest and scrolls within itself.
       Flex (not a fixed 2-row grid) so any number of top panels stack cleanly
       instead of fighting the library for a grid row. */
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-height: 0;
  }

  .work.editing .context-review,
  .work.editing .waiting,
  .work.editing .decisions,
  .work.editing .connectors {
    display: none;
  }

  .decisions {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    padding: 12px 16px;
    display: grid;
    gap: 8px;
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }

  .decisions-head {
    border: none;
    background: transparent;
    padding: 0;
    display: flex;
    align-items: baseline;
    gap: 10px;
    cursor: pointer;
    text-align: left;
    color: var(--mdx-text-primary);
  }

  .decisions-title {
    font-size: 14px;
    font-weight: 600;
  }

  .decisions-count {
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-xs);
  }

  .decisions-toggle {
    margin-left: auto;
    color: var(--mdx-accent-primary);
    font-size: var(--mdx-text-xs);
  }

  .decisions-sub {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-xs);
    line-height: 1.5;
  }

  .decision-list {
    margin: 4px 0 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 10px;
  }

  .decision-row {
    display: grid;
    gap: 4px;
    padding: 10px 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
  }

  .decision-row.superseded {
    opacity: 0.62;
  }

  .decision-main {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-wrap: wrap;
  }

  .decision-class {
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .decision-name {
    font-size: 13.5px;
    color: var(--mdx-text-primary);
  }

  .decision-flag {
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-secondary);
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-sm);
    padding: 1px 6px;
  }

  .decision-why {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    font-style: italic;
  }

  .decision-outcome {
    margin: 0;
    font-size: 12.5px;
    color: var(--mdx-text-primary);
  }

  .decision-outcome.pending {
    color: var(--mdx-text-secondary);
  }

  .decision-row.asserted {
    border-left: 2px solid color-mix(in srgb, var(--mdx-accent-primary) 50%, var(--mdx-border-subtle));
  }

  .decision-origin {
    font-size: var(--mdx-text-xs);
    color: var(--mdx-accent-primary);
    border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 35%, var(--mdx-border-subtle));
    border-radius: var(--mdx-radius-sm);
    padding: 1px 6px;
  }

  .decision-read {
    align-self: start;
    border: none;
    background: transparent;
    padding: 0;
    color: var(--mdx-accent-primary);
    font-size: 12.5px;
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .connectors {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    padding: 12px 16px;
    display: grid;
    gap: 8px;
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }

  .source-list {
    margin: 4px 0 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 6px;
  }

  .source-row {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-wrap: wrap;
    font-size: 12.5px;
  }

  .source-kind,
  .external-kind {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--mdx-text-secondary);
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-sm);
    padding: 0 5px;
    line-height: 15px;
  }

  .source-id {
    color: var(--mdx-text-primary);
  }

  .source-scope {
    font-size: 10px;
    color: var(--mdx-text-secondary);
  }

  .source-scope[data-scope="personal"] {
    color: var(--mdx-accent-primary);
  }

  .source-count {
    margin-left: auto;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
  }

  .external-list {
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 10px;
  }

  .external-row {
    display: grid;
    gap: 4px;
    padding: 10px 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-left: 2px solid color-mix(in srgb, var(--mdx-text-tertiary) 50%, var(--mdx-border-subtle));
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
  }

  .external-main {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-wrap: wrap;
  }

  .external-title {
    font-size: 13.5px;
    color: var(--mdx-text-primary);
  }

  .external-grade {
    font-size: var(--mdx-text-xs);
    color: var(--mdx-accent-primary);
  }

  .external-sensitive {
    font-size: var(--mdx-text-xs);
    color: var(--mdx-status-warn, var(--mdx-text-secondary));
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-sm);
    padding: 0 6px;
  }

  .external-summary {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
  }

  .waiting {
    border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 30%, var(--mdx-border-subtle));
    border-radius: var(--mdx-radius-lg);
    background: color-mix(in srgb, var(--mdx-accent-primary) 6%, var(--mdx-surface-raised));
    padding: 14px 16px;
    display: grid;
    gap: 10px;
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }

  .waiting-head {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13.5px;
    font-weight: 600;
    color: var(--mdx-text-primary);
  }

  .waiting-count {
    color: var(--mdx-accent-primary);
    font-size: var(--mdx-text-xs);
    font-weight: 500;
  }

  .waiting-list {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 8px;
  }

  .waiting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }

  .waiting-title {
    border: none;
    background: transparent;
    padding: 0;
    text-align: left;
    color: var(--mdx-text-primary);
    font-size: 14px;
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 2px;
    text-decoration-color: var(--mdx-border-strong);
  }

  .waiting-title:hover {
    text-decoration-color: var(--mdx-accent-primary);
  }

  .waiting-actions {
    display: flex;
    gap: 8px;
    flex: none;
  }

  .decide {
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    padding: 6px 14px;
    background: var(--mdx-surface-base);
    color: var(--mdx-text-primary);
    font-size: 13px;
    cursor: pointer;
  }

  .decide:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .decide.approve {
    background: var(--mdx-accent-primary);
    border-color: var(--mdx-accent-primary);
    color: #fff;
  }

  .decide.approve:hover:not(:disabled) {
    filter: brightness(1.05);
  }

  .decide.reject:hover:not(:disabled) {
    border-color: var(--mdx-border-strong);
  }

  .waiting-cue {
    margin: 0;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
    line-height: 1.5;
  }

  .doc-entry {
    display: grid;
    gap: 2px;
  }

  .doc-section-label {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 6px 8px 2px;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .doc-section-label span {
    font-variant-numeric: tabular-nums;
  }

  .published-label { margin-top: 14px; }
  .draft-entry .doc-item { border-color: color-mix(in srgb, var(--mdx-accent-primary) 18%, transparent); }
  .draft-state { color: var(--mdx-text-muted); }
  .draft-state[data-state="approved"] { color: var(--mdx-accent-success); }
  .draft-state[data-state="in_review"] { color: var(--mdx-accent-primary); }
  .draft-state[data-state="needs_work"] { color: var(--mdx-accent-warning); }

  .doc-note {
    margin: 2px 0 0;
    padding: 0 12px;
    color: var(--mdx-accent-success);
    font-size: var(--mdx-text-xs);
    line-height: 1.4;
  }

  .doc-note.declined {
    color: var(--mdx-text-tertiary);
  }

  .doc-note.bad {
    color: var(--mdx-accent-warning);
  }

  .doc-note-disclose {
    padding: 0 12px;
    margin-bottom: 4px;
  }

  .doc-note-disclose summary {
    cursor: pointer;
    color: var(--mdx-text-muted);
    text-decoration: underline;
    text-underline-offset: 2px;
    font-size: var(--mdx-text-xs);
  }

  .doc-note-disclose[open] summary {
    display: none;
  }

  .doc-note-disclose span {
    display: block;
    color: var(--mdx-text-tertiary);
    font-family: var(--mdx-font-mono);
    font-size: var(--mdx-text-xs);
    line-height: 1.5;
    overflow-wrap: anywhere;
  }

  .library {
    flex: 1;
    display: grid;
    grid-template-columns: 260px minmax(0, 1fr);
    gap: 18px;
    min-height: 0;
  }

  .library.editing {
    grid-template-columns: minmax(0, 1fr);
  }

  .library.editing .doc-list {
    display: none;
  }

  .doc-list {
    overflow-y: auto;
    display: grid;
    gap: 6px;
    align-content: start;
  }

  .doc-item {
    display: grid;
    gap: 2px;
    text-align: left;
    border: 1px solid transparent;
    border-radius: var(--mdx-radius-md);
    padding: 10px 12px;
    background: transparent;
    color: var(--mdx-text-primary);
    cursor: pointer;
  }

  .doc-item:hover {
    background: var(--mdx-surface-raised);
  }

  .doc-item.active {
    border-color: var(--mdx-border-strong);
    background: var(--mdx-surface-raised);
  }

  .doc-item span {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
  }

  .doc-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .doc-repeat {
    color: var(--mdx-text-muted);
  }

  .doc-type {
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-size: 10px;
    font-weight: 600;
    color: var(--mdx-text-secondary);
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-sm);
    padding: 0 5px;
    line-height: 15px;
  }

  .doc-type[data-type="decision"] {
    color: var(--mdx-accent-primary);
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 40%, var(--mdx-border-subtle));
  }

  .editor-type {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 2px;
  }

  .editor-type-label {
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-xs);
  }

  .editor-type-chips {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .type-chip {
    border: 1px solid var(--mdx-border-subtle);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-secondary);
    border-radius: var(--mdx-radius-sm);
    padding: 2px 9px;
    font-size: var(--mdx-text-xs);
    cursor: pointer;
  }

  .type-chip:hover {
    border-color: var(--mdx-border-strong);
  }

  .type-chip.active {
    background: color-mix(in srgb, var(--mdx-accent-primary) 12%, var(--mdx-surface-base));
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 45%, var(--mdx-border-subtle));
    color: var(--mdx-text-primary);
  }

  .reader {
    overflow-y: auto;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    padding: 22px 26px;
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }

  .library.editing .reader {
    min-height: 0;
  }

  .reader h2 {
    margin: 0 0 14px;
  }

  .quiet {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-sm);
  }

  .prose {
    display: grid;
    gap: 10px;
  }

  .prose-h {
    margin: 8px 0 0;
    font-weight: 700;
  }

  .prose-h[data-level="1"] {
    font-size: 21px;
    margin-top: 16px;
    letter-spacing: -0.01em;
  }

  .prose-h[data-level="2"] {
    font-size: 17px;
    margin-top: 12px;
  }

  .prose-h[data-level="3"] {
    font-size: 14.5px;
    margin-top: 8px;
  }

  .prose-p {
    margin: 0;
    color: var(--mdx-text-primary);
    font-size: 14px;
    line-height: 1.65;
    max-width: 68ch;
  }

  .prose :global(code) {
    font-family: var(--mdx-font-mono);
    font-size: 0.88em;
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--mdx-surface-sunken, var(--mdx-surface-raised));
  }

  .prose :global(a) {
    color: var(--mdx-accent-primary);
  }

  .prose-ul {
    margin: 0;
    padding-left: 22px;
    color: var(--mdx-text-primary);
    font-size: 14px;
    line-height: 1.65;
    display: grid;
    gap: 4px;
    max-width: 68ch;
  }

  .prose-code {
    margin: 0;
    padding: 12px;
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
    font-family: var(--mdx-font-mono);
    font-size: 12px;
    overflow-x: auto;
  }

  .prose-table-wrap {
    max-width: 100%;
    overflow-x: auto;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
  }

  .prose-table {
    width: 100%;
    min-width: 620px;
    border-collapse: collapse;
    table-layout: auto;
    font-size: 13px;
    line-height: 1.45;
  }

  .prose-table th,
  .prose-table td {
    min-width: 140px;
    padding: 10px 12px;
    text-align: left;
    vertical-align: top;
    overflow-wrap: anywhere;
    border-bottom: 1px solid var(--mdx-border-subtle);
  }

  .prose-table th {
    color: var(--mdx-text-secondary);
    font-weight: 700;
    background: var(--mdx-surface-sunken, var(--mdx-surface-base));
  }

  .prose-table tr:last-child td {
    border-bottom: 0;
  }

  .pages-head .mdx-btn {
    flex: none;
  }

  .reader-tools {
    display: flex;
    justify-content: flex-end;
    margin-bottom: 4px;
  }

  .editor {
    display: grid;
    gap: 10px;
  }

  .editor-head {
    display: flex;
    gap: 10px;
  }

  .editor-title {
    flex: 1;
    padding: 10px 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: 16px;
    font-weight: 600;
  }

  .editor-close {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: none;
    color: var(--mdx-text-tertiary);
    padding: 8px 12px;
    cursor: pointer;
  }

  .editor-body {
    min-height: 320px;
    resize: vertical;
    padding: 12px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font-family: var(--mdx-font-mono, monospace);
    font-size: 13px;
    line-height: 1.6;
  }

  .editor-hint {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
    display: flex;
    gap: 12px;
    align-items: baseline;
  }

  .editor-review {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 12px 14px;
    border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 28%, var(--mdx-border-subtle));
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-accent-primary) 5%, var(--mdx-surface-raised));
  }

  .editor-review > div:first-child { display: grid; gap: 3px; }
  .editor-review strong { font-size: var(--mdx-text-sm); }
  .editor-review span { color: var(--mdx-text-secondary); font-size: var(--mdx-text-xs); }
  .editor-review-actions { display: flex; gap: 8px; flex: none; }
  .editor-review.approved { border-color: color-mix(in srgb, var(--mdx-accent-success) 35%, var(--mdx-border-subtle)); background: color-mix(in srgb, var(--mdx-accent-success) 6%, var(--mdx-surface-raised)); }
  .editor-review.needs-work { border-color: color-mix(in srgb, var(--mdx-accent-warning) 35%, var(--mdx-border-subtle)); }

  .template-row {
    display: grid;
    gap: 6px;
  }

  .template-row .quiet {
    margin: 0;
    font-size: var(--mdx-text-xs);
  }

  .template-chips {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .template-chip {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: 999px;
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-secondary);
    padding: 5px 12px;
    cursor: pointer;
    font-size: var(--mdx-text-xs);
  }

  .template-chip:hover {
    border-color: var(--mdx-accent-primary);
    color: var(--mdx-text-primary);
  }

  .template-advice {
    margin: 0;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
  }

  .editor-compare {
    border: none;
    background: none;
    color: var(--mdx-text-tertiary);
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: pointer;
    font-size: var(--mdx-text-xs);
  }

  .compare {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    padding: 8px 12px;
    max-height: 220px;
    overflow-y: auto;
    display: grid;
    gap: 2px;
  }

  .compare-line {
    margin: 0;
    font-family: var(--mdx-font-mono, monospace);
    font-size: 12px;
    white-space: pre-wrap;
  }

  .compare-line[data-kind="added"] {
    color: var(--mdx-accent-success, #2f9e6e);
  }

  .compare-line[data-kind="removed"] {
    color: var(--mdx-accent-danger, #d4183d);
    text-decoration: line-through;
  }

  .editor-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .editor-actions button {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    padding: 9px 14px;
    cursor: pointer;
    font-size: var(--mdx-text-sm);
  }

  .editor-actions button:disabled {
    opacity: 0.55;
    cursor: default;
  }

  .editor-publish:not(:disabled) {
    border-color: var(--mdx-accent-primary);
  }

  .editor-note {
    margin: 0;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-sm);
  }

  .editor-rails {
    font-size: var(--mdx-text-xs);
  }

  .history {
    margin-top: 10px;
  }

  .history summary {
    cursor: pointer;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-sm);
  }

  .history-list {
    list-style: none;
    margin: 6px 0 0;
    padding: 0;
    display: grid;
    gap: 4px;
  }

  .history-list li {
    display: flex;
    gap: 10px;
    align-items: baseline;
    font-size: var(--mdx-text-sm);
  }

  .history-restore {
    border: none;
    background: none;
    color: var(--mdx-text-tertiary);
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: pointer;
    font-size: var(--mdx-text-xs);
  }

  .history-current {
    color: var(--mdx-accent-success, #2f9e6e);
    font-size: var(--mdx-text-xs);
  }

  .lifecycle {
    margin-top: 18px;
    padding: 12px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    display: grid;
    gap: 8px;
  }

  .life-state {
    margin: 0;
    font-size: 13.5px;
    font-weight: 600;
  }

  .life-state[data-state="needs_work"] {
    color: var(--mdx-accent-danger, #d4183d);
  }

  .life-state[data-state="published"] {
    color: var(--mdx-accent-success, #2f9e6e);
  }

  .life-line {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 6px 14px;
  }

  .life-step {
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
  }

  .life-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--mdx-border-default);
    align-self: center;
  }

  .life-step[data-state="published"] .life-dot,
  .life-step[data-state="approved"] .life-dot {
    background: var(--mdx-accent-success, #2f9e6e);
  }

  .life-step[data-state="needs_work"] .life-dot {
    background: var(--mdx-accent-danger, #d4183d);
  }

  .life-label {
    font-size: var(--mdx-text-sm);
  }

  .life-evidence {
    margin: 0;
    display: flex;
  }

  .life-steward {
    margin: 0;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-sm);
    display: flex;
    flex-wrap: wrap;
    gap: 6px 10px;
    align-items: baseline;
  }

  .life-steward.stale {
    color: var(--mdx-accent-danger, #d4183d);
  }

  .steward-act {
    border: none;
    background: none;
    color: var(--mdx-text-muted);
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: pointer;
    font-size: var(--mdx-text-xs);
  }

  .steward-form {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    align-items: center;
  }

  .steward-form input {
    padding: 6px 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: var(--mdx-text-sm);
    width: 150px;
  }

  .steward-form input[type="number"] {
    width: 90px;
  }

  .steward-form button {
    padding: 6px 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    cursor: pointer;
    font-size: var(--mdx-text-sm);
  }

  .steward-note {
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .life-trust {
    margin: 0;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-sm);
  }

  .life-note {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
    font-style: italic;
  }

  .body-evidence {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-top: 16px;
    padding-top: 12px;
    border-top: 1px solid var(--mdx-border-subtle);
  }

  .body-evidence small {
    color: var(--mdx-text-tertiary);
    line-height: 1.5;
  }

  .quiet-line {
    display: flex;
    align-items: baseline;
    gap: 6px;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
    line-height: 1.5;
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

  @media (max-width: 860px) {
    .library {
      grid-template-columns: 1fr;
      grid-template-rows: 200px minmax(0, 1fr);
    }
  }
</style>
