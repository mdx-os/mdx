<script>
  import PublicNav from "../../lib/marketing/PublicNav.svelte";
  import PublicFooter from "../../lib/marketing/PublicFooter.svelte";

  const platforms = [
    {
      name: "Web",
      state: "Private beta",
      description: "Open MDx in a modern browser. Hosted updates arrive automatically.",
      update: "Updates automatically",
      action: "Open MDx",
      href: "/auth/sign-in?next=%2Fwelcome%2Fbeta"
    },
    {
      name: "macOS",
      state: "Private beta",
      description: "Install the signed, notarized Mac app and use the same MDx account.",
      update: "Prompts when a new build is ready",
      action: "Download for Mac",
      href: "/auth/sign-in?next=%2Fdownload%2Fmacos"
    },
    {
      name: "iPhone",
      state: "TestFlight",
      description: "Approved beta members receive the iPhone app through TestFlight.",
      update: "Updates through TestFlight",
      action: "Request access",
      href: "/waitlist"
    }
  ];
</script>

<svelte:head>
  <title>Download MDx</title>
  <meta name="description" content="Use the MDx private beta on the web, Mac, and iPhone." />
</svelte:head>

<div class="page">
  <PublicNav active="downloads" />

  <main>
    <header>
      <p class="eyebrow">Download</p>
      <h1>Use MDx where<br />the work happens.</h1>
      <p>One account across web, Mac, and iPhone. Native access remains invite-only while the beta is small.</p>
    </header>

    <section class="platforms" aria-label="MDx platforms">
      {#each platforms as platform}
        <article>
          <div class="platform-name">
            <span>{platform.name}</span>
            <small>{platform.state}</small>
          </div>
          <p>{platform.description}</p>
          <span class="update">{platform.update}</span>
          <a href={platform.href}>{platform.action} <span aria-hidden="true">&rarr;</span></a>
        </article>
      {/each}
    </section>

    <p class="boundary">
      The web beta updates automatically. Mac builds are signed and notarized before release.
      iPhone builds are delivered and updated by Apple through TestFlight.
    </p>
  </main>

  <PublicFooter />
</div>

<style>
  :global(body) { margin: 0; background: #08080a; }
  .page { min-height: 100dvh; background: #08080a; color: #f4f4f6; font-family: var(--mdx-font-body); }
  main { max-width: 1080px; margin: 0 auto; padding: clamp(6rem, 12vw, 10rem) 1.5rem clamp(7rem, 12vw, 11rem); }
  header { max-width: 790px; }
  .eyebrow { margin: 0 0 1.2rem; color: #78a1ff; font-family: var(--mdx-font-mono); font-size: .72rem; font-weight: 800; letter-spacing: .18em; text-transform: uppercase; }
  h1 { margin: 0; font-family: var(--mdx-font-display); font-size: clamp(3.4rem, 7.5vw, 6.7rem); line-height: .96; letter-spacing: -.06em; }
  header > p:last-child { max-width: 620px; margin: 1.6rem 0 0; color: #96969f; font-size: 1.08rem; line-height: 1.7; }
  .platforms { display: grid; margin-top: clamp(4rem, 9vw, 7rem); border-top: 1px solid #29292f; }
  .platforms article { display: grid; grid-template-columns: 170px minmax(0, 1fr) 220px auto; gap: 1.5rem; align-items: center; padding: 2rem 0; border-bottom: 1px solid #29292f; }
  .platform-name { display: grid; gap: .45rem; }.platform-name > span { font-family: var(--mdx-font-display); font-size: 1.5rem; font-weight: 800; }.platform-name small { color: #78a1ff; font-family: var(--mdx-font-mono); font-size: .65rem; letter-spacing: .08em; text-transform: uppercase; }
  article > p { margin: 0; color: #92929b; line-height: 1.6; }.update { color: #6f6f78; font-size: .78rem; }
  article > a { display: inline-flex; min-height: 2.8rem; align-items: center; gap: .7rem; padding: 0 1.15rem; border: 1px solid #303036; border-radius: 999px; color: #eeeeF1; font-size: .8rem; font-weight: 800; text-decoration: none; white-space: nowrap; }
  .boundary { max-width: 680px; margin: 2rem 0 0; color: #5f5f68; font-size: .78rem; line-height: 1.7; }
  @media (max-width: 820px) { .platforms article { grid-template-columns: 1fr auto; }.platform-name { grid-column: 1; } article > p, .update { grid-column: 1 / -1; } article > a { grid-column: 2; grid-row: 1; } }
  @media (max-width: 520px) { main { padding-top: 5.5rem; }.platforms article { grid-template-columns: 1fr; gap: .9rem; } article > a { grid-column: 1; grid-row: auto; justify-self: start; }.platform-name, article > p, .update { grid-column: 1; } }
</style>
