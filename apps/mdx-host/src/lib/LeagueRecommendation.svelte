<script>
  // The recommendation at work start, compact and honest. Forge is the subject:
  // it names the builder it will use and offers a held trial of an outside
  // machine when there is reason to. This is Forge's current stance, not a
  // per-task promise - the rationale stays honest when evidence is still thin.
  // Renders nothing until the recommendation loads, so it never blocks the
  // composer. Full roster and the learning loop live behind the link.
  let { rec = null } = $props();

  const selected = $derived(rec?.selected_runner_display_name || "");
  const fallback = $derived(rec?.fallback_runner_display_name || "");
  const stillLearning = $derived(rec?.confidence === "gathering_evidence");
</script>

{#if selected}
  <div class="league-rec">
    <span class="lr-eyebrow">Forge will build this with</span>
    <span class="lr-runner">{selected}</span>
    {#if fallback}
      <span class="lr-fallback">When there is reason to, it can stage a held trial with {fallback} and keep what works.</span>
    {/if}
    <a class="lr-link" href="/forge/models/league">Machines &rarr;</a>
  </div>
{/if}

<style>
  .league-rec {
    display: flex; align-items: center; gap: 8px; flex-wrap: wrap;
    padding: 9px 13px; border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-accent-primary) 5%, transparent);
    border: 1px solid var(--mdx-border-subtle);
    font-size: 12.5px;
  }
  .lr-eyebrow { color: var(--mdx-text-muted); }
  .lr-runner { font-weight: 650; color: var(--mdx-text-primary); }
  .lr-fallback { color: var(--mdx-text-muted); flex: 1; min-width: 200px; line-height: 1.5; }
  .lr-link { flex: none; color: var(--mdx-accent-primary); text-decoration: none; font-weight: 600; }
  .lr-link:hover { text-decoration: underline; }
</style>
