<script>
  // Help: the concepts an engineer needs after onboarding ends. The welcome
  // flow explains all of this once and then 307s away forever; this page is
  // where that knowledge lives permanently. Content is explanation only -
  // every claim here matches how the kernel actually behaves.
  const concepts = [
    {
      term: "Run",
      meaning:
        "One agent doing one piece of work: it reads the repo in an isolated copy, makes the change, runs the checks you allowed, and comes back with a branch. It never deploys."
    },
    {
      term: "Receipt",
      meaning:
        "The durable, signed record of something that happened - every ask, run, decision, and publish leaves one. Receipt ids link to their evidence view anywhere you see them."
    },
    {
      term: "Review",
      meaning:
        "Where finished work waits for your call. You see the diff, the checks that ran, and the proof packet - then ship it, ask for changes, or stop it. Nothing lands without a human decision."
    },
    {
      term: "Mission",
      meaning:
        "Long-horizon work shaped as a goal with milestones, a budget, and checkpoints. A mission is admitted and planned; live workers only move when execution is turned on."
    },
    {
      term: "Fleet",
      meaning:
        "Several workers on one ask, split along real seams with disjoint write scopes, plus an integration lane that brings the pieces together for one review."
    },
    {
      term: "Twin",
      meaning:
        "Your thinking partner. It reasons from your context and remembers, but it never acts - when thinking becomes work, hand it to Forge with one click."
    },
    {
      term: "Ship decision",
      meaning:
        "The human verb that accepts a run's change. It records who decided and why, writes the changelog entry, and opens the road to a PR - push authority stays governed."
    },
    {
      term: "Governed / fail-closed",
      meaning:
        "Anything that could write beyond this machine starts locked and refuses honestly until the policy, authority, and proof line up. A refusal is the system working, not breaking."
    }
  ];

  const shortcuts = [
    { keys: "⌘K", does: "Command palette - go anywhere, or type a sentence to start a build or ask Twin" },
    { keys: "⌘1-6", does: "Jump between Twin, Message, Forge, Memory, Marketplace, and Runs" },
    { keys: "Esc", does: "Close palettes, dialogs, and disclosures" },
    { keys: "1 / 2 / 3 / h", does: "Verdict keys on the triage stream" }
  ];
</script>

<main class="help">
  <h1>Help</h1>
  <p class="sub">What the words mean, what the keys do, and where to go when something looks wrong.</p>

  <section aria-label="Concepts">
    <h2>The concepts</h2>
    <dl class="concepts">
      {#each concepts as item (item.term)}
        <div class="concept">
          <dt>{item.term}</dt>
          <dd>{item.meaning}</dd>
        </div>
      {/each}
    </dl>
  </section>

  <section aria-label="Keyboard shortcuts">
    <h2>Keyboard</h2>
    <ul class="keys">
      {#each shortcuts as shortcut (shortcut.keys)}
        <li><kbd>{shortcut.keys}</kbd><span>{shortcut.does}</span></li>
      {/each}
    </ul>
  </section>

  <section aria-label="When something looks wrong">
    <h2>When something looks wrong</h2>
    <p>
      A refusal with a reason means MDx is protecting a boundary - read the reason, it says what to do next.
      A page that says MDx isn't reachable means the local server is down, not that anything is lost.
      For everything else, use <strong>Report an issue</strong> (the quiet button on every surface) -
      it files with a receipt and the team sees it on the triage board.
    </p>
    <p class="links">
      <a href="/changelog">What shipped recently &rarr;</a>
      <a href="/proof">The live proof room &rarr;</a>
    </p>
  </section>
</main>

<style>
  .help { max-width: 720px; margin: 0 auto; padding: 40px 24px 60px; }
  h1 { font-size: 24px; margin: 0 0 4px; }
  .sub { margin: 0 0 26px; font-size: 14px; color: var(--mdx-text-secondary); }
  h2 { font-size: 15px; margin: 26px 0 10px; }
  .concepts { margin: 0; display: grid; gap: 10px; }
  .concept { border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-md); background: var(--mdx-surface-base); padding: 12px 14px; }
  dt { font-weight: 650; font-size: 13.5px; margin: 0 0 3px; }
  dd { margin: 0; font-size: 13.5px; color: var(--mdx-text-secondary); line-height: 1.55; }
  .keys { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; }
  .keys li { display: flex; align-items: baseline; gap: 12px; font-size: 13.5px; }
  .keys kbd { flex: 0 0 auto; min-width: 58px; text-align: center; font-family: var(--mdx-font-mono, monospace); font-size: 12px; border: 1px solid var(--mdx-border-subtle); border-radius: 5px; padding: 2px 8px; background: var(--mdx-surface-raised); }
  .keys span { color: var(--mdx-text-secondary); }
  section p { font-size: 13.5px; color: var(--mdx-text-secondary); line-height: 1.6; }
  .links { display: flex; gap: 18px; }
  .links a { color: var(--mdx-text-primary); }
</style>
