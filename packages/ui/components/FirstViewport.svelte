<script>
  import StatusPill from "./StatusPill.svelte";

  export let surface = "observatory";
  export let eyebrow = "MDx";
  export let title = "See what matters now.";
  export let summary = "";
  export let status = "ready";
  export let statusLabel = "";
  export let pulseLabel = "Current pulse";
  export let pulseText = "";
  export let judgmentLabel = "Needs my judgment";
  export let judgmentTitle = "";
  export let judgmentBody = "";
  export let judgmentCues = [];
  export let safeNextLabel = "Safe next move";
  export let safeNextTitle = "";
  export let safeNextBody = "";
  export let safeNextCues = [];
  export let boundaryTitle = "Boundary";
  export let boundaryHeading = "No hidden hands.";
  export let boundaryBody = "";
  export let boundaryCues = [];
  export let evidence = [];
  export let actions = [];
  export let signals = [];
  export let decisionTrail = [];

  $: visibleEvidence = evidence.filter(Boolean).slice(0, 4);
  $: visibleActions = actions.slice(0, 4);
  $: visibleSignals = signals.filter(Boolean).slice(0, 3);
  $: visibleDecisionTrail = decisionTrail.filter(Boolean).slice(0, 4);
  $: visibleJudgmentCues = judgmentCues.filter(Boolean).slice(0, 3);
  $: visibleSafeNextCues = safeNextCues.filter(Boolean).slice(0, 3);
  $: visibleBoundaryCues = boundaryCues.filter(Boolean).slice(0, 3);
</script>

<section class="first-viewport" data-ui-first-viewport data-surface={surface} aria-label={`${eyebrow} first viewport`}>
  <div class="hero-panel">
    <p>{eyebrow}</p>
    <h1>{title}</h1>
    <span>{summary}</span>
    <div class="hero-status" data-ui-current-signal>
      <StatusPill status={status} label={statusLabel} />
      <small>{pulseLabel}: {pulseText}</small>
    </div>
    {#if visibleSignals.length > 0}
      <div class="hero-signals" aria-label="Current signal details">
        {#each visibleSignals as signal}
          <span>
            <strong>{signal.label}</strong>
            <small>{signal.value}</small>
          </span>
        {/each}
      </div>
    {/if}
    {#if visibleDecisionTrail.length > 0}
      <ol class="decision-trail" data-ui-decision-trail aria-label="First decision trail">
        {#each visibleDecisionTrail as item}
          <li>
            <strong>{item.label}</strong>
            <small>{item.value}</small>
          </li>
        {/each}
      </ol>
    {/if}
    {#if visibleActions.length > 0}
      <div class="hero-actions" aria-label="Safe read-only actions">
        {#each visibleActions as action}
          <a href={action.href} title={action.intent} aria-label={`${action.label}: ${action.intent}`}>{action.label}</a>
        {/each}
      </div>
    {/if}
  </div>

  <aside class="judgment-panel" data-ui-judgment-affordance>
    <p>{judgmentLabel}</p>
    <h2>{judgmentTitle}</h2>
    <span>{judgmentBody}</span>
    {#if visibleJudgmentCues.length > 0}
      <ul class="guidance-list" data-ui-judgment-cues aria-label={`${judgmentLabel} cues`}>
        {#each visibleJudgmentCues as cue}
          <li>{cue}</li>
        {/each}
      </ul>
    {/if}
  </aside>

  <aside class="safe-panel" data-ui-safe-next>
    <p>{safeNextLabel}</p>
    <h2>{safeNextTitle}</h2>
    <span>{safeNextBody}</span>
    {#if visibleSafeNextCues.length > 0}
      <ul class="guidance-list" data-ui-safe-next-cues aria-label={`${safeNextLabel} cues`}>
        {#each visibleSafeNextCues as cue}
          <li>{cue}</li>
        {/each}
      </ul>
    {/if}
  </aside>

  <aside class="boundary-panel" data-ui-boundary>
    <p>{boundaryTitle}</p>
    <h2>{boundaryHeading}</h2>
    <span>{boundaryBody}</span>
    {#if visibleBoundaryCues.length > 0}
      <ul class="guidance-list" data-ui-boundary-cues aria-label={`${boundaryTitle} cues`}>
        {#each visibleBoundaryCues as cue}
          <li>{cue}</li>
        {/each}
      </ul>
    {/if}
    {#if visibleEvidence.length > 0}
      <details data-ui-evidence-disclosure>
        <summary>Evidence used</summary>
        {#each visibleEvidence as item}
          <small>{item}</small>
        {/each}
      </details>
    {/if}
  </aside>
</section>

<style>
  .first-viewport {
    display: grid;
    grid-template-columns: minmax(430px, 1.45fr) minmax(220px, 0.7fr) minmax(220px, 0.7fr);
    gap: 12px;
    margin-top: 20px;
  }

  .hero-panel,
  .judgment-panel,
  .safe-panel,
  .boundary-panel {
    border: 1px solid var(--mdx-border-soft);
    border-radius: var(--mdx-radius-panel);
    background: rgba(var(--mdx-surface-rgb), 0.84);
    box-shadow: var(--mdx-shadow-panel);
  }

  .hero-panel {
    grid-row: span 2;
    min-height: 430px;
    padding: clamp(22px, 4vw, 42px);
    background:
      radial-gradient(circle at 16% 8%, rgba(var(--mdx-evidence-cyan-rgb), 0.22), transparent 34%),
      linear-gradient(145deg, rgba(var(--mdx-focus-blue-rgb), 0.16), rgba(var(--mdx-success-green-rgb), 0.08) 54%, rgba(var(--mdx-surface-rgb), 0.82)),
      rgba(var(--mdx-surface-raised-rgb), 0.86);
  }

  .judgment-panel,
  .safe-panel,
  .boundary-panel {
    display: grid;
    align-content: start;
    gap: 10px;
    min-height: 204px;
    padding: 18px;
  }

  .judgment-panel {
    border-color: color-mix(in srgb, var(--mdx-warning-amber) 34%, var(--mdx-border-soft));
    --cue-color: var(--mdx-warning-amber);
  }

  .safe-panel {
    border-color: color-mix(in srgb, var(--mdx-success-green) 30%, var(--mdx-border-soft));
    --cue-color: var(--mdx-success-green);
  }

  .boundary-panel {
    grid-column: 2 / 4;
    min-height: 214px;
    border-color: color-mix(in srgb, var(--mdx-evidence-cyan) 30%, var(--mdx-border-soft));
    --cue-color: var(--mdx-evidence-cyan);
  }

  p,
  h1,
  h2,
  span,
  small {
    margin: 0;
  }

  p {
    color: var(--mdx-text-caption);
    font-size: 12px;
    font-weight: 800;
    letter-spacing: 0;
    text-transform: uppercase;
  }

  h1 {
    max-width: 820px;
    margin-top: 10px;
    color: var(--mdx-text-primary);
    font-family: var(--mdx-font-display);
    font-size: 58px;
    line-height: 1;
  }

  h2 {
    color: var(--mdx-text-primary);
    font-family: var(--mdx-font-display);
    font-size: 24px;
    line-height: 1.08;
  }

  .hero-panel > span {
    display: block;
    max-width: 720px;
    margin-top: 18px;
    color: var(--mdx-text-secondary);
    font-size: 16px;
    line-height: 1.58;
  }

  .judgment-panel span,
  .safe-panel span,
  .boundary-panel span,
  details small {
    color: var(--mdx-text-secondary);
    font-size: 13px;
    line-height: 1.48;
  }

  .hero-status {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin-top: 28px;
  }

  .hero-status small {
    color: var(--mdx-text-secondary);
    font-size: 13px;
  }

  .hero-signals {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
    margin-top: 24px;
  }

  .hero-signals span {
    display: grid;
    gap: 6px;
    min-height: 86px;
    border: 1px solid var(--mdx-border-chip);
    border-radius: var(--mdx-radius-panel);
    padding: 12px;
    background: rgba(var(--mdx-bg-rgb), 0.32);
  }

  .hero-signals strong {
    color: var(--mdx-text-primary);
    font-size: 13px;
    line-height: 1.22;
  }

  .hero-signals small {
    color: var(--mdx-text-secondary);
    font-size: 12px;
    line-height: 1.35;
  }

  .decision-trail {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
    margin: 24px 0 0;
    padding: 0;
    list-style: none;
  }

  .decision-trail li {
    display: grid;
    align-content: start;
    gap: 6px;
    min-height: 74px;
    border: 1px solid var(--mdx-border-chip);
    border-radius: var(--mdx-radius-panel);
    padding: 10px;
    background: rgba(var(--mdx-surface-rgb), 0.5);
  }

  .decision-trail strong {
    color: var(--mdx-evidence-cyan);
    font-size: 12px;
    line-height: 1.18;
  }

  .decision-trail small {
    color: var(--mdx-text-secondary);
    font-size: 12px;
    line-height: 1.32;
    overflow-wrap: anywhere;
  }

  .hero-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    margin-top: 28px;
  }

  .hero-actions a {
    display: inline-flex;
    align-items: center;
    min-height: 42px;
    border: 1px solid var(--mdx-border-chip);
    border-radius: var(--mdx-radius-control);
    padding: 0 14px;
    color: var(--mdx-text-primary);
    background: rgba(var(--mdx-bg-rgb), 0.42);
    font-size: 13px;
    font-weight: 800;
    text-decoration: none;
    transition:
      border-color var(--mdx-motion-base) var(--mdx-ease),
      background-color var(--mdx-motion-base) var(--mdx-ease),
      transform var(--mdx-motion-fast) var(--mdx-ease);
  }

  .hero-actions a:hover {
    border-color: rgba(var(--mdx-evidence-cyan-rgb), 0.52);
    background: rgba(var(--mdx-evidence-cyan-rgb), 0.12);
    transform: translateY(-1px);
  }

  details {
    display: grid;
    gap: 8px;
    margin-top: 2px;
  }

  .guidance-list {
    display: grid;
    gap: 8px;
    margin: 2px 0 0;
    padding: 0;
    list-style: none;
  }

  .guidance-list li {
    position: relative;
    padding-left: 16px;
    color: var(--mdx-text-secondary);
    font-size: 12px;
    line-height: 1.4;
  }

  .guidance-list li::before {
    position: absolute;
    top: 0.55em;
    left: 0;
    width: 6px;
    height: 6px;
    border-radius: var(--mdx-radius-pill);
    background: var(--cue-color, var(--mdx-evidence-cyan));
    content: "";
  }

  summary {
    cursor: pointer;
    color: var(--mdx-evidence-cyan);
    font-size: 13px;
    font-weight: 800;
  }

  details small {
    display: block;
    margin-top: 7px;
  }

  @media (max-width: 940px) {
    .first-viewport {
      grid-template-columns: 1fr;
    }

    .hero-panel,
    .boundary-panel {
      grid-column: auto;
      grid-row: auto;
    }

    .hero-panel {
      min-height: 0;
    }

    h1 {
      font-size: 48px;
    }
  }

  @media (max-width: 560px) {
    .first-viewport {
      margin-top: 14px;
    }

    .hero-panel,
    .judgment-panel,
    .safe-panel,
    .boundary-panel {
      padding: 16px;
    }

    h1 {
      font-size: 34px;
    }

    .hero-signals,
    .decision-trail {
      grid-template-columns: 1fr;
    }
  }
</style>
