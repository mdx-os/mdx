<script>
  import PublicNav from "../../lib/marketing/PublicNav.svelte";
  import PublicFooter from "../../lib/marketing/PublicFooter.svelte";

  let { data } = $props();

  const readable = (value) => String(value ?? "")
    .replace(/[_-]/g, " ")
    .replace(/\b\w/g, (letter) => letter.toUpperCase());
</script>

<svelte:head>
  <title>Security - MDx</title>
  <meta name="description" content="The practical boundaries behind agent work in MDx." />
</svelte:head>

<div class="page">
  <PublicNav />

  <main>
    <header>
      <p class="eyebrow">Security</p>
      <h1>Keep the agent useful.<br />Keep the boundary clear.</h1>
      <p>
        MDx records consequential work, requires human judgment at the important
        edges, and refuses to turn an agent answer into silent authority.
      </p>
    </header>

    <section class="principles" aria-label="MDx security boundaries">
      <article>
        <span>01</span>
        <h2>Scope before action</h2>
        <p>Work starts with an explicit boundary, not open-ended permission.</p>
      </article>
      <article>
        <span>02</span>
        <h2>People make the call</h2>
        <p>Consequential steps stop for review instead of quietly crossing the line.</p>
      </article>
      <article>
        <span>03</span>
        <h2>Work leaves a trail</h2>
        <p>Plans, checks, decisions, and results stay connected to the work they shaped.</p>
      </article>
    </section>

    <section class="evidence">
      <p class="eyebrow">Inspect the details</p>
      <h2>The claims have checks behind them.</h2>

      <details>
        <summary><span>Required build and dependency checks</span><small>{data.scanners.length} repository checks</small></summary>
        <ul>
          {#each data.scanners as scanner}
            <li><strong>{readable(scanner.id)}</strong><span>{readable(scanner.category)}</span></li>
          {/each}
        </ul>
      </details>

      <details>
        <summary><span>Enforced product boundaries</span><small>{data.controls.length} controls</small></summary>
        <ul>
          {#each data.controls as control}
            <li><strong>{readable(control.id)}</strong><span>{control.control}</span></li>
          {/each}
        </ul>
      </details>

      {#if data.hardStops.length}
        <details>
          <summary><span>Hard stops</span><small>{data.hardStops.length} recorded limits</small></summary>
          <ul>
            {#each data.hardStops as stop}
              <li><strong>Refused</strong><span>{stop}</span></li>
            {/each}
          </ul>
        </details>
      {/if}

      <p class="note">The public repository contains the policies, checks, and generated evidence behind this page.</p>
    </section>
  </main>

  <PublicFooter />
</div>

<style>
  :global(body) { margin: 0; background: #08080a; }
  .page { min-height: 100dvh; background: #08080a; color: #f4f4f6; font-family: var(--mdx-font-body); }
  main { max-width: 1080px; margin: 0 auto; padding: clamp(6rem, 12vw, 10rem) 1.5rem clamp(7rem, 12vw, 11rem); }
  header { max-width: 900px; }
  .eyebrow { margin: 0 0 1.2rem; color: #78a1ff; font-family: var(--mdx-font-mono); font-size: .72rem; font-weight: 800; letter-spacing: .18em; text-transform: uppercase; }
  h1, h2 { font-family: var(--mdx-font-display); }
  h1 { margin: 0; font-size: clamp(3.2rem, 7vw, 6.4rem); line-height: .97; letter-spacing: -.06em; }
  header > p:last-child { max-width: 670px; margin: 1.6rem 0 0; color: #96969f; font-size: 1.08rem; line-height: 1.7; }
  .principles { display: grid; grid-template-columns: repeat(3, 1fr); gap: 1px; margin-top: clamp(4rem, 9vw, 7rem); border: 1px solid #28282e; background: #28282e; }
  .principles article { min-height: 260px; padding: 2rem; background: #0d0d10; }.principles span { color: #5e5e67; font-family: var(--mdx-font-mono); font-size: .7rem; }.principles h2 { margin: 5rem 0 .8rem; font-size: 1.45rem; }.principles p { margin: 0; color: #898992; line-height: 1.65; }
  .evidence { max-width: 820px; margin-top: clamp(7rem, 14vw, 12rem); }.evidence > h2 { margin: 0 0 3rem; font-size: clamp(2.5rem, 5vw, 4.5rem); line-height: 1; letter-spacing: -.05em; }
  details { border-top: 1px solid #2b2b31; } details:last-of-type { border-bottom: 1px solid #2b2b31; }
  summary { display: flex; justify-content: space-between; gap: 2rem; padding: 1.5rem 0; cursor: pointer; list-style: none; } summary::-webkit-details-marker { display: none; } summary span { font-weight: 750; } summary small { color: #6f6f78; }
  details[open] summary { color: #78a1ff; } ul { display: grid; gap: 0; margin: 0 0 1.5rem; padding: 0; list-style: none; border: 1px solid #25252b; }
  li { display: grid; grid-template-columns: minmax(170px, .7fr) 1.3fr; gap: 1.5rem; padding: 1rem; border-bottom: 1px solid #25252b; } li:last-child { border-bottom: 0; } li strong { font-size: .78rem; } li span { color: #85858e; font-size: .78rem; line-height: 1.5; }
  .note { margin: 1.5rem 0 0; color: #606069; font-size: .76rem; line-height: 1.6; }
  @media (max-width: 720px) { .principles { grid-template-columns: 1fr; }.principles article { min-height: 200px; }.principles h2 { margin-top: 3rem; } }
  @media (max-width: 520px) { main { padding-top: 5.5rem; } summary { gap: 1rem; } summary small { text-align: right; } li { grid-template-columns: 1fr; gap: .45rem; } }
</style>
