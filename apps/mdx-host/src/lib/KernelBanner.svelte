<script>
  // The one honest kernel banner, shell-level. When the local kernel stops
  // answering, every surface says so in the same calm voice - offline in red,
  // slow-but-alive in amber - instead of each page inventing its own dot or,
  // worse, rendering a normal greeting over a dead kernel. Retry re-reads the
  // page data and re-probes; Diagnose discloses the base URL, the failed
  // streak, and the last real error for anyone who wants the detail.
  import { invalidateAll } from "$app/navigation";
  import { kernelReachability, retryReachability } from "./kernelReachability.js";

  let showDetail = $state(false);
  let retrying = $state(false);

  const state = $derived($kernelReachability);
  // Offline: the kernel is not answering. Degraded: it answered slowly or
  // missed a beat but is still reachable. Healthy surfaces render nothing.
  const mode = $derived(!state.online ? "offline" : state.degraded ? "degraded" : "");

  async function retry() {
    if (retrying) return;
    retrying = true;
    try {
      await Promise.all([retryReachability(), invalidateAll()]);
    } finally {
      retrying = false;
    }
  }
</script>

{#if mode}
  <div class="kbanner" data-mode={mode} role="status" aria-live="polite">
    <span class="kb-dot" aria-hidden="true"></span>
    <div class="kb-body">
      <p class="kb-line">
        {#if mode === "offline"}
          Can't reach the MDx kernel. What's on screen may be out of date, and new actions won't go through until it's back.
        {:else}
          The MDx kernel is answering slowly. Live updates may lag for a moment.
        {/if}
      </p>
      {#if showDetail}
        <dl class="kb-detail">
          <div><dt>Kernel</dt><dd>{state.baseUrl || "local proxy"}</dd></div>
          <div><dt>Checks failed in a row</dt><dd>{state.failedStreak}</dd></div>
          {#if state.lastError}<div><dt>Last error</dt><dd>{state.lastError}</dd></div>{/if}
        </dl>
      {/if}
    </div>
    <div class="kb-actions">
      <button type="button" class="kb-retry" onclick={retry} disabled={retrying}>
        {retrying ? "Retrying..." : "Retry"}
      </button>
      <button type="button" class="kb-diag" onclick={() => (showDetail = !showDetail)} aria-expanded={showDetail}>
        {showDetail ? "Hide detail" : "Diagnose"}
      </button>
    </div>
  </div>
{/if}

<style>
  .kbanner {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--mdx-border-subtle);
    font-size: 13px;
    color: var(--mdx-text-primary);
    background: var(--mdx-surface-raised);
  }
  .kbanner[data-mode="offline"] {
    background: color-mix(in srgb, var(--mdx-accent-error) 12%, var(--mdx-surface-base));
    border-bottom-color: color-mix(in srgb, var(--mdx-accent-error) 40%, transparent);
  }
  .kbanner[data-mode="degraded"] {
    background: color-mix(in srgb, var(--mdx-accent-warning, #b45309) 12%, var(--mdx-surface-base));
    border-bottom-color: color-mix(in srgb, var(--mdx-accent-warning, #b45309) 40%, transparent);
  }
  .kb-dot {
    flex: none;
    width: 9px;
    height: 9px;
    margin-top: 5px;
    border-radius: 50%;
  }
  .kbanner[data-mode="offline"] .kb-dot { background: var(--mdx-accent-error); }
  .kbanner[data-mode="degraded"] .kb-dot {
    background: var(--mdx-accent-warning, #b45309);
    animation: kbpulse 1.4s ease-in-out infinite;
  }
  @keyframes kbpulse { 0%, 100% { opacity: 0.4; } 50% { opacity: 1; } }
  @media (prefers-reduced-motion: reduce) {
    .kbanner[data-mode="degraded"] .kb-dot { animation: none; }
  }
  .kb-body { flex: 1; min-width: 0; }
  .kb-line { margin: 0; line-height: 1.5; }
  .kb-detail {
    margin: 8px 0 0;
    display: grid;
    gap: 4px;
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-muted);
  }
  .kb-detail div { display: flex; gap: 8px; }
  .kb-detail dt { font-weight: 600; min-width: 140px; }
  .kb-detail dd { margin: 0; font-family: var(--mdx-font-mono, monospace); word-break: break-all; }
  .kb-actions { flex: none; display: flex; gap: 8px; align-items: center; }
  .kb-retry,
  .kb-diag {
    font: inherit;
    font-size: 12.5px;
    padding: 5px 12px;
    border-radius: var(--mdx-radius-full, 999px);
    cursor: pointer;
    border: 1px solid var(--mdx-border-default);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-primary);
  }
  .kb-retry { border-color: transparent; background: var(--mdx-accent-primary); color: var(--mdx-on-accent, #fff); }
  .kb-retry:disabled { opacity: 0.6; cursor: default; }
  .kb-diag:hover,
  .kb-retry:not(:disabled):hover { filter: brightness(1.05); }
</style>
