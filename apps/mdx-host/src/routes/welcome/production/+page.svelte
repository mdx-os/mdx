<script>
  // First-run orientation for the Prepare-for-Production path. Cloud/infra
  // operators need two things before a checklist makes sense: what MDx actually
  // IS as a system (topology, state, trust boundaries) and what they own when
  // it leaves this machine. We give them a system map in their own language,
  // then the few decisions that shape everything downstream.
  let { data } = $props();
  const greeting = $derived(data.ownerName ? `${data.ownerName}, ` : "");

  // The system, grouped by what an operator actually reasons about. "here" =
  // running locally today; "cloud" = the part the operator provisions and owns.
  const planes = [
    {
      plane: "Runs MDx",
      parts: [
        { name: "MDx host", what: "Web UI and the API edge", where: "here" },
        { name: "Kernel + runtime", what: "Governs every write; emits receipts", where: "here" }
      ]
    },
    {
      plane: "Holds state",
      parts: [
        { name: "Postgres", what: "Snapshot file locally; managed database in the cloud", where: "cloud" }
      ]
    },
    {
      plane: "Guards access",
      parts: [
        { name: "Secret manager", what: "Named env slots - values never live in MDx", where: "cloud" },
        { name: "External IdP", what: "Login, tenant, and role claims for real users", where: "cloud" }
      ]
    },
    {
      plane: "Reaches out",
      parts: [
        { name: "Model provider", what: "The model you connected answers here", where: "here" },
        { name: "Message relay", what: "Realtime delivery path at scale", where: "cloud" }
      ]
    }
  ];

  const decisions = [
    { q: "Which target first?", a: "Local, Render, AWS, GCP, or Kubernetes - it shapes every row below." },
    { q: "Where do secrets live?", a: "Which secret manager, and which runtime identity may read them." },
    { q: "Who owns login?", a: "The IdP that issues tenant, role, and session claims." },
    { q: "Who signs off?", a: "Data migration, rollback, incident response, and cutover approvers." }
  ];
</script>

<section class="onboard">
  <header>
    <p class="eyebrow">Prepare for Production</p>
    <h1>First, the lay of the land.</h1>
    <p class="lede">
      {greeting}your local install works with {data.modelName || "a connected model"}. Production is
      a separate path: it's the same system, but you own where it runs, where its data lives, and who
      gets in. Here's the shape before the checklist.
    </p>
  </header>

  <figure class="map" aria-label="What MDx is, as a system">
    <div class="planes">
      {#each planes as group}
        <section class="plane">
          <h2>{group.plane}</h2>
          {#each group.parts as part}
            <div class="part">
              <div class="part-head">
                <strong>{part.name}</strong>
                <span class="where" class:cloud={part.where === "cloud"}>
                  {part.where === "cloud" ? "You provide in cloud" : "Running here"}
                </span>
              </div>
              <p>{part.what}</p>
            </div>
          {/each}
        </section>
      {/each}
    </div>
    <figcaption>
      Everything marked <em>running here</em> already works on this machine. Everything marked
      <em>you provide</em> is what production setup is about: standing up the real thing and proving
      it before traffic arrives.
    </figcaption>
  </figure>

  <section class="decisions" aria-labelledby="first-decisions">
    <div class="decisions-head">
      <p class="eyebrow">Before you read rows</p>
      <h2 id="first-decisions">Four decisions shape the rest.</h2>
    </div>
    <ul>
      {#each decisions as item}
        <li>
          <strong>{item.q}</strong>
          <span>{item.a}</span>
        </li>
      {/each}
    </ul>
  </section>

  <div class="nav">
    <a class="ghost" href="/welcome/path">All paths</a>
    <a class="primary" href="/admin/setup">Open the production checklist &rarr;</a>
  </div>
</section>

<style>
  .onboard {
    max-width: 54rem;
    margin: 0 auto;
    padding: 3.5rem 1.5rem 5rem;
    display: grid;
    gap: 1.75rem;
  }
  header {
    display: grid;
    gap: 0.75rem;
  }
  .eyebrow {
    margin: 0;
    font-size: var(--mdx-text-sm);
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--mdx-text-tertiary);
  }
  h1 {
    margin: 0;
    font-family: var(--mdx-font-display);
    color: var(--mdx-text-primary);
    font-size: clamp(2.25rem, 6.5vw, 4rem);
    line-height: 1;
    max-width: 42rem;
  }
  h2 {
    margin: 0;
    font-family: var(--mdx-font-display);
    color: var(--mdx-text-primary);
    font-size: var(--mdx-text-lg);
  }
  .lede {
    margin: 0;
    max-width: 44rem;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-lg);
    line-height: 1.45;
  }

  /* The system map. */
  .map {
    margin: 0;
    display: grid;
    gap: 0.85rem;
  }
  .planes {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.75rem;
  }
  .plane {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
    padding: 1rem;
    display: grid;
    gap: 0.6rem;
    align-content: start;
  }
  .plane > h2 {
    font-size: var(--mdx-text-sm);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--mdx-text-tertiary);
    font-family: inherit;
    font-weight: 700;
  }
  .part {
    display: grid;
    gap: 0.25rem;
  }
  .part-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
  }
  .part-head strong {
    color: var(--mdx-text-primary);
  }
  .where {
    font-size: var(--mdx-text-xs);
    font-weight: 600;
    padding: 0.1rem 0.5rem;
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-tone-ok-surface, var(--mdx-surface-sunken));
    color: var(--mdx-tone-ok-text);
    white-space: nowrap;
  }
  .where.cloud {
    background: var(--mdx-surface-sunken);
    color: var(--mdx-text-tertiary);
  }
  .part p {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
    line-height: 1.45;
  }
  .map figcaption {
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
    line-height: 1.5;
  }
  .map figcaption em {
    font-style: normal;
    font-weight: 600;
    color: var(--mdx-text-primary);
  }

  /* First decisions. */
  .decisions {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
    padding: 1.25rem;
    display: grid;
    gap: 0.85rem;
  }
  .decisions ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.85rem;
  }
  .decisions li {
    display: grid;
    gap: 0.2rem;
  }
  .decisions strong {
    color: var(--mdx-text-primary);
  }
  .decisions span {
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
    line-height: 1.45;
  }

  .nav {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: center;
  }
  .primary,
  .ghost {
    border-radius: var(--mdx-radius-pill);
    padding: 0.85rem 1.35rem;
    text-decoration: none;
    font-weight: 700;
    white-space: nowrap;
  }
  .primary {
    color: white;
    background: var(--mdx-accent-primary);
  }
  .ghost {
    color: var(--mdx-text-secondary);
  }

  @media (max-width: 760px) {
    .planes,
    .decisions ul {
      grid-template-columns: 1fr;
    }
    .nav {
      flex-direction: column-reverse;
      align-items: stretch;
    }
    .nav .primary,
    .nav .ghost {
      text-align: center;
    }
  }
</style>
