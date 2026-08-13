<script>
  import { page } from "$app/state";
</script>

<section class="error-boundary" data-route-state="error">
  <h1>Something broke on this page.</h1>
  <p>
    The rest of MDx is fine - this one page hit an error and stopped instead of taking anything down with it.
  </p>
  <div class="error-actions">
    <a class="mdx-btn primary" href="/">Back to home</a>
    <button type="button" class="mdx-btn" onclick={() => location.reload()}>Reload this page</button>
  </div>
  <details class="error-detail">
    <summary>Error detail</summary>
    <pre>{page.error?.message ?? "Unknown error"}</pre>
  </details>
</section>

<style>
  .error-boundary {
    max-width: 760px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    padding: 24px;
    background: var(--mdx-surface-raised);
  }

  h1 {
    margin: 0 0 10px;
    font-size: 28px;
  }

  .error-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    margin: 18px 0 4px;
  }

  .error-detail {
    margin-top: 14px;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-sm);
  }
  .error-detail summary {
    cursor: pointer;
  }

  pre {
    white-space: pre-wrap;
    color: var(--mdx-text-muted);
    margin: 8px 0 0;
  }
</style>
