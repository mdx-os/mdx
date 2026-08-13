<script>
  // One receipt, human first: what happened, who did it, when, in which
  // loop - with the raw record one disclosure away. Linked from every
  // receipt id in the product.
  let { data } = $props();
  const receipt = $derived(data.receipt);

  function humanWhen(iso) {
    const raw = String(iso ?? "");
    const date = new Date(raw);
    if (Number.isNaN(date.getTime())) return raw;
    return date.toLocaleString(undefined, {
      year: "numeric", month: "short", day: "numeric", hour: "2-digit", minute: "2-digit"
    });
  }

  function human(value) {
    return String(value ?? "")
      .replaceAll("_", " ")
      .replace(/^\w/, (c) => c.toUpperCase());
  }
  const knownRows = $derived(
    receipt
      ? [
          ["What", human(receipt.loop_id ?? receipt.receipt_kind ?? receipt.kind ?? "")],
          ["Who", String(receipt.actor_id ?? "")],
          ["When", humanWhen(receipt.receipt_timestamp ?? receipt.recorded_at ?? receipt.created_at)],
          ["Workflow", String(receipt.workflow_id ?? "")],
          ["Trace", String(receipt.trace_id ?? "")],
          ["Tenant", String(receipt.tenant_id ?? "")]
        ].filter(([, value]) => value)
      : []
  );
</script>

<main class="evidence">
  <h1>Evidence</h1>
  <p class="sub"><code>{data.receiptId}</code></p>

  {#if receipt}
    <section class="card" aria-label="Receipt summary">
      {#each knownRows as [label, value] (label)}
        <div class="row"><span class="k">{label}</span><span class="v">{value}</span></div>
      {/each}
    </section>
    <details class="raw">
      <summary>Full record</summary>
      <pre>{JSON.stringify(receipt, null, 2)}</pre>
    </details>
  {:else}
    <section class="card">
      <p class="miss">This receipt could not be read right now - it may predate this kernel's ledger, or MDx may be unreachable. The id above is still the durable reference.</p>
    </section>
  {/if}
</main>

<style>
  .evidence { max-width: 720px; margin: 0 auto; padding: 40px 24px 60px; }
  h1 { font-size: 22px; margin: 0 0 4px; }
  .sub { margin: 0 0 18px; }
  .sub code { font-family: var(--mdx-font-mono, monospace); font-size: 13px; color: var(--mdx-text-secondary); }
  .card { border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-md); background: var(--mdx-surface-base); padding: 14px 16px; }
  .row { display: flex; gap: 14px; padding: 5px 0; font-size: 13.5px; }
  .row + .row { border-top: 1px solid var(--mdx-border-subtle); }
  .k { flex: 0 0 90px; color: var(--mdx-text-secondary); }
  .v { color: var(--mdx-text-primary); word-break: break-all; }
  .miss { margin: 0; font-size: 13.5px; color: var(--mdx-text-secondary); line-height: 1.55; }
  .raw { margin-top: 14px; }
  .raw summary { cursor: pointer; font-size: 13px; color: var(--mdx-text-secondary); }
  .raw pre { margin: 10px 0 0; padding: 12px; border-radius: var(--mdx-radius-md); background: var(--mdx-surface-raised); border: 1px solid var(--mdx-border-subtle); font-size: 11.5px; overflow: auto; max-height: 420px; }
</style>
