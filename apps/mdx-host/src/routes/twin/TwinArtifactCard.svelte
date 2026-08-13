<script>
  // The governed artifact read, rendered: three things first (name, state,
  // next move), interpretation leading, the uploaded artifact and company
  // context cited separately, one proposal-card grammar, receipts in quiet
  // disclosure. Pure presentation - the forget write stays with the caller,
  // which owns the session. Used by the standalone surface and inline in the
  // Twin thread so the artifact experience reads the same everywhere.
  // saveState: idle | saving | saved | error. onSave fires the governed Pages
  // draft write (B5); when absent the offer stays a preview (standalone use).
  import Markdown from "@mdx/ui/components/Markdown.svelte";

  let {
    report,
    deleting = false,
    onForget,
    onSave,
    saveState = "idle",
    savedRoute = "/pages",
    onMakeOffice,
    officeState = {}
  } = $props();

  const officeKinds = [
    { kind: "docx", label: "Word doc", busy: "Making the doc...", done: "Word doc saved" },
    { kind: "xlsx", label: "Workbook", busy: "Making the workbook...", done: "Workbook saved" },
    { kind: "pptx", label: "Deck", busy: "Making the deck...", done: "Deck saved" }
  ];

  function modelWords(providerClass) {
    return providerClass === "sovereign" ? "your private model" : "your local model";
  }
  // When B3 reports a sensitive read that a private model would sharpen but
  // none is connected, it fails closed with provider_not_connected_reason
  // "connect_private_model". Offer a calm way to connect, pointing at the
  // existing /admin/models surface. Hidden once a private model is connected
  // (the read sharpens through `interpretation` and provider_call_allowed).
  const sharperReadAvailable = $derived(
    report?.provider_not_connected_reason === "connect_private_model"
  );

  // Private models sometimes emit a heading and its paragraph in one block
  // (no blank line between), which the shared renderer only treats as a
  // heading when it stands alone. Give headings their own block so they render
  // instead of showing literal # marks. Local and safe; no shared-lib change.
  const renderText = $derived((report?.interpretation ?? "").replace(/^(#{1,3}\s+.+)$/gm, "$1\n"));

  const typeGlyph = (normalized) => {
    if (!normalized) return "doc";
    if (normalized.includes("csv") || normalized.includes("table")) return "sheet";
    if (normalized.includes("pdf")) return "pdf";
    return "doc";
  };
</script>

<article class="card answered">
  <div class="card-top">
    <span class="glyph" data-kind={typeGlyph(report.normalized_type)} aria-hidden="true"></span>
    <div class="title-block">
      <p class="artifact-name">{report.artifact_name}</p>
      <p class="state-line small">Read it. Here is what it means for you.</p>
    </div>
    {#if onForget}
      <button type="button" class="forget" onclick={onForget} disabled={deleting} aria-label="Forget this file">
        {deleting ? "Removing..." : "Forget"}
      </button>
    {/if}
  </div>

  {#if report.company_context_source}
    <p class="grounded">I also checked your {report.company_context_source}.</p>
  {/if}

  <div class="interpretation"><Markdown text={renderText} /></div>

  <div class="citations">
    <div class="cite-group">
      <p class="cite-label">From your file</p>
      <p class="cite-body">{report.artifact_name}</p>
    </div>
    <div class="cite-group">
      <p class="cite-label">From your company</p>
      <p class="cite-body">{report.company_context_source}</p>
    </div>
  </div>

  <div class="proposal">
    {#if saveState === "saved"}
      <p class="proposal-offer">Saved to Pages as a draft, pending your approval.</p>
      <a class="saved-link" href={savedRoute}>View it in Pages</a>
    {:else}
      <p class="proposal-offer">I can turn this into a Pages draft for the team - your call.</p>
      <p class="proposal-where">Saved to Pages, pending your approval.</p>
      {#if saveState === "error"}
        <p class="save-err" role="alert">Could not save just now - nothing was written. Try again.</p>
      {/if}
      <div class="proposal-actions">
        <button
          type="button"
          class="primary"
          onclick={onSave}
          disabled={saveState === "saving" || !onSave}
          title={onSave ? "" : "Pages draft write lands with slice B5"}
        >
          {saveState === "saving" ? "Saving..." : "Looks right, save it"}
        </button>
        <button type="button" class="ghost">Let me edit first</button>
      </div>
      {#if !onSave}
        <p class="proposal-soon">Saving to Pages turns on with the next slice.</p>
      {/if}
    {/if}
  </div>

  {#if onMakeOffice}
    <div class="take-it">
      <span class="take-label">Take it with you</span>
      <div class="take-actions">
        {#each officeKinds as o}
          {@const state = officeState[o.kind] ?? "idle"}
          <button
            type="button"
            class="take-btn"
            class:done={state === "done"}
            onclick={() => onMakeOffice(o.kind)}
            disabled={state === "busy"}
          >
            {state === "busy" ? o.busy : state === "done" ? o.done : state === "error" ? "Try again" : `Make a ${o.label}`}
          </button>
        {/each}
      </div>
    </div>
  {/if}

  {#if sharperReadAvailable}
    <p class="sharper-hint">
      This is a quick local read. <a href="/admin/models">Connect a private model</a>
      for a sharper take, still fully private.
    </p>
  {/if}

  <p class="reassure">Kept private. Only {modelWords(report.provider_class)} saw this.</p>

  <details class="quiet">
    <summary>How this was handled</summary>
    <ul class="facts">
      <li><span>Privacy</span><b>Private to you ({report.privacy_class})</b></li>
      <li><span>Models</span><b>{modelWords(report.provider_class)} only{report.frontier_raw_content_allowed ? "" : " - no outside model saw the content"}</b></li>
      <li><span>Read</span><b>{report.page_count ? `${report.page_count} ${report.page_count === 1 ? "page" : "pages"}, ` : ""}{report.extracted_text_char_count} characters, on your machine</b></li>
      <li><span>Fingerprint</span><b class="mono">{report.source_artifact_hash}</b></li>
      <li><span>Receipts</span><b class="mono">{report.artifact_admission_receipt_id} - {report.parse_receipt_id} - {report.grounding_receipt_id}</b></li>
      <li><span>Storage</span><b>{report.blob_bytes_stored ? "kept privately on your machine, by reference" : "metadata only, nothing uploaded"}</b></li>
    </ul>
  </details>
</article>

<style>
  .card {
    display: grid;
    gap: 0.65rem;
    padding: 1.25rem 1.35rem;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
  }
  .card.answered { border-left: 3px solid var(--mdx-accent-primary); }

  .state-line { margin: 0; font-size: 1.05rem; font-weight: 600; color: var(--mdx-text-primary); }
  .state-line.small { font-size: 0.92rem; font-weight: 600; }

  .card-top { display: flex; align-items: flex-start; gap: 0.75rem; }
  .glyph {
    flex: none;
    width: 2rem;
    height: 2rem;
    border-radius: 8px;
    background: color-mix(in srgb, var(--mdx-accent-primary) 14%, transparent);
  }
  .title-block { flex: 1; display: grid; gap: 0.15rem; }
  .artifact-name { margin: 0; font-weight: 600; color: var(--mdx-text-primary); font-size: 0.95rem; }
  .forget {
    flex: none;
    padding: 0.35rem 0.7rem;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-pill);
    background: transparent;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-sm);
    cursor: pointer;
  }

  .grounded { margin: 0; color: var(--mdx-accent-primary); font-size: 0.85rem; }
  .interpretation { color: var(--mdx-text-primary); font-size: 1rem; line-height: 1.6; }
  .interpretation :global(p) { margin: 0 0 0.6rem; }
  .interpretation :global(p:last-child) { margin-bottom: 0; }
  .interpretation :global(strong) { color: var(--mdx-text-primary); font-weight: 650; }
  .interpretation :global(ul), .interpretation :global(ol) { margin: 0.3rem 0 0.6rem; padding-left: 1.1rem; }

  .citations { display: grid; grid-template-columns: 1fr 1fr; gap: 0.6rem; }
  .cite-group {
    padding: 0.6rem 0.75rem;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
  }
  .cite-label { margin: 0 0 0.2rem; font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--mdx-text-muted); }
  .cite-body { margin: 0; font-size: 0.85rem; color: var(--mdx-text-secondary); }

  .proposal {
    display: grid;
    gap: 0.4rem;
    padding: 0.85rem 1rem;
    border: 1px solid var(--mdx-border-default);
    border-left: 3px solid var(--mdx-accent-primary);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
  }
  .proposal-offer { margin: 0; color: var(--mdx-text-primary); font-size: 0.9rem; font-weight: 600; }
  .proposal-where { margin: 0; color: var(--mdx-text-secondary); font-size: 0.85rem; }
  .proposal-actions { display: flex; gap: 0.5rem; margin-top: 0.2rem; }
  .proposal-soon { margin: 0.1rem 0 0; color: var(--mdx-text-muted); font-size: 11px; }
  .saved-link { justify-self: start; color: var(--mdx-accent-primary); font-size: 0.85rem; text-decoration: none; }
  .save-err { margin: 0; color: var(--mdx-accent-warning); font-size: var(--mdx-text-sm); }

  .primary {
    padding: 0.55rem 1.15rem;
    border: none;
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-accent-primary);
    color: #fff;
    font-size: 0.9rem;
    font-weight: 600;
    cursor: pointer;
  }
  .primary:disabled { opacity: 0.5; cursor: default; }
  .ghost {
    padding: 0.5rem 1rem;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-pill);
    background: transparent;
    color: var(--mdx-text-secondary);
    font-size: 0.9rem;
    cursor: pointer;
  }

  .reassure { margin: 0.4rem 0 0; color: var(--mdx-text-muted); font-size: var(--mdx-text-sm); }
  .sharper-hint {
    margin: 0;
    padding: 0.5rem 0.7rem;
    border: 1px dashed var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    color: var(--mdx-text-secondary);
    font-size: 0.85rem;
  }
  .sharper-hint a { color: var(--mdx-accent-primary); text-decoration: none; }

  .take-it { display: grid; gap: 0.4rem; }
  .take-label { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--mdx-text-muted); }
  .take-actions { display: flex; flex-wrap: wrap; gap: 0.4rem; }
  .take-btn {
    padding: 0.4rem 0.8rem;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-secondary);
    font-size: 0.85rem;
    cursor: pointer;
  }
  .take-btn:disabled { opacity: 0.6; cursor: default; }
  .take-btn.done { color: var(--mdx-accent-primary); border-color: color-mix(in srgb, var(--mdx-accent-primary) 40%, transparent); }

  .quiet { font-size: var(--mdx-text-sm); color: var(--mdx-text-tertiary); }
  .quiet summary { cursor: pointer; color: var(--mdx-text-muted); }
  .facts { list-style: none; margin: 0.6rem 0 0; padding: 0; display: grid; gap: 0.35rem; }
  .facts li { display: grid; grid-template-columns: 6rem 1fr; gap: 0.5rem; font-size: 12.5px; }
  .facts span { color: var(--mdx-text-muted); }
  .facts b { color: var(--mdx-text-secondary); font-weight: 500; }
  .mono { font-family: var(--mdx-font-mono, monospace); font-size: 11px; word-break: break-all; }
</style>
