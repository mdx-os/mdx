<script>
  let { data, form } = $props();
  const models = $derived(data.models);
  const defaultConnectionMode = $derived(models.accessPolicy.modes[0] ?? "byok");
  const routeReady = (connection, deployment) => ["healthy", "observed"].includes(connection?.health)
    && connection?.credential_available !== false
    && deployment.enabled
    && connection.data_retention !== "unknown"
    && connection.training_policy !== "unknown";
  const assignedDeployment = (workloadId) => models.policies
    .filter((policy) => policy.state === "promoted" && policy.workload_ids?.includes(workloadId))
    .sort((left, right) => left.policy_version.localeCompare(right.policy_version, undefined, { numeric: true }))
    .at(-1)?.preferred_deployment_ids?.[0] ?? "";
</script>

<svelte:head><title>Models - MDx Console</title></svelte:head>

<section class="models" data-route-state="ready">
  <header class="mdx-page-head">
    <h1>Models</h1>
    <p class="mdx-page-sub">Connect models once. MDx chooses among them for each kind of work.</p>
  </header>

  <section class="readiness" data-ready={models.readiness.ready}>
    <div>
      <p class="eyebrow">Model access</p>
      <h2>{models.readiness.ready ? `${models.readiness.model} is ready` : models.readiness.status.replaceAll("_", " ").toLowerCase()}</h2>
      <p>{models.readiness.nextAction}</p>
      {#if models.readiness.ready && models.readiness.surfaceCount > 0}
        <p class="surface-readiness">{models.readiness.readySurfaces} of {models.readiness.surfaceCount} app areas have eligible work.</p>
      {/if}
    </div>
    <span class="mdx-badge" data-tone={models.readiness.ready ? "ok" : "bydesign"}>
      {models.readiness.ready ? "Ready for supported work" : "Needs setup"}
    </span>
  </section>

  {#if form}
    <p class="action-result" data-ok={form.ok}>{form.line}</p>
  {/if}

  <div class="workspace">
    <main>
      <section class="section-head">
        <div>
          <h2>Your models</h2>
          <p>{models.deployments.length} {models.deployments.length === 1 ? "deployment" : "deployments"} available to the router</p>
        </div>
      </section>

      {#if models.deployments.length === 0}
        <section class="empty-state">
          <h2>No models connected yet</h2>
          <p>Add one model with the form beside this list. MDx verifies the connection before it becomes available, and keeps the key out of receipts and app state.</p>
        </section>
      {:else}
        <div class="deployment-list">
          {#each models.deployments as deployment}
            {@const connection = models.connected.find((item) => item.connection_id === deployment.connection_id)}
            {@const ready = routeReady(connection, deployment)}
            <article class="deployment">
              <div class="deployment-main">
                <div>
                  <p class="eyebrow">{deployment.provider_id}</p>
                  <h3>{deployment.model_id.replace(`${deployment.provider_id}/`, "")}</h3>
                </div>
                <span class="mdx-badge" data-tone={ready ? "ok" : "bydesign"}>
                  {ready ? "Ready" : "Held"}
                </span>
              </div>
              {#if connection?.data_retention === "unknown" || connection?.training_policy === "unknown"}
                <p class="hold-note">Confirm this provider's no-storage and no-training controls before MDx routes work here.</p>
              {/if}
              <div class="facts">
                <span>{deployment.privacy_class}</span>
                <span>{deployment.region}</span>
                <span>{deployment.capabilities.tools ? "tools" : "text"}</span>
                {#if deployment.capabilities.structured_output}<span>structured output</span>{/if}
                {#if connection?.credential_source}<span>{connection.credential_source.replaceAll("_", " ")}</span>{/if}
              </div>
              {#if ready}
                <form method="POST" action="?/test" class="test-row">
                  <input type="hidden" name="deployment_id" value={deployment.deployment_id} />
                  <input type="hidden" name="max_cost_microusd" value="1000" />
                  <button type="submit">Run a capped live test</button>
                  <span>Maximum 1,000 micro-USD</span>
                </form>
              {/if}
              <div class="connection-actions">
                <form method="POST" action="?/disconnect">
                  <input type="hidden" name="provider_id" value={deployment.provider_id} />
                  <input type="hidden" name="connection_action" value="disconnect" />
                  <button type="submit">Disconnect</button>
                </form>
                <details>
                  <summary>Rotate or revoke</summary>
                  <form method="POST" action="?/connect" class="rotate-form">
                    <input type="hidden" name="provider_id" value={deployment.provider_id} />
                    <input type="hidden" name="model_id" value={deployment.model_id.replace(`${deployment.provider_id}/`, "")} />
                    <input type="hidden" name="connection_mode" value="byok" />
                    <input type="hidden" name="persist_key" value="on" />
                    <label>Replacement key<input name="api_key" type="password" required autocomplete="off" /></label>
                    <button type="submit">Verify and rotate</button>
                  </form>
                  <form method="POST" action="?/disconnect" class="revoke-form">
                    <input type="hidden" name="provider_id" value={deployment.provider_id} />
                    <input type="hidden" name="connection_action" value="revoke" />
                    <button type="submit">Revoke and forget local key</button>
                  </form>
                </details>
              </div>
              <details>
                <summary>View model record</summary>
                <dl>
                  <div><dt>Deployment</dt><dd><code>{deployment.deployment_id}</code></dd></div>
                  <div><dt>Data retention</dt><dd>{connection?.data_retention ?? "unknown"}</dd></div>
                  <div><dt>Training</dt><dd>{connection?.training_policy === "none" ? "not used by default" : connection?.training_policy ?? "unknown"}</dd></div>
                  <div><dt>Residency</dt><dd>{deployment.residency}</dd></div>
                  <div><dt>Price source</dt><dd>{deployment.price?.source ?? "unknown"}</dd></div>
                  <div><dt>Capability source</dt><dd>{deployment.capabilities.provenance}</dd></div>
                </dl>
              </details>
            </article>
          {/each}
        </div>
      {/if}

      <section class="routing">
        <div class="section-head">
          <div><h2>How work is routed</h2><p>Apps name the job. Policy filters first, then the preset ranks eligible models.</p></div>
        </div>
        <div class="preset-grid">
          {#each models.fabric.presets as preset}
            <article class:active={preset.id === "balanced"}>
              <strong>{preset.label}</strong>
              <span>{preset.summary}</span>
              {#if preset.id === "balanced"}<small>Default</small>{/if}
            </article>
          {/each}
        </div>
        <details>
          <summary>Choose models for app work</summary>
          <div class="assignment-list">
            {#each models.fabric.workloads as workload}
              <form method="POST" action="?/assign" class="assignment-row">
                <input type="hidden" name="workload_id" value={workload.workload_id} />
                <label>
                  <span><strong>{workload.surface}</strong> {workload.stage.replaceAll("_", " ")}</span>
                  <select name="deployment_id" value={assignedDeployment(workload.workload_id)}>
                    <option value="">Automatic - {workload.default_preset}</option>
                    {#each models.deployments as deployment}
                      <option value={deployment.deployment_id}>{deployment.model_id.replace(`${deployment.provider_id}/`, "")}</option>
                    {/each}
                  </select>
                </label>
                <button type="submit" disabled={models.deployments.length === 0}>Save</button>
              </form>
            {/each}
          </div>
        </details>
      </section>
    </main>

    <aside>
      <form method="POST" action="?/connect" class="connect-card">
        <div>
          <p class="eyebrow">Add to MDx</p>
          <h2>Connect a model</h2>
          <p>Choose how this environment supplies access. MDx verifies the connection before routing work.</p>
        </div>
        <label>Provider
          <select name="provider_id" required>
            <option value="">Choose provider</option>
            {#each models.fabric.providers as provider}
              <option value={provider.provider_id} disabled={!provider.connectable || !models.accessPolicy.providers.includes(provider.provider_id)}>{provider.label}{!models.accessPolicy.providers.includes(provider.provider_id) ? " - blocked by policy" : provider.connectable ? "" : " - catalog only"}</option>
            {/each}
          </select>
        </label>
        <label>Connection
          <select name="connection_mode">
            <option value="byok" selected={defaultConnectionMode === "byok"} disabled={!models.accessPolicy.modes.includes("byok")}>Use my provider key</option>
            <option value="managed" selected={defaultConnectionMode === "managed"} disabled={!models.accessPolicy.modes.includes("managed")}>Use MDx managed models</option>
            <option value="enterprise" selected={defaultConnectionMode === "enterprise"} disabled={!models.accessPolicy.modes.includes("enterprise")}>Use an operator-managed secret</option>
            <option value="local" selected={defaultConnectionMode === "local"} disabled={!models.accessPolicy.modes.includes("local")}>Use a local model server</option>
          </select>
        </label>
        <label>Model ID - optional<input name="model_id" placeholder="discover a recommended model" autocomplete="off" /></label>
        <details open>
          <summary>Access details</summary>
          <label>Provider key - personal key mode<input name="api_key" type="password" placeholder="never stored in receipts" autocomplete="off" /></label>
          <label class="check"><input name="persist_key" type="checkbox" checked /> Remember securely on this machine</label>
          <label>Secret reference - operator-managed mode<input name="credential_ref" placeholder="secret:environment/OPENAI_API_KEY" autocomplete="off" /></label>
          <small class="field-help">Leave the reference blank to use this provider's deployment environment secret.</small>
        </details>
        <details class="advanced">
          <summary>Advanced settings</summary>
          <label>Custom endpoint<input name="endpoint_base_url" type="url" placeholder="use provider default" /></label>
          <label>Context window<input name="context_tokens" type="number" min="0" value="0" /></label>
          <label class="check"><input name="tools" type="checkbox" checked /> Tool calling</label>
          <label class="check"><input name="structured_output" type="checkbox" checked /> Structured output</label>
          <label class="check"><input name="vision" type="checkbox" /> Vision</label>
          <label class="check"><input name="paid_service_data_terms" type="checkbox" /> Paid Gemini project</label>
          <label class="check"><input name="privacy_controls_attested" type="checkbox" /> Provider no-storage and no-training controls are enabled</label>
        </details>
        <button class="primary" type="submit">Connect model</button>
        <small class="field-help">Allowed here: {models.accessPolicy.modes.join(", ") || "none"}. Live tests are capped at {models.accessPolicy.maxLiveTestMicrousd.toLocaleString()} micro-USD by tenant policy.</small>
      </form>
      <p class="boundary">{data.boundary}</p>
    </aside>
  </div>

  <footer>
    <details><summary>View routing record</summary><p>{models.fabric.providerCount} provider adapters and {models.fabric.workloads.length} workload aliases are declared. {data.safeNext}</p></details>
  </footer>
</section>

<style>
  .models { display:grid; gap:22px; max-width:1120px; margin:0 auto; padding:4vh 0 44px; }
  .workspace { display:grid; grid-template-columns:minmax(0,1fr) 330px; gap:20px; align-items:start; }
  main { display:grid; gap:24px; }
  aside { position:sticky; top:20px; display:grid; gap:10px; }
  h2,h3,p,dl,dd { margin:0; }
  h2,h3 { font-family:var(--mdx-font-display); }
  h2 { font-size:18px; } h3 { font-size:16px; }
  .section-head { display:flex; justify-content:space-between; align-items:end; }
  .section-head p,.connect-card p,.empty-state p,.boundary,footer p { color:var(--mdx-text-muted); font-size:13px; line-height:1.5; }
  .deployment-list { display:grid; gap:10px; }
  .deployment,.routing,.connect-card,.empty-state,.readiness { border:1px solid var(--mdx-border-subtle); border-radius:var(--mdx-radius-xl); background:var(--mdx-surface-raised); }
  .readiness { display:flex; justify-content:space-between; align-items:center; gap:16px; padding:16px 18px; }
  .readiness[data-ready="true"] { border-color:color-mix(in srgb,var(--mdx-tone-ok-text) 35%,var(--mdx-border-subtle)); }
  .readiness div { display:grid; gap:4px; }
  .readiness p { margin:0; color:var(--mdx-text-muted); font-size:12px; }
  .deployment { display:grid; gap:12px; padding:16px; }
  .deployment-main { display:flex; justify-content:space-between; gap:12px; }
  .facts { display:flex; flex-wrap:wrap; gap:6px; }
  .hold-note { color:var(--mdx-tone-warning-text); font-size:12px; }
  .facts span { padding:4px 8px; border-radius:999px; background:var(--mdx-surface); color:var(--mdx-text-muted); font-size:11px; }
  .test-row { display:flex; align-items:center; gap:10px; }
  .test-row button { padding:8px 10px; border:1px solid var(--mdx-border-subtle); border-radius:var(--mdx-radius-md); background:var(--mdx-surface); color:var(--mdx-text-primary); cursor:pointer; }
  .test-row span { color:var(--mdx-text-muted); font-size:11px; }
  .connection-actions { display:flex; align-items:start; gap:10px; }
  .connection-actions button,.rotate-form button,.revoke-form button { padding:8px 10px; border:1px solid var(--mdx-border-subtle); border-radius:var(--mdx-radius-md); background:var(--mdx-surface); color:var(--mdx-text-primary); cursor:pointer; }
  .connection-actions details { padding-top:7px; }
  .rotate-form,.revoke-form { display:grid; gap:8px; min-width:250px; margin-top:10px; }
  .revoke-form button { color:var(--mdx-tone-warning-text); }
  details summary { cursor:pointer; color:var(--mdx-text-secondary); font-size:12px; }
  dl { display:grid; gap:7px; margin-top:10px; }
  dl div { display:grid; grid-template-columns:120px 1fr; gap:10px; font-size:12px; }
  dt,.eyebrow { color:var(--mdx-text-muted); font-size:10px; text-transform:uppercase; }
  code { font-family:var(--mdx-font-mono); overflow-wrap:anywhere; }
  .empty-state { display:grid; gap:8px; padding:28px; }
  .routing { display:grid; gap:16px; padding:18px; }
  .preset-grid { display:grid; grid-template-columns:repeat(4,minmax(0,1fr)); gap:8px; }
  .preset-grid article { display:grid; gap:5px; padding:11px; border:1px solid var(--mdx-border-subtle); border-radius:var(--mdx-radius-md); background:var(--mdx-surface); }
  .preset-grid article.active { border-color:var(--mdx-accent); }
  .preset-grid span,.preset-grid small { color:var(--mdx-text-muted); font-size:11px; line-height:1.4; }
  .preset-grid small { color:var(--mdx-accent); }
  .assignment-list { display:grid; gap:7px; margin-top:12px; max-height:390px; overflow:auto; }
  .assignment-row { display:grid; grid-template-columns:minmax(0,1fr) auto; gap:8px; align-items:end; padding:8px; border:1px solid var(--mdx-border-subtle); border-radius:var(--mdx-radius-md); }
  .assignment-row label span { color:var(--mdx-text-muted); font-size:11px; text-transform:capitalize; }
  .assignment-row button { padding:9px 11px; border:1px solid var(--mdx-border-subtle); border-radius:var(--mdx-radius-md); background:var(--mdx-surface); color:var(--mdx-text-primary); cursor:pointer; }
  .assignment-row button:disabled { cursor:not-allowed; opacity:.5; }
  .connect-card { display:grid; gap:14px; padding:18px; }
  .connect-card > div { display:grid; gap:5px; }
  label { display:grid; gap:6px; color:var(--mdx-text-secondary); font-size:12px; }
  input,select { width:100%; box-sizing:border-box; padding:10px; border:1px solid var(--mdx-border-subtle); border-radius:var(--mdx-radius-md); background:var(--mdx-surface); color:var(--mdx-text-primary); font:inherit; }
  .advanced { display:grid; gap:10px; }
  .advanced[open] summary { margin-bottom:10px; }
  .advanced label { margin-top:8px; }
  .check { display:flex !important; align-items:center; gap:8px; }
  .check input { width:auto; }
  .field-help { color:var(--mdx-text-muted); font-size:11px; line-height:1.45; }
  button.primary { width:100%; padding:11px 14px; border:0; border-radius:var(--mdx-radius-md); background:var(--mdx-accent); color:white; font-weight:700; cursor:pointer; }
  .boundary { padding:0 4px; }
  .action-result { padding:11px 14px; border-radius:var(--mdx-radius-md); background:var(--mdx-surface-raised); border:1px solid var(--mdx-border-subtle); font-size:13px; }
  .action-result[data-ok="true"] { color:var(--mdx-tone-ok-text); }
  footer { border-top:1px solid var(--mdx-border-subtle); padding-top:14px; }
  @media(max-width:860px){ .workspace{grid-template-columns:1fr}.preset-grid{grid-template-columns:1fr 1fr}aside{position:static} }
</style>
