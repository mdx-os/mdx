<script>
  import { page } from "$app/state";
  import PublicNav from "../../lib/marketing/PublicNav.svelte";
  import PublicFooter from "../../lib/marketing/PublicFooter.svelte";
  import { SOURCE_REPOSITORY_URL, SOURCE_REPOSITORY_LABEL } from "../../lib/marketing/publicSite.js";
</script>

<svelte:head>
  <title>MDx - Build with agents. Keep the final say.</title>
  <meta
    name="description"
    content="MDx is one private workspace to plan work, run agents, and review what they did. The source is open and the hosted beta is invite-only."
  />
  <meta property="og:title" content="MDx - Build with agents. Keep the final say." />
  <meta property="og:description" content="Plan work, run agents, and review what they did in one private workspace." />
  <meta property="og:type" content="website" />
  <meta property="og:image" content={`${page.url.origin}/og.png`} />
  <meta name="twitter:card" content="summary_large_image" />
  <meta name="twitter:image" content={`${page.url.origin}/og.png`} />
</svelte:head>

<div class="page">
  <PublicNav active="product" />

  <main>
    <section class="hero">
      <p class="eyebrow">Private beta</p>
      <h1>Build with agents.<br /><em>Keep the final say.</em></h1>
      <p class="lede">
        MDx is one private workspace to plan work, run agents, and review what
        they did. You decide what moves forward.
      </p>
      <div class="actions">
        <a class="button primary" href="/waitlist">Request an invite</a>
        <a class="button secondary" href={SOURCE_REPOSITORY_URL}>View the source</a>
      </div>
    </section>

    <section class="product-stage" aria-label="MDx product preview">
      <div class="product-window">
        <div class="window-bar">
          <div class="traffic" aria-hidden="true"><i></i><i></i><i></i></div>
          <span>MDx</span>
          <span class="workspace"><i></i> local workspace</span>
        </div>
        <div class="window-body">
          <aside>
            <strong>MD<span>x</span></strong>
            <a class="selected" href="#product">Today</a>
            <a href="#product">Twin</a>
            <a href="#product">Forge <small>1</small></a>
            <a href="#product">Message</a>
            <a href="#product">Pages</a>
          </aside>
          <div class="workbench">
            <header>
              <div><span>Today</span><h2>Your work, with agents.</h2></div>
              <button type="button">Start something</button>
            </header>
            <article class="decision-card">
              <div class="decision-mark" aria-hidden="true">&rarr;</div>
              <div>
                <span>Ready for your review</span>
                <h3>Repair session refresh after sleep</h3>
                <p>The change is bounded, checks are green, and nothing has shipped.</p>
              </div>
              <button type="button">Review change</button>
            </article>
            <div class="work-grid">
              <article class="trail-card">
                <p class="card-label">Work trail</p>
                <ol>
                  <li data-state="done"><i></i><div><strong>Plan agreed</strong><span>Scope and checks recorded</span></div></li>
                  <li data-state="done"><i></i><div><strong>Agent work complete</strong><span>One branch, bounded files</span></div></li>
                  <li data-state="done"><i></i><div><strong>Checks passed</strong><span>Evidence attached</span></div></li>
                  <li data-state="active"><i></i><div><strong>Your call</strong><span>Ship, revise, or set aside</span></div></li>
                </ol>
              </article>
              <article class="context-card">
                <p class="card-label">Twin</p>
                <h3>The context stays with the work.</h3>
                <p>Decisions, source material, and agent output stay together instead of disappearing into separate chats.</p>
                <span class="source-chip">3 sources on the record</span>
              </article>
            </div>
          </div>
        </div>
      </div>
    </section>

    <section id="product" class="jobs section-wrap">
      <div class="section-heading">
        <p class="eyebrow">One workspace</p>
        <h2>Less agent theater.<br />More work you can trust.</h2>
      </div>
      <div class="job-grid">
        <article>
          <span>01</span>
          <h3>Plan together</h3>
          <p>Start with intent, context, and a bounded next move before an agent touches the work.</p>
        </article>
        <article>
          <span>02</span>
          <h3>Run bounded work</h3>
          <p>Give agents a clear scope and the checks that have to pass. Keep the work isolated and reviewable.</p>
        </article>
        <article>
          <span>03</span>
          <h3>Make the call</h3>
          <p>See what changed and why. Shipping, revising, or setting work aside stays a human decision.</p>
        </article>
      </div>
    </section>

    <section class="source section-wrap">
      <div class="source-copy">
        <p class="eyebrow">Apache 2.0</p>
        <h2>Take the whole thing.</h2>
        <p>
          Read it, run it, fork it, and make it yours. The source is public.
          The hosted MDx beta remains invite-only.
        </p>
        <a class="button secondary" href={SOURCE_REPOSITORY_URL}>Browse the source</a>
      </div>
      <div class="source-card" aria-label="MDx source repository">
        <span>Open source</span>
        <strong>{SOURCE_REPOSITORY_LABEL}</strong>
        <div>
          <code>git clone {SOURCE_REPOSITORY_URL}</code>
          <code>cd mdx</code>
          <code>make local-smoke</code>
        </div>
        <small>Public setup must pass from a clean machine before launch.</small>
      </div>
    </section>

    <section class="beta section-wrap">
      <p class="eyebrow">Hosted beta</p>
      <h2>Small by design.</h2>
      <p>
        MDx is early. Expect rough edges and direct conversations with the people
        building it. We are inviting a small group before making bigger claims.
      </p>
      <div class="actions">
        <a class="button primary" href="/waitlist">Request an invite</a>
        <a class="text-link" href="/redeem">I already have one</a>
      </div>
    </section>
  </main>

  <PublicFooter />
</div>

<style>
  :global(html) { scroll-behavior: smooth; }
  :global(body) { margin: 0; background: #08080a; }
  .page { min-height: 100dvh; overflow: hidden; background: #08080a; color: #f4f4f6; font-family: var(--mdx-font-body); }
  .hero { max-width: 1040px; margin: 0 auto; padding: clamp(7rem, 13vw, 11rem) 1.5rem clamp(4rem, 8vw, 7rem); text-align: center; }
  .eyebrow { margin: 0 0 1.25rem; color: #78a1ff; font-family: var(--mdx-font-mono); font-size: .72rem; font-weight: 800; letter-spacing: .18em; text-transform: uppercase; }
  h1, h2, h3, p { margin-top: 0; }
  h1, h2, h3 { font-family: var(--mdx-font-display); }
  h1 { margin-bottom: 1.8rem; font-size: clamp(3.6rem, 8.8vw, 8.1rem); line-height: .94; letter-spacing: -.065em; }
  h1 em { color: #78a1ff; font-style: normal; }
  .lede { max-width: 690px; margin: 0 auto; color: #a0a0aa; font-size: clamp(1.05rem, 2vw, 1.3rem); line-height: 1.65; }
  .actions { display: flex; align-items: center; justify-content: center; gap: .8rem; flex-wrap: wrap; margin-top: 2.2rem; }
  .button { display: inline-flex; min-height: 3.2rem; align-items: center; justify-content: center; padding: 0 1.5rem; border-radius: 999px; font-size: .92rem; font-weight: 800; text-decoration: none; }
  .button.primary { background: #3f78f6; color: white; box-shadow: 0 14px 38px rgba(63,120,246,.2); }
  .button.secondary { border: 1px solid #303036; background: #101012; color: #e7e7ea; }
  .button:hover { transform: translateY(-1px); }
  .product-stage { max-width: 1180px; margin: 0 auto; padding: 0 1.25rem clamp(7rem, 12vw, 11rem); }
  .product-window { overflow: hidden; border: 1px solid #303037; border-radius: 16px; background: #0d0d10; box-shadow: 0 50px 120px rgba(0,0,0,.55), 0 0 80px rgba(63,120,246,.08); }
  .window-bar { height: 46px; display: grid; grid-template-columns: 1fr auto 1fr; align-items: center; padding: 0 16px; border-bottom: 1px solid #24242a; color: #74747d; font-size: .72rem; }
  .traffic { display: flex; gap: 6px; }.traffic i { width: 8px; height: 8px; border-radius: 50%; background: #35353b; }
  .workspace { justify-self: end; display: flex; align-items: center; gap: 6px; }.workspace i { width: 6px; height: 6px; border-radius: 50%; background: #66d59a; }
  .window-body { min-height: 570px; display: grid; grid-template-columns: 150px 1fr; }
  aside { display: flex; flex-direction: column; gap: 4px; padding: 22px 12px; border-right: 1px solid #24242a; }
  aside strong { margin: 0 10px 22px; font-size: 1rem; } aside strong span { color: #78a1ff; }
  aside a { display: flex; justify-content: space-between; padding: 9px 10px; border-radius: 7px; color: #767680; font-size: .78rem; text-decoration: none; }
  aside a.selected { background: rgba(63,120,246,.12); color: #f3f3f5; } aside small { color: #78a1ff; }
  .workbench { padding: clamp(1.3rem, 4vw, 3rem); background: radial-gradient(44rem 30rem at 70% 0%, rgba(63,120,246,.08), transparent 70%); }
  .workbench > header { display: flex; justify-content: space-between; gap: 2rem; align-items: end; margin-bottom: 2rem; }
  .workbench header span, .card-label, .decision-card span { color: #78a1ff; font-family: var(--mdx-font-mono); font-size: .65rem; font-weight: 700; letter-spacing: .08em; text-transform: uppercase; }
  .workbench header h2 { margin: .35rem 0 0; font-size: clamp(1.8rem, 4vw, 3.1rem); letter-spacing: -.04em; }
  button { border: 0; border-radius: 8px; padding: .75rem 1rem; background: #f1f1f3; color: #111114; font: inherit; font-size: .75rem; font-weight: 800; }
  .decision-card, .trail-card, .context-card { border: 1px solid #29292f; border-radius: 10px; background: rgba(20,20,23,.88); }
  .decision-card { display: grid; grid-template-columns: auto 1fr auto; align-items: center; gap: 1rem; padding: 1.2rem; }
  .decision-mark { width: 34px; height: 34px; display: grid; place-items: center; border-radius: 50%; background: #78a1ff; color: #0b0b0d; font-weight: 900; }
  .decision-card h3 { margin: .3rem 0; font-size: 1.05rem; }.decision-card p { margin: 0; color: #7d7d86; font-size: .78rem; }
  .work-grid { display: grid; grid-template-columns: 1.1fr .9fr; gap: 1rem; margin-top: 1rem; }
  .trail-card, .context-card { min-height: 270px; padding: 1.3rem; }
  .trail-card ol { display: grid; gap: 0; margin: 1.2rem 0 0; padding: 0; list-style: none; }
  .trail-card li { position: relative; min-height: 48px; display: grid; grid-template-columns: auto 1fr; gap: .8rem; }
  .trail-card li:not(:last-child)::after { content: ""; position: absolute; top: 12px; bottom: 0; left: 4px; width: 1px; background: #34343a; }
  .trail-card li > i { z-index: 1; width: 8px; height: 8px; margin-top: 2px; border: 2px solid #55555f; border-radius: 50%; background: #141417; }
  .trail-card li[data-state="done"] > i { border-color: #66d59a; background: #66d59a; }.trail-card li[data-state="active"] > i { border-color: #78a1ff; box-shadow: 0 0 0 4px rgba(120,161,255,.12); }
  .trail-card li div { display: grid; align-content: start; gap: 2px; }.trail-card strong { font-size: .78rem; }.trail-card span { color: #72727b; font-size: .69rem; }
  .context-card { display: flex; flex-direction: column; }.context-card h3 { margin: 1.2rem 0 .8rem; font-size: 1.55rem; line-height: 1.05; }.context-card > p:not(.card-label) { color: #888891; font-size: .82rem; line-height: 1.6; }
  .source-chip { align-self: flex-start; margin-top: auto; padding: .5rem .65rem; border: 1px solid #303037; border-radius: 999px; color: #83838c; font-size: .68rem; }
  .section-wrap { max-width: 1120px; margin: 0 auto; padding: clamp(6rem, 11vw, 10rem) 1.5rem; }
  .section-heading { max-width: 790px; }.section-heading h2, .source h2, .beta h2 { margin-bottom: 0; font-size: clamp(2.8rem, 6.3vw, 5.7rem); line-height: .98; letter-spacing: -.055em; }
  .jobs { border-top: 1px solid #222227; }
  .job-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 1px; margin-top: clamp(3rem, 8vw, 6rem); background: #25252a; border: 1px solid #25252a; }
  .job-grid article { min-height: 280px; padding: clamp(1.5rem, 3vw, 2.3rem); background: #0c0c0e; }
  .job-grid article > span { color: #5f5f68; font-family: var(--mdx-font-mono); font-size: .7rem; }.job-grid h3 { margin: 5rem 0 .8rem; font-size: 1.5rem; }.job-grid p { margin: 0; color: #85858e; line-height: 1.65; }
  .source { max-width: none; display: grid; grid-template-columns: 1fr 1fr; gap: clamp(3rem, 9vw, 9rem); align-items: center; padding-inline: max(1.5rem, calc((100vw - 1120px) / 2)); border-top: 1px solid #222227; background: #0c0c0e; }
  .source-copy > p:not(.eyebrow) { max-width: 540px; margin: 1.5rem 0 2rem; color: #91919a; font-size: 1.05rem; line-height: 1.7; }
  .source-card { min-width: 0; overflow: hidden; border: 1px solid #2c2c32; border-radius: 12px; background: #111114; }
  .source-card > span, .source-card > strong, .source-card > small { display: block; padding-inline: 1.4rem; }.source-card > span { padding-top: 1.4rem; color: #78a1ff; font-family: var(--mdx-font-mono); font-size: .68rem; text-transform: uppercase; }.source-card > strong { padding-top: .45rem; padding-bottom: 1.4rem; font-size: 1rem; }
  .source-card div { display: grid; gap: .7rem; padding: 1.4rem; border-block: 1px solid #2c2c32; background: #0a0a0c; }.source-card code { overflow: hidden; color: #c8c8ce; font-size: .76rem; text-overflow: ellipsis; white-space: nowrap; }.source-card code::before { content: "$ "; color: #66d59a; }
  .source-card small { padding-top: 1rem; padding-bottom: 1rem; color: #66666f; font-size: .7rem; }
  .beta { text-align: center; }.beta > p:not(.eyebrow) { max-width: 650px; margin: 1.5rem auto 0; color: #91919a; line-height: 1.7; }.beta .text-link { color: #aaaab2; font-size: .88rem; font-weight: 700; text-decoration: none; }
  @media (max-width: 800px) {
    .window-body { grid-template-columns: 82px 1fr; } aside { padding-inline: 7px; } aside strong { margin-inline: 6px; } aside a { padding-inline: 6px; font-size: .7rem; }
    .work-grid { grid-template-columns: 1fr; }.context-card { min-height: 220px; }
    .job-grid, .source { grid-template-columns: 1fr; }.source { gap: 3rem; }.job-grid h3 { margin-top: 3rem; }
  }
  @media (max-width: 560px) {
    .hero { padding-top: 6rem; }.product-stage { padding-inline: .75rem; }.product-window { margin-right: -18rem; }.window-body { min-height: 520px; grid-template-columns: 74px 1fr; }.workbench { width: 520px; padding: 1.1rem; }.workbench > header button { display: none; }
    .section-wrap { padding-inline: 1.1rem; }.decision-card { grid-template-columns: auto 1fr; }.decision-card > button { grid-column: 2; justify-self: start; }.job-grid article { min-height: 220px; }
  }
  @media (prefers-reduced-motion: reduce) { :global(*) { scroll-behavior: auto !important; transition: none !important; } }
</style>
