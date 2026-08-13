<script>
  export let experience = {
    eyebrow: "Ask Concierge",
    headline: "Ask what MDx knows.",
    summary: "Answers use saved proof and keep write authority out of the conversation.",
    prompt: ""
  };
  export let questions = [];
  export let answerContract = [];
  export let evidence = [];
  export let caveat = "Free-form chat waits for login, rate limits, live answer proof, and saved answer evidence.";

  let query = experience.prompt || "";
  let selectedQuestion = questions[0]?.question ?? "";
  let freeformBlocked = false;

  const normalize = (value) =>
    String(value ?? "")
      .toLowerCase()
      .replace(/[^a-z0-9 ]+/g, " ")
      .replace(/\s+/g, " ")
      .trim();

  const matchScore = (item, value) => {
    const haystack = normalize(`${item.question} ${item.answer} ${item.safeNext}`);
    const terms = normalize(value).split(" ").filter(Boolean);
    if (terms.length === 0) {
      return 1;
    }
    return terms.filter((term) => haystack.includes(term)).length;
  };

  $: matches = query.trim()
    ? questions.filter((item) => matchScore(item, query) > 0)
    : questions;
  $: selected = query.trim()
    ? matches.find((item) => item.question === selectedQuestion) ?? matches[0]
    : questions.find((item) => item.question === selectedQuestion) ?? matches[0] ?? questions[0];

  function chooseQuestion(item) {
    selectedQuestion = item.question;
    query = item.question;
    freeformBlocked = false;
  }

  function handleKeydown(event) {
    if (!((event.metaKey || event.ctrlKey) && event.key === "Enter")) {
      return;
    }
    event.preventDefault();
    if (matches.length > 0) {
      chooseQuestion(matches[0]);
    } else {
      freeformBlocked = true;
    }
  }
</script>

<section class="ask-panel" aria-label="Ask Concierge">
  <article class="ask-primary">
    <p>{experience.eyebrow}</p>
    <h2>{experience.headline}</h2>
    <span>{experience.summary}</span>

    <label class="question-preview" for="concierge-local-question">
      <strong>Ask a grounded question</strong>
      <textarea
        id="concierge-local-question"
        bind:value={query}
        onkeydown={handleKeydown}
        rows="3"
        placeholder="Try trust, missing, held back, or live authority"
        aria-describedby="concierge-ask-caveat"
      ></textarea>
      <small id="concierge-ask-caveat">Local matching only. Choose a saved answer below.</small>
    </label>

    {#if freeformBlocked || (query.trim() && matches.length === 0)}
      <div class="blocked-answer" role="status">
        <strong>Not live chat yet.</strong>
        <span>{caveat} Try one of the saved questions below.</span>
      </div>
    {/if}
  </article>

  <div class="suggested-questions" aria-label="Suggested grounded questions">
    {#each matches.slice(0, 4) as item}
      <a
        href="#concierge-answer"
        aria-current={selected?.question === item.question ? "true" : undefined}
        onclick={(event) => {
          event.preventDefault();
          chooseQuestion(item);
        }}
      >
        <p>{item.question}</p>
        <h2>{item.answer}</h2>
        <span>{item.safeNext}</span>
      </a>
    {/each}
  </div>

  {#if selected}
    <article class="conversation-answer" id="concierge-answer" aria-label="Grounded Concierge answer">
      <p>Concierge answer</p>
      <h2>{selected.question}</h2>
      <span>{selected.answer} {selected.safeNext}</span>
      <div class="citation-row" aria-label="Cited evidence">
        {#each evidence.slice(0, 4) as item, index}
          <span>[{index + 1}] {item}</span>
        {/each}
      </div>
      <details>
        <summary>Why I said this</summary>
        {#each answerContract as item}
          <small>{item}</small>
        {/each}
        <small>{caveat}</small>
      </details>
    </article>
  {/if}
</section>

<style>
  .ask-panel {
    display: grid;
    grid-template-columns: minmax(340px, 1.08fr) minmax(320px, 0.92fr);
    gap: 12px;
    margin-top: 20px;
  }

  .ask-primary,
  .conversation-answer,
  .suggested-questions a {
    border: 1px solid var(--mdx-border-soft);
    border-radius: var(--mdx-radius-panel);
    background: rgba(var(--mdx-surface-rgb), 0.86);
  }

  .ask-primary {
    min-height: 300px;
    padding: 16px;
    background:
      linear-gradient(135deg, rgba(var(--mdx-focus-blue-rgb), 0.16), rgba(var(--mdx-evidence-cyan-rgb), 0.08)),
      rgba(var(--mdx-surface-raised-rgb), 0.82);
  }

  p,
  h2,
  span,
  small {
    margin: 0;
  }

  p {
    color: var(--mdx-text-caption);
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0;
    font-weight: 800;
  }

  h2 {
    margin-top: 8px;
    color: var(--mdx-text-primary);
    line-height: 1.1;
  }

  .ask-primary h2 {
    max-width: 780px;
    font-size: 42px;
    line-height: 1.04;
  }

  .ask-primary > span,
  .conversation-answer > span,
  .suggested-questions span {
    display: block;
    margin-top: 10px;
    color: var(--mdx-text-secondary);
    font-size: 13px;
    line-height: 1.45;
  }

  .question-preview {
    display: grid;
    gap: 9px;
    margin-top: 28px;
    border: 1px solid rgba(var(--mdx-evidence-cyan-rgb), 0.28);
    border-radius: var(--mdx-radius-panel);
    padding: 14px;
    background: rgba(var(--mdx-bg-rgb), 0.42);
  }

  .question-preview strong {
    color: var(--mdx-text-primary);
    font-size: 14px;
  }

  textarea {
    width: 100%;
    min-height: 94px;
    border: 1px solid var(--mdx-border-chip);
    border-radius: var(--mdx-radius-control);
    padding: 12px;
    resize: vertical;
    color: var(--mdx-text-primary);
    background: rgba(var(--mdx-surface-rgb), 0.74);
    font: inherit;
    line-height: 1.45;
  }

  textarea::placeholder {
    color: var(--mdx-text-muted);
  }

  textarea:focus {
    outline: 2px solid var(--mdx-focus-blue);
    outline-offset: 2px;
  }

  .question-preview small,
  details small {
    display: block;
    color: var(--mdx-text-secondary);
    font-size: 12px;
    line-height: 1.45;
  }

  .blocked-answer {
    display: grid;
    gap: 6px;
    margin-top: 12px;
    border-left: 2px solid var(--mdx-warning-amber);
    padding-left: 12px;
  }

  .blocked-answer strong {
    color: var(--mdx-warning-amber);
    font-size: 13px;
  }

  .suggested-questions {
    display: grid;
    gap: 12px;
  }

  .suggested-questions a {
    display: block;
    padding: 14px;
    color: var(--mdx-text-primary);
    text-decoration: none;
    transition:
      border-color var(--mdx-motion-base) var(--mdx-ease),
      background-color var(--mdx-motion-base) var(--mdx-ease),
      transform var(--mdx-motion-fast) var(--mdx-ease);
  }

  .suggested-questions a:hover,
  .suggested-questions a[aria-current="true"] {
    border-color: rgba(var(--mdx-evidence-cyan-rgb), 0.46);
    background: rgba(var(--mdx-surface-raised-rgb), 0.82);
    transform: translateY(-1px);
  }

  .suggested-questions a:focus-visible {
    outline: 2px solid var(--mdx-focus-blue);
    outline-offset: 3px;
  }

  .suggested-questions h2 {
    color: var(--mdx-success-green);
    font-size: 18px;
  }

  .conversation-answer {
    grid-column: 1 / -1;
    padding: 16px;
  }

  .conversation-answer h2 {
    font-size: 24px;
  }

  .citation-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 14px;
  }

  .citation-row span {
    border: 1px solid var(--mdx-border-chip);
    border-radius: var(--mdx-radius-pill);
    padding: 7px 10px;
    color: var(--mdx-text-secondary);
    background: rgba(var(--mdx-surface-rgb), 0.68);
    font-size: 12px;
    line-height: 1;
  }

  details {
    margin-top: 14px;
    border-top: 1px solid var(--mdx-border-soft);
    padding-top: 12px;
  }

  summary {
    cursor: pointer;
    color: var(--mdx-evidence-cyan);
    font-size: 13px;
    font-weight: 700;
  }

  @media (max-width: 860px) {
    .ask-panel {
      grid-template-columns: 1fr;
    }

    .ask-primary h2 {
      font-size: 32px;
    }
  }
</style>
