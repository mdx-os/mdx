<script>
  const SEV_ORDER = { p0: 0, P0: 0, high: 1, medium: 2, low: 3 };

  // S4 intake: compose a Forge intent FROM the finding record - summary,
  // taxonomy, evidence receipts, and the expected shape - and pick checks
  // by the surface it names. A person approves the composed intent; no
  // person authors it.
  function forgeIntentFrom(item) {
    const evidence = item.feedback_receipt_id ? ` Evidence receipt: ${item.feedback_receipt_id}.` : "";
    const sev = ["unset", "UNSET", "", null, undefined].includes(item.severity) ? "unrated" : item.severity;
    return (
      `Fix a ${sev} ${item.category ?? "issue"} reported on the ${item.surface ?? "product"} surface: ` +
      `${item.summary ?? item.note ?? "see the linked report"}.` +
      evidence +
      ` Reproduce it first, fix the cause (not the symptom), and prove the fix with the named checks. ` +
      `Do not widen scope beyond this finding.`
    );
  }
  // The beta operating dashboard. One home for the people running the beta:
  // the flywheel ladder up top, the seven operating views, then the triage
  // board and cohort report. Everything here is a read over governed
  // projections and safe telemetry aggregates - counts, kinds, states, never
  // content. Honest-empty when data is thin; honest-down when the kernel is.
  let { data, form } = $props();

  const L = $derived(data.learning.ladder);
  const ladder = $derived([
    { key: "wired", label: "Wired", note: "Real write edges across the surfaces", on: L.wired },
    { key: "turns", label: "Turns", note: "A later lap cites an earlier lesson", on: L.turns },
    { key: "compounds", label: "Compounds", note: "A later lap measurably improves", on: L.compounds },
    { key: "self", label: "Self-improves", note: "Lessons change behavior on their own", on: L.selfImproves, intentionallyClosed: true }
  ]);

  function entries(obj) {
    return Object.entries(obj ?? {}).sort((a, b) => b[1] - a[1]);
  }
  function total(obj) {
    return Object.values(obj ?? {}).reduce((n, v) => n + Number(v), 0);
  }
</script>

<svelte:head><title>Beta - MDx</title></svelte:head>

<section class="beta" data-route-state="ready">
  <header class="beta-head">
    <h1>Beta</h1>
    <p class="sub">How the beta is going: who is in it, what they do, what they learn, and what breaks.</p>
    {#if !data.reachable}
      <p class="down" role="status">Resting - start the local server and this fills in from real beta activity.</p>
    {/if}
  </header>

  <section class="card wide" aria-label="External canary readiness">
    <p class="card-head">External canary <span class="owner">target {data.canary.targetMin}-{data.canary.targetMax} people</span></p>
    <p class="big">{data.canary.participantCount} <span class="big-l">active · {data.canary.firstValueCount} at first value · {data.canary.returnCount} returned · {data.canary.latestMacCount} on {data.canary.latestMacLabel || "a proven Mac release"} · {data.canary.supportBlockerCount} critical support blockers</span></p>
    <p class="card-foot">{data.canary.stage}</p>
    <ul class="rows">
      {#each data.canary.gates as gate (gate.label)}
        <li><span>{gate.label}</span><span>{gate.met ? "met" : "waiting"}</span></li>
      {/each}
    </ul>
  </section>

  <!-- Flywheel ladder: lit only from the proof route, never claimed. -->
  <section class="ladder" aria-label="Flywheel state">
    <p class="card-head">Flywheel</p>
    <ol class="rungs">
      {#each ladder as rung (rung.key)}
        <li class="rung" class:on={rung.on} class:closed={rung.intentionallyClosed && !rung.on}>
          <span class="rung-dot" aria-hidden="true"></span>
          <span class="rung-label">{rung.label}</span>
          <span class="rung-note">{rung.note}</span>
          <span class="rung-state">{rung.on ? "yes" : rung.intentionallyClosed ? "closed on purpose" : "not yet"}</span>
        </li>
      {/each}
    </ol>
    <p class="card-foot">Measured improvement: {data.learning.measuredStatus === "OBSERVED_MEASURED_IMPROVEMENT" ? "observed" : "insufficient samples - needs comparable laps"} ({data.learning.comparablePairs} comparable {data.learning.comparablePairs === 1 ? "pair" : "pairs"}).</p>
  </section>

  <div class="views">
    <!-- 1. Cohort health -->
    <section class="card" aria-label="Cohort health">
      <p class="card-head">Cohort health <span class="owner">{data.owners.cohort}</span></p>
      {#if data.cohort.activeCount > 0}
        <p class="big">{data.cohort.activeCount} <span class="big-l">active {data.cohort.activeCount === 1 ? "participant" : "participants"}</span></p>
        <ul class="rows">
          {#each entries(data.cohort.byCohort) as [k, v] (k)}<li><span>{k}</span><span>{v}</span></li>{/each}
        </ul>
        <p class="card-foot">{data.cohort.analyticsOptOut} opted out of product analytics (still counted in support and security).</p>
      {:else}
        <p class="empty">No one enrolled yet. Enrollments land through the governed beta enrollment record.</p>
      {/if}
    </section>

    <!-- 2. Surface health -->
    <section class="card" aria-label="Surface health">
      <p class="card-head">Surface health <span class="owner">{data.owners.surface}</span></p>
      {#if total(data.telemetry.eventCounts) > 0}
        <ul class="rows">
          {#each entries(data.telemetry.eventCounts) as [k, v] (k)}<li><span>{k}</span><span>{v}</span></li>{/each}
        </ul>
      {:else}
        <p class="empty">No product telemetry yet. Activation and surface-visit events appear once enrolled participants browse.</p>
      {/if}
    </section>

    <!-- 3. Forge health -->
    <section class="card" aria-label="Forge health">
      <p class="card-head">Forge health <span class="owner">{data.owners.forge}</span></p>
      {#if data.forge.runCount > 0}
        <p class="big">{data.forge.runCount} <span class="big-l">{data.forge.runCount === 1 ? "run" : "runs"}</span></p>
        <ul class="rows">
          <li><span>checks passed</span><span>{data.forge.checksPassed}</span></li>
          <li><span>checks failed</span><span>{data.forge.checksFailed}</span></li>
          <li><span>runs with an outcome</span><span>{data.forge.withOutcome}</span></li>
        </ul>
      {:else}
        <p class="empty">No Forge runs yet. Connect a model and hand Forge a scoped build to start the loop.</p>
      {/if}
    </section>

    <!-- 4. Learning and flywheel -->
    <section class="card" aria-label="Learning health">
      <p class="card-head">Learning health <span class="owner">{data.owners.learning}</span></p>
      <ul class="rows">
        <li><span>outcome signals</span><span>{data.learning.outcomes}</span></li>
        <li><span>candidate lessons</span><span>{data.learning.candidates}</span></li>
        <li><span>active (promoted) lessons</span><span>{data.learning.activeMemories}</span></li>
        <li><span>lesson citations</span><span>{data.learning.citationEvents}</span></li>
      </ul>
      <p class="card-foot">Lessons are advisory. Nothing changes behavior on its own.</p>
    </section>

    <!-- 5. Trust and safety -->
    <section class="card" aria-label="Trust and safety">
      <p class="card-head">Trust and safety <span class="owner">{data.owners.trust}</span></p>
      <ul class="rows">
        <li><span>capability execution</span><span>{data.trust.capabilityExecutionAllowed ? "open" : "closed"}</span></li>
        <li><span>quarantined capabilities</span><span>{data.trust.quarantines}</span></li>
      </ul>
      {#if data.trust.blocked && total(data.trust.blocked) > 0}
        <p class="card-foot">Blocked actions: {total(data.trust.blocked)} (held at their gates, as designed).</p>
      {:else}
        <p class="card-foot">No blocked-action events recorded yet.</p>
      {/if}
    </section>

    <!-- 6. Reliability -->
    <section class="card" aria-label="Reliability">
      <p class="card-head">Reliability <span class="owner">{data.owners.reliability}</span></p>
      <p class="card-foot">Interaction budget: {data.reliability.interactionBudgetMs}ms p75.</p>
      {#if data.reliability.notYetInstrumented.length > 0}
        <p class="empty">Not yet measured: {data.reliability.notYetInstrumented.join(", ")}. Named honestly, never faked.</p>
      {:else}
        <p class="card-foot">All named runtime metrics are instrumented.</p>
      {/if}
    </section>

    <!-- 7. Support -->
    <section class="card" aria-label="Support">
      <p class="card-head">Support <span class="owner">{data.owners.support}</span></p>
      {#if data.support.count > 0}
        <p class="big">{data.support.count} <span class="big-l">{data.support.count === 1 ? "report" : "reports"}</span></p>
        <ul class="rows">
          {#each entries(data.support.bySeverity) as [k, v] (k)}<li><span>{["unknown", "unset", ""].includes(k) ? "unrated" : k}</span><span>{v}</span></li>{/each}
        </ul>
      {:else}
        <p class="empty">No feedback yet. Reports arrive through the safe feedback rail, pre-tagged by class and severity.</p>
      {/if}
    </section>
  </div>

  <section class="card wide" aria-label="Backlog">
    <p class="card-head">The backlog that writes itself</p>
    {#if (data.support?.recent ?? []).length > 0}
      <ul class="bk-list">
        {#each [...(data.support?.recent ?? [])].sort((a, b) => (SEV_ORDER[a.severity] ?? 9) - (SEV_ORDER[b.severity] ?? 9)) as item, i (i)}
          <li class="bk-row">
            <span class="sev" data-sev={item.severity}>{["unset", "UNSET", "", null, undefined].includes(item.severity) ? "unrated" : item.severity}</span>
            <span class="bk-line">{item.summary ?? item.note ?? item.category ?? "report"}</span>
            <span class="bk-meta">{item.surface ?? ""} · <span class="st" data-st={item.status}>{item.status ?? "received"}</span></span>
            {#if item.feedback_receipt_id}
              <a class="bk-ev" href={`/evidence/${encodeURIComponent(item.feedback_receipt_id)}`}>evidence</a>
            {/if}
            {#if item.triage_entry_ref}
              <a class="bk-ev" href="/product-direction/triage">decide →</a>
            {/if}
            <a class="bk-compose" href={`/forge?intent=${encodeURIComponent(forgeIntentFrom(item))}`}>hand to Forge →</a>
          </li>
        {/each}
      </ul>
      <p class="bk-note">Every report lands here with its receipt; decisions happen in the triage stream; accepted work becomes a precomposed Forge intent. Simulation feedback (program arc S3) flows in through the same rail.</p>
    {:else}
      <p class="empty">No reports yet. Feedback from every surface (and every simulated persona) lands here with receipts, pre-ordered by severity.</p>
    {/if}
  </section>

  <section class="card wide" aria-label="Participant journeys">
    <p class="card-head">Participant journeys <span class="owner">beta support owner</span></p>
    {#if data.telemetry.journeys.length > 0}
      <ul class="rows">
        {#each data.telemetry.journeys as journey (journey.participant_actor_id)}
          <li>
            <span>{journey.participant_id || journey.participant_actor_id} · {journey.signalSummary}</span>
            <span>Last seen {journey.lastSeenLabel}. {journey.nextAction}</span>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="empty">No participant journey receipts yet. A signed-in web or Mac session starts this view.</p>
    {/if}
  </section>

  <!-- Triage board: the safe feedback captures as a working queue. -->
  <section class="card wide" aria-label="Admissions">
    <p class="card-head">Admissions</p>
    <p class="adm-line">
      Waiting {data.admissions.signups.available ? data.admissions.signups.rows.length : data.admissions.waitlist.length} · invites issued {data.admissions.invites.length} · redeemed {data.admissions.redemptions.length}.
      Issuing records the governed receipt, then delivers one idempotent email through {data.inviteDelivery.provider || "the configured provider"}.
    </p>
    {#if data.admissions.signups.available && data.admissions.signups.rows.length > 0}
      <ul class="adm-list">
        {#each data.admissions.signups.rows.slice(0, 20) as entry, i (entry.id)}
          <li class="adm-row">
            <span class="adm-id">{entry.full_name}<em class="adm-email">{entry.email}</em></span>
            <span class="adm-meta">{entry.role ?? ""}{entry.build_goal ? ` · "${entry.build_goal}"` : ""}</span>
            <form method="POST" action="?/invite">
              <input type="hidden" name="signup_id" value={entry.id} />
              <button type="submit" class="adm-invite" disabled={entry.invite_delivery_status === "sent" || !data.inviteDelivery.ready}>
                {entry.invite_delivery_status === "sent" ? "Delivered ✓" : entry.invite_delivery_status === "failed" ? "Retry delivery" : "Issue and deliver"}
              </button>
            </form>
          </li>
        {/each}
      </ul>
      {#if !data.inviteDelivery.ready}<p class="adm-flash">Invite delivery is held until the server email provider inputs are configured.</p>{/if}
      {#if form?.inviteSuccess}<p class="adm-flash">{form.inviteSuccess}</p>{/if}
      {#if form?.inviteError}<p class="adm-flash">{form.inviteError}</p>{/if}
    {:else if data.admissions.waitlist.length > 0}
      <ul class="adm-list">
        {#each data.admissions.waitlist.slice(0, 12) as entry, i (i)}
          <li class="adm-row">
            <span class="adm-id">{entry.applicant_id ?? entry.email_hash ?? "applicant"}</span>
            <span class="adm-meta">{entry.role ?? ""} · {entry.repo_ecosystem ?? ""} · {entry.build_goal ?? ""}</span>
            <span class="adm-meta">Contact delivery unavailable</span>
          </li>
        {/each}
      </ul>
      <p class="empty">These requests carry a hash only (no contact store is connected here), so share issued invite ids yourself.</p>
    {:else}
      <p class="empty">No one is waiting yet. The public gate is /waitlist - requests land here with name and email.</p>
    {/if}
  </section>

  <section class="card wide" aria-label="Triage board">
    <p class="card-head">Triage board</p>
    {#if data.support.recent.length > 0}
      <ul class="triage">
        {#each data.support.recent as item, i (i)}
          <li class="triage-row">
            <span class="sev" data-sev={item.severity}>{["unset", "UNSET", "", null].includes(item.severity) ? "unrated" : item.severity}</span>
            <span class="cls">{item.category ?? item.failure_class ?? "uncategorized"}</span>
            <span class="srf">{item.surface ?? ""}</span>
            <span class="st" data-st={item.status}>{(item.status ?? "received") === "unknown" ? "uncategorized" : (item.status ?? "received")}</span>
            {#if item.triage_entry_ref}
              <a class="triage-open" href="/product-direction/triage" title="Decide on this in the triage stream (entry {item.triage_entry_ref})">decide →</a>
            {/if}
          </li>
        {/each}
      </ul>
    {:else}
      <p class="empty">Nothing to triage. Reports land here by severity and class, content-free, ready to assign.</p>
    {/if}
    {#if data.kernel}
      <p class="kernel-line">Kernel {data.kernel.version} · {data.kernel.routes} routes · contract {data.kernel.fingerprint} · needs app {data.kernel.minApp}+</p>
    {/if}
  </section>

  <!-- Cohort report: the enrollment-derived roster summary. -->
  <section class="card wide" aria-label="Cohort report">
    <p class="card-head">Cohort report</p>
    {#if data.cohort.participants.length > 0}
      <ul class="roster">
        {#each data.cohort.participants as p, i (i)}
          <li class="roster-row">
            <span class="who">{p.participant_id}</span>
            <span class="tag">{p.cohort_id}</span>
            <span class="tag">{p.role}</span>
            <span class="tag">{p.use_case}</span>
            <span class="tag muted">{p.product_analytics_consent ? "analytics on" : "analytics off"}</span>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="empty">No cohort yet. Once participants enroll, the roster and their cohorts, roles, and consent show here.</p>
    {/if}
  </section>

  <footer class="quiet">
    {data.boundary}
    <span class="next">Safe next: {data.safeNext}</span>
  </footer>
</section>

<style>
  .beta { padding: 8px 4px 40px; max-width: 1100px; }
  .beta-head h1 { margin: 0; font-size: 22px; }
  .sub { margin: 4px 0 0; color: var(--mdx-text-secondary); font-size: 13px; }
  .down { margin: 8px 0 0; color: var(--mdx-text-tertiary); font-size: 12.5px; }

  .ladder { margin: 18px 0 0; padding: 16px 18px; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-lg); background: var(--mdx-surface-raised); }
  .rungs { list-style: none; margin: 12px 0 0; padding: 0; display: grid; grid-template-columns: repeat(4, 1fr); gap: 10px; }
  .rung { display: flex; flex-direction: column; gap: 3px; padding: 12px; border-radius: var(--mdx-radius-md); background: var(--mdx-surface-base); opacity: 0.55; }
  .rung.on { opacity: 1; }
  .rung-dot { width: 9px; height: 9px; border-radius: 50%; background: var(--mdx-text-tertiary); }
  .rung.on .rung-dot { background: var(--mdx-accent-success, var(--mdx-success-green)); }
  .rung.closed .rung-dot { background: var(--mdx-text-tertiary); }
  .rung-label { font-weight: 650; font-size: 14px; color: var(--mdx-text-primary); }
  .rung-note { font-size: 11.5px; color: var(--mdx-text-muted); }
  .rung-state { font-size: 11px; color: var(--mdx-text-tertiary); margin-top: 2px; }

  .views { margin: 16px 0 0; display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 12px; }
  .card { padding: 14px 16px; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-lg); background: var(--mdx-surface-raised); }
  .card.wide { margin-top: 12px; }
  .card-head { margin: 0; font-size: 13px; font-weight: 650; color: var(--mdx-text-primary); display: flex; justify-content: space-between; align-items: baseline; gap: 10px; }
  .owner { font-size: 11px; font-weight: 500; color: var(--mdx-text-tertiary); }
  .big { margin: 10px 0 6px; font-size: 24px; font-weight: 650; color: var(--mdx-text-primary); }
  .big-l { font-size: 12px; font-weight: 500; color: var(--mdx-text-muted); }
  .rows { list-style: none; margin: 8px 0 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }
  .rows li { display: flex; justify-content: space-between; gap: 12px; font-size: 12.5px; }
  .rows li > span:first-child { color: var(--mdx-text-muted); }
  .rows li > span:last-child { color: var(--mdx-text-secondary); font-variant-numeric: tabular-nums; }
  .empty { margin: 10px 0 0; font-size: 12.5px; color: var(--mdx-text-tertiary); }
  .card-foot { margin: 8px 0 0; font-size: 11.5px; color: var(--mdx-text-tertiary); }

  .triage, .roster { list-style: none; margin: 10px 0 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  .triage-row, .roster-row { display: flex; align-items: center; gap: 10px; font-size: 12.5px; padding: 7px 10px; border-radius: var(--mdx-radius-md); background: var(--mdx-surface-base); }
  .sev { font-size: 11px; font-weight: 600; padding: 1px 8px; border-radius: var(--mdx-radius-pill); background: var(--mdx-surface-raised); color: var(--mdx-text-secondary); text-transform: uppercase; }
  .sev[data-sev="P0"], .sev[data-sev="p0"] { color: var(--mdx-accent-danger, var(--mdx-danger-red)); }
  .sev[data-sev="P1"], .sev[data-sev="p1"] { color: var(--mdx-accent-warn, var(--mdx-accent-warning)); }
  .cls { font-weight: 550; color: var(--mdx-text-primary); }
  .srf, .st { color: var(--mdx-text-tertiary); }
  .st { margin-left: auto; }
  .who { font-weight: 600; color: var(--mdx-text-primary); }
  .tag { font-size: 11px; padding: 1px 8px; border-radius: var(--mdx-radius-pill); background: var(--mdx-surface-raised); color: var(--mdx-text-secondary); }
  .tag.muted { color: var(--mdx-text-tertiary); }

  .quiet { display: block; margin: 18px 0 0; font-size: 11.5px; color: var(--mdx-text-tertiary); line-height: 1.5; }
  .next { display: block; margin-top: 4px; }

  @media (max-width: 760px) {
    .rungs { grid-template-columns: 1fr 1fr; }
  }
  .kernel-line { margin: 10px 0 0; font-size: 11.5px; color: var(--mdx-text-muted); font-family: var(--mdx-font-mono, monospace); }
  .adm-line { margin: 0 0 10px; font-size: 12.5px; color: var(--mdx-text-secondary); line-height: 1.5; }
  .adm-list { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; }
  .adm-row { display: flex; align-items: center; gap: 12px; font-size: 12.5px; }
  .adm-id { font-family: var(--mdx-font-mono, monospace); font-size: 11.5px; color: var(--mdx-text-primary); display: grid; }
  .adm-email { font-style: normal; color: var(--mdx-text-muted); font-size: 10.5px; }
  .adm-meta { flex: 1; color: var(--mdx-text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .adm-invite { font: inherit; font-size: 12px; padding: 4px 10px; border-radius: 7px; border: 1px solid var(--mdx-border-subtle); background: var(--mdx-surface-raised); color: var(--mdx-text-primary); cursor: pointer; }
  .adm-flash { margin: 8px 0 0; font-size: 12px; color: var(--mdx-text-secondary); }
  .triage-open { font-size: 11.5px; color: var(--mdx-text-secondary); text-decoration: none; }
  .triage-open:hover { text-decoration: underline; }
  .st[data-st="working"] { color: var(--mdx-accent-primary); }
  .st[data-st="resolved"] { color: var(--mdx-accent-success); }
  .bk-list { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; }
  .bk-row { display: flex; align-items: baseline; gap: 10px; font-size: 12.5px; }
  .bk-line { flex: 1; color: var(--mdx-text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .bk-meta { color: var(--mdx-text-muted); white-space: nowrap; }
  .bk-ev { font-size: 11.5px; color: var(--mdx-text-secondary); }
  .bk-note { margin: 10px 0 0; font-size: 12px; color: var(--mdx-text-muted); line-height: 1.5; }
  .bk-compose { font-size: 11.5px; color: var(--mdx-accent-primary); }
</style>
