<script>
  // The operator's room, as a guided journey. One clear "you are here"
  // step, computed from the kernel's own packets - like setting up a new
  // machine, where each step proves itself before the next opens. Done
  // steps carry a quiet check. The step you're on says what you GET and
  // the single command or link that moves it. Steps after it dim to one
  // line of what's coming. Kernel down: step one becomes an invitation
  // and nothing claims to be ready.
  //
  // The rooms grid and the coming slots stay below the journey - the
  // journey is the spine, the rooms are where the work lives once you
  // know where you are.
  import Provenance from "@mdx/ui/components/Provenance.svelte";

  let { data } = $props();

  const journey = $derived(data.journey);
  const steps = $derived(journey?.steps ?? []);

  const sourceItems = $derived(
    (data.sources ?? []).map((source) => ({ label: source.label, value: source.route }))
  );

  // The rooms where the day-to-day work lives, below the spine.
  const areas = [
    {
      href: "/admin/watchtower",
      name: "Watchtower",
      line: "One pulse: is beta healthy, what's blocked, what changed, what to look at next."
    },
    {
      href: "/admin/setup",
      name: "Setup",
      line: "From this machine to the cloud — six tracks, each proven before it moves."
    },
    {
      href: "/admin/platform",
      name: "Platform",
      line: "What this install runs on, what's live, and what turns on next."
    },
    {
      href: "/admin/developer",
      name: "Developer",
      line: "How MDx is built and how to build on it — the repo describing itself."
    },
    {
      href: "/admin/registry",
      name: "Registry",
      line: "Components, routes, flows, sources, and capabilities in one operator map."
    },
    {
      href: "/admin/access",
      name: "Access",
      line: "Roles, invites, sessions, policy, and blocked authority from the auth contracts."
    },
    {
      href: "/admin/guardrails",
      name: "Guardrails",
      line: "Input, output, privacy, and refusal rails that stay closed until authority exists."
    },
    {
      href: "/admin/gate",
      name: "Gate",
      line: "Write authority, receipt coverage, change gates, and blocked paths in one map."
    },
    {
      href: "/admin/models",
      name: "Models",
      line: "Deterministic routing, provider slots, and live turn-on evidence without live-call authority."
    },
    {
      href: "/admin/operations",
      name: "Operations",
      line: "Runtime pulse, evidence state, live-substrate blockers, and proof routes."
    },
    {
      href: "/admin/evidence",
      name: "Evidence",
      line: "Receipts, checkpoints, evidence chains, and receipt-backed stores."
    },
    {
      href: "/admin/pages",
      name: "Knowledge",
      line: "Pages, citations, gaps, and learning hooks for the company memory layer."
    },
    {
      href: "/admin/aegis",
      name: "Security",
      line: "What's watching, what it found, and what's blocked by design."
    },
    {
      href: "/admin/charter",
      name: "Charter",
      line: "The rules this company runs by, attested on the record."
    }
  ];

  const coming = [
    {
      name: "Connectors",
      line: "Slack, Teams, GitHub, and sign-on land here — each with its own recorded turn-on."
    }
  ];
</script>

<section class="admin-home" data-route-state="ready">
  <header class="mdx-page-head">
    <h1>Getting started</h1>
    {#if data.pulse}
      <p class="mdx-page-sub">{data.pulse}</p>
    {:else}
      <p class="mdx-page-sub">
        Start the local server and this fills in from your install's own evidence — one clear step
        at a time.
      </p>
    {/if}
  </header>

  <ol class="journey">
    {#each steps as step (step.id)}
      <li class="step {step.state}">
        <div class="marker" aria-hidden="true">
          {#if step.state === "done"}
            <span class="check">✓</span>
          {:else}
            <span class="num">{step.number}</span>
          {/if}
        </div>

        <div class="step-body">
          <div class="step-title">
            <h2>{step.title}</h2>
            {#if step.state === "current"}<span class="here">you are here</span>{/if}
          </div>

          {#if step.state === "done"}
            <p class="step-line">{step.doneLine}</p>
          {:else if step.state === "current"}
            <p class="step-line">{step.get}</p>
            <div class="step-do">
              {#if step.command}
                <span class="do-label">Run this</span>
                <code class="do-command">{step.command}</code>
              {:else if step.link}
                <a class="do-link" href={step.link}>{step.linkLabel} →</a>
              {/if}
            </div>
            <p class="step-proves">{step.proves}</p>
          {:else}
            <p class="step-line muted">{step.coming}</p>
          {/if}

          {#if step.state !== "later" && step.rawState}
            <details class="quiet-more">
              <summary>view the state</summary>
              <span><code>{step.rawState}</code></span>
            </details>
          {/if}
        </div>
      </li>
    {/each}
  </ol>

  {#if sourceItems.length > 0}
    <p class="journey-source">
      This journey is read from your install's own packets.
      <Provenance items={sourceItems} label="Where each step's state comes from" />
    </p>
  {/if}

  <div class="rooms">
    <h3 class="rooms-head">Inside Admin</h3>
    <div class="area-grid">
      {#each areas as area}
        <a class="area" href={area.href}>
          <strong>{area.name}</strong>
          <span>{area.line}</span>
        </a>
      {/each}
      {#each coming as slot}
        <div class="area coming">
          <strong>{slot.name}</strong>
          <span>{slot.line}</span>
        </div>
      {/each}
    </div>
  </div>

  <footer class="quiet-line">
    This area gates itself the day sign-in lands; today it's yours.
    <details class="quiet-more">
      <summary>more</summary>
      <span>
        Boundary: {data.boundary}. Safe next move: {data.safeNext}.
      </span>
    </details>
  </footer>
</section>

<style>
  .admin-home {
    display: grid;
    gap: 26px;
    align-content: start;
    max-width: 760px;
    margin: 0 auto;
    padding: 5vh 0 40px;
  }

  /* The journey spine */
  .journey {
    display: grid;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .step {
    display: grid;
    grid-template-columns: 34px minmax(0, 1fr);
    gap: 14px;
    padding: 16px 16px 16px 8px;
    border-radius: var(--mdx-radius-lg);
    transition: background 180ms ease-out;
  }

  .step.current {
    background: var(--mdx-surface-raised);
    border: 1px solid var(--mdx-accent-primary);
    padding-left: 12px;
  }

  .step.later {
    opacity: 0.5;
  }

  .marker {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: 50%;
    border: 1px solid var(--mdx-border-strong);
    font-family: var(--mdx-font-display);
    font-size: 13px;
    font-weight: 650;
    color: var(--mdx-text-muted);
  }

  .step.done .marker {
    border-color: var(--mdx-tone-ok-text);
    color: var(--mdx-tone-ok-text);
  }

  .step.current .marker {
    border-color: var(--mdx-accent-primary);
    color: var(--mdx-accent-primary);
  }

  .check {
    font-size: 15px;
    line-height: 1;
  }

  .step-body {
    display: grid;
    gap: 7px;
    min-width: 0;
  }

  .step-title {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 10px;
  }

  .step-title h2 {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: 16px;
    font-weight: 650;
  }

  .here {
    font-size: var(--mdx-text-xs);
    font-weight: 600;
    color: var(--mdx-accent-primary);
    text-transform: lowercase;
    letter-spacing: 0.02em;
  }

  .step-line {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 14px;
    line-height: 1.55;
  }

  .step-line.muted {
    color: var(--mdx-text-muted);
    font-size: 13px;
  }

  .step.done .step-line {
    color: var(--mdx-text-muted);
    font-size: 13px;
  }

  .step-do {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 10px;
    margin: 4px 0 2px;
  }

  .do-label {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
  }

  .do-command {
    padding: 6px 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md, 8px);
    background: var(--mdx-surface-sunken, var(--mdx-surface-raised));
    font-family: var(--mdx-font-mono);
    font-size: 12.5px;
    color: var(--mdx-text-primary);
    overflow-wrap: anywhere;
  }

  .do-link {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--mdx-accent-primary);
    text-decoration: none;
  }

  .do-link:hover,
  .do-link:focus-visible {
    text-decoration: underline;
  }

  .step-proves {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: 12.5px;
    line-height: 1.5;
  }

  .journey-source {
    display: inline;
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .journey-source :global(.provenance) {
    margin-left: 6px;
    vertical-align: baseline;
  }

  /* The rooms grid below the spine */
  .rooms {
    display: grid;
    gap: 12px;
  }

  .rooms-head {
    margin: 0;
    color: var(--mdx-text-tertiary);
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .area-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
  }

  .area {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 18px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    text-decoration: none;
    transition: border-color 180ms ease-out, transform 180ms ease-out;
  }

  a.area:hover,
  a.area:focus-visible {
    border-color: var(--mdx-accent-primary);
    transform: translateY(-2px);
  }

  .area strong {
    font-family: var(--mdx-font-display);
    font-size: 15px;
    font-weight: 650;
  }

  .area span {
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    line-height: 1.5;
  }

  .area.coming {
    opacity: 0.75;
    border-style: dashed;
  }

  .area.coming span {
    color: var(--mdx-text-muted);
  }

  .quiet-line {
    display: flex;
    align-items: baseline;
    gap: 8px;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .quiet-more {
    display: inline;
  }

  .quiet-more summary {
    display: inline;
    color: var(--mdx-text-muted);
    cursor: pointer;
    list-style: none;
    font-size: var(--mdx-text-xs);
  }

  .quiet-more summary::-webkit-details-marker {
    display: none;
  }

  .quiet-line .quiet-more[open] summary {
    display: none;
  }

  .step .quiet-more summary {
    color: var(--mdx-accent-primary);
    opacity: 0.8;
  }

  @media (prefers-reduced-motion: reduce) {
    .area,
    .step {
      transition: none;
    }

    a.area:hover,
    a.area:focus-visible {
      transform: none;
    }
  }

  @media (max-width: 640px) {
    .area-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
