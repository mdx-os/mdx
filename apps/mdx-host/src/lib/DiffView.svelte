<script>
  // Shared diff viewer (Slick Rails primitive). Renders the structured diff for a
  // finished run from POST /forge/run-diff.json -> files[].hunks[].lines[], colored
  // by kind (add/remove/context/meta). This is the "see the actual change" moment:
  // exactly what Forge wrote, in a safe copy, before anything ships. The first file
  // opens by default; the rest expand on click. Consumed by the first mission and,
  // by the same source as Review, anywhere a run's change needs to be shown.
  let { runId = "", ready = false } = $props();

  let status = $state("idle"); // idle | loading | ready | empty | error
  let files = $state([]);
  let branch = $state("");
  let lastKey = "";

  $effect(() => {
    if (!ready || !runId || runId === lastKey) return;
    lastKey = runId;
    load(runId);
  });

  async function load(id) {
    status = "loading";
    try {
      const r = await fetch("/api/kernel/forge/run-diff.json", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ run_id: id }),
        signal: AbortSignal.timeout(4000),
      });
      if (!r.ok) { status = "error"; return; }
      const data = await r.json();
      files = (data.files ?? []).map((f, i) => ({ ...f, open: i === 0 }));
      branch = data.branch ?? "";
      status = files.length ? "ready" : "empty";
    } catch (error) {
      status = "error";
    }
  }

  const toggle = (i) => { files = files.map((f, j) => (j === i ? { ...f, open: !f.open } : f)); };
  const totalAdded = $derived(files.reduce((n, f) => n + (f.added ?? 0), 0));
  const totalRemoved = $derived(files.reduce((n, f) => n + (f.removed ?? 0), 0));
  const gutter = (k) => (k === "add" ? "+" : k === "remove" ? "-" : "");
</script>

{#if ready && status !== "idle"}
  <div class="dv">
    <div class="dv-top">
      <span class="dv-title">Here's exactly what it changed</span>
      {#if branch}<span class="dv-branch">{branch}</span>{/if}
      {#if status === "ready"}<span class="dv-stat"><span class="dv-add">+{totalAdded}</span> <span class="dv-rm">-{totalRemoved}</span></span>{/if}
    </div>

    {#if status === "loading"}
      <p class="dv-hint">Reading the diff...</p>
    {:else if status === "empty"}
      <p class="dv-hint">The proof passed - this run didn't need to touch any files.</p>
    {:else if status === "error"}
      <p class="dv-hint">Couldn't load the diff just now. It's still safe on the branch above.</p>
    {:else}
      <ul class="dv-files">
        {#each files as f, i (f.path)}
          <li class="dv-file" class:open={f.open}>
            <button type="button" class="dv-file-head" onclick={() => toggle(i)} aria-expanded={f.open}>
              <span class="dv-caret" aria-hidden="true">{f.open ? "▾" : "▸"}</span>
              <span class="dv-path">{f.path}</span>
              {#if f.agent_confidence === "needs_attention"}
                <span class="dv-sig dv-sig-warn" title="{f.error_step_count ?? 0} recorded step{(f.error_step_count ?? 0) === 1 ? "" : "s"} hit an error near this file">look here first</span>
              {:else if f.agent_confidence === "checked"}
                <span class="dv-sig dv-sig-ok" title="Checks touched this file while it was being written">checked</span>
              {/if}
              <span class="dv-file-stat"><span class="dv-add">+{f.added ?? 0}</span> <span class="dv-rm">-{f.removed ?? 0}</span></span>
            </button>
            {#if f.open}
              <div class="dv-body">
                {#each f.hunks ?? [] as h (h.header)}
                  <div class="dv-hunk-head">{h.header}</div>
                  {#each h.lines ?? [] as line}
                    <div class="dv-line" data-kind={line.kind}><span class="dv-gutter">{gutter(line.kind)}</span><span class="dv-text">{line.text}</span></div>
                  {/each}
                {/each}
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
{/if}

<style>
  .dv { margin: 20px 0 0; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-md); background: var(--mdx-surface-base); overflow: hidden; }
  .dv-top { display: flex; align-items: center; gap: 10px; padding: 12px 14px; border-bottom: 1px solid var(--mdx-border-subtle); }
  .dv-title { flex: 1; font-size: 13px; font-weight: 650; color: var(--mdx-text-primary); }
  .dv-branch { font-family: var(--mdx-font-mono, monospace); font-size: 12px; color: var(--mdx-text-primary); padding: 2px 8px; border-radius: var(--mdx-radius-md); background: var(--mdx-surface-raised); border: 1px solid var(--mdx-border-subtle); }
  .dv-stat, .dv-file-stat { flex: none; font-family: var(--mdx-font-mono, monospace); font-size: 12px; white-space: nowrap; }
  .dv-sig { flex: none; font-size: 11px; font-weight: 650; padding: 1px 7px; border-radius: 999px; white-space: nowrap; }
  .dv-sig-warn { color: var(--mdx-accent-warning, #b45309); background: color-mix(in srgb, var(--mdx-accent-warning, #b45309) 12%, transparent); }
  .dv-sig-ok { color: var(--mdx-accent-success); background: color-mix(in srgb, var(--mdx-accent-success) 12%, transparent); }
  .dv-add { color: var(--mdx-accent-success); font-weight: 700; }
  .dv-rm { color: var(--mdx-accent-error); font-weight: 700; }
  .dv-hint { margin: 0; padding: 14px; font-size: 13px; color: var(--mdx-text-secondary); line-height: 1.5; }
  .dv-files { list-style: none; margin: 0; padding: 0; }
  .dv-file:not(:last-child) { border-bottom: 1px solid var(--mdx-border-subtle); }
  .dv-file-head { display: flex; align-items: center; gap: 9px; width: 100%; padding: 10px 14px; border: none; background: none; cursor: pointer; text-align: left; font: inherit; }
  .dv-file-head:hover { background: var(--mdx-surface-raised); }
  .dv-caret { flex: none; width: 12px; color: var(--mdx-text-muted); font-size: 11px; }
  .dv-path { flex: 1; min-width: 0; font-family: var(--mdx-font-mono, monospace); font-size: 12.5px; color: var(--mdx-text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dv-body { max-height: 320px; overflow: auto; border-top: 1px solid var(--mdx-border-subtle); background: var(--mdx-surface-raised); }
  .dv-hunk-head { padding: 5px 14px; font-family: var(--mdx-font-mono, monospace); font-size: 11px; color: var(--mdx-text-muted); background: color-mix(in srgb, var(--mdx-accent-primary) 6%, transparent); border-top: 1px solid var(--mdx-border-subtle); }
  .dv-line { display: flex; font-family: var(--mdx-font-mono, monospace); font-size: 12px; line-height: 1.6; }
  .dv-gutter { flex: none; width: 22px; text-align: center; color: var(--mdx-text-muted); user-select: none; }
  .dv-text { flex: 1; min-width: 0; white-space: pre-wrap; word-break: break-word; padding-right: 12px; color: var(--mdx-text-secondary); }
  .dv-line[data-kind="add"] { background: color-mix(in srgb, var(--mdx-accent-success) 12%, transparent); }
  .dv-line[data-kind="add"] .dv-gutter, .dv-line[data-kind="add"] .dv-text { color: var(--mdx-accent-success); }
  .dv-line[data-kind="remove"] { background: color-mix(in srgb, var(--mdx-accent-error) 11%, transparent); }
  .dv-line[data-kind="remove"] .dv-gutter, .dv-line[data-kind="remove"] .dv-text { color: var(--mdx-accent-error); }
  .dv-line[data-kind="context"] .dv-text { color: var(--mdx-text-secondary); }
</style>
