<script>
  // Beta onboarding: the first ten minutes as a real, honest, resumable
  // checklist, beside a scope-honesty panel so blocked states read as
  // intentional. Completing a step records a Source-B activation_step event
  // (safe fields only); receipt-backed completion restores across devices and
  // local state keeps the UI immediate while the event is in flight.
  import { onMount } from "svelte";
  import { activationStepEvent, telemetry } from "../../../lib/telemetry.js";

  let { data } = $props();

  let done = $state({});
  const STORE = "mdx-beta-onboarding";

  onMount(() => {
    const receiptBacked = Object.fromEntries((data.completedSteps ?? []).map((step) => [step, true]));
    try {
      done = { ...receiptBacked, ...JSON.parse(localStorage.getItem(STORE) ?? "{}") };
    } catch (error) {
      done = receiptBacked;
    }
  });

  const completedCount = $derived(data.steps.filter((s) => done[s.id]).length);

  function markDone(step) {
    if (done[step.id]) return;
    done = { ...done, [step.id]: true };
    try {
      localStorage.setItem(STORE, JSON.stringify(done));
    } catch (error) {
      // best-effort persistence
    }
    telemetry.record(activationStepEvent({ step: step.id, completed: true, route: "/welcome/beta" }));
    telemetry.flush();
  }
</script>

<svelte:head><title>Welcome to the beta - MDx</title></svelte:head>

<section class="wb" data-route-state="ready">
  <header class="wb-head">
    <span class="eyebrow">Beta</span>
    <h1>Welcome. Here's the short path in.</h1>
    <p class="sub">Eight small steps. Do them in any order. {completedCount} of {data.steps.length} done.</p>
    <p class="sub"><a class="meet-link" href="/welcome/meet">New here? Meet the apps first</a></p>
  </header>

  <div class="wb-grid">
    <ol class="steps" aria-label="Getting started">
      {#each data.steps as step, i (step.id)}
        <li class="step" class:done={done[step.id]}>
          <span class="num" aria-hidden="true">{done[step.id] ? "✓" : i + 1}</span>
          <div class="step-body">
            <p class="step-title">{step.title}</p>
            <p class="step-text">{step.body}</p>
            <div class="step-acts">
              <a class="go" href={step.href}>Open</a>
              <button type="button" class="mark" onclick={() => markDone(step)} disabled={done[step.id]}>
                {done[step.id] ? "Done" : "Mark done"}
              </button>
            </div>
          </div>
        </li>
      {/each}
    </ol>

    <aside class="scope" aria-label="What's in beta">
      <section class="scope-card">
        <p class="scope-head">In beta now</p>
        <ul>{#each data.scope.inBeta as line (line)}<li>{line}</li>{/each}</ul>
      </section>
      <section class="scope-card">
        <p class="scope-head">Gated on purpose</p>
        <ul class="gated">
          {#each data.scope.gated as g (g.line)}
            <li class:held={g.on}><span class="lock" aria-hidden="true">{g.on ? "✓" : "·"}</span>{g.line}</li>
          {/each}
        </ul>
        <p class="scope-foot">{data.curatedNote}</p>
      </section>
      <section class="scope-card">
        <p class="scope-head">Coming later</p>
        <ul class="coming">{#each data.scope.coming as line (line)}<li>{line}</li>{/each}</ul>
      </section>
    </aside>
  </div>

  <p class="quiet">{data.boundary} Safe next: {data.safeNext}</p>
</section>

<style>
  .wb { max-width: 1000px; margin: 0 auto; padding: 4vh 16px 40px; }
  .eyebrow { font-size: 11px; font-weight: 650; letter-spacing: 0.06em; text-transform: uppercase; color: var(--mdx-accent-primary); }
  .wb-head h1 { margin: 6px 0 0; font-size: 26px; }
  .sub { margin: 6px 0 0; color: var(--mdx-text-secondary); font-size: 13.5px; }
  .meet-link { color: var(--mdx-accent-primary); text-decoration: none; }

  .wb-grid { margin: 22px 0 0; display: grid; grid-template-columns: minmax(0, 1.5fr) minmax(0, 1fr); gap: 18px; align-items: start; }

  .steps { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
  .step { display: flex; gap: 12px; padding: 14px; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-lg); background: var(--mdx-surface-raised); }
  .step.done { opacity: 0.7; }
  .num { flex: none; width: 26px; height: 26px; border-radius: 50%; display: grid; place-items: center; font-size: 13px; font-weight: 650; background: var(--mdx-surface-base); color: var(--mdx-text-secondary); }
  .step.done .num { background: var(--mdx-accent-success, var(--mdx-success-green)); color: #fff; }
  .step-body { min-width: 0; }
  .step-title { margin: 0; font-size: 14.5px; font-weight: 600; color: var(--mdx-text-primary); }
  .step-text { margin: 3px 0 0; font-size: 12.5px; color: var(--mdx-text-muted); }
  .step-acts { margin: 10px 0 0; display: flex; gap: 8px; align-items: center; }
  .go { font-size: 12.5px; text-decoration: none; color: #fff; background: var(--mdx-accent-primary); padding: 5px 14px; border-radius: var(--mdx-radius-control, 8px); }
  .mark { font-size: 12.5px; background: none; border: 1px solid var(--mdx-border-default); color: var(--mdx-text-secondary); padding: 5px 12px; border-radius: var(--mdx-radius-control, 8px); cursor: pointer; }
  .mark:disabled { opacity: 0.6; cursor: default; }

  .scope { display: flex; flex-direction: column; gap: 12px; }
  .scope-card { padding: 14px 16px; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-lg); background: var(--mdx-surface-raised); }
  .scope-head { margin: 0 0 8px; font-size: 12.5px; font-weight: 650; color: var(--mdx-text-primary); }
  .scope-card ul { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 6px; font-size: 12.5px; color: var(--mdx-text-secondary); }
  .gated li { display: flex; gap: 8px; }
  .lock { color: var(--mdx-accent-success, var(--mdx-success-green)); flex: none; }
  .coming li { color: var(--mdx-text-tertiary); }
  .scope-foot { margin: 8px 0 0; font-size: 11.5px; color: var(--mdx-text-tertiary); }

  .quiet { margin: 18px 0 0; font-size: 11.5px; color: var(--mdx-text-tertiary); }

  @media (max-width: 760px) {
    .wb-grid { grid-template-columns: 1fr; }
  }
</style>
