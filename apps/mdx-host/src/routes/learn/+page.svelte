<script>
  import { createMdxClient } from "@mdx/client";
  import { invalidateAll } from "$app/navigation";

  let { data } = $props();

  const activeMemories = $derived(data.activeMemories ?? []);
  const adaptationProposals = $derived(data.adaptationProposals ?? []);
  const evidenceRequests = $derived(data.evidenceRequests?.requests ?? []);
  const ledger = $derived(data.ledger);
  const blockedCount = $derived(ledger.adaptation_queue.length);
  const firstPath = $derived(data.contract.first_proof_path);
  const forgeOutcomes = $derived(data.forgeOutcomes ?? []);
  // The kernel's candidate lessons from finished Forge runs. Citation-only:
  // a human promotes one before any plan can cite it.
  const candidateLessons = $derived(data.candidateLessons ?? []);
  const judgmentDecisions = $derived(data.judgmentDecisions ?? []);
  const lessonPromotions = $derived(data.lessonPromotions ?? []);
  const judgmentRecords = $derived(data.judgmentRecords ?? []);
  const memoryCandidates = $derived(data.memoryCandidates ?? []);
  const memoryQueue = $derived(data.memoryQueue ?? []);
  const memoryTargets = $derived(data.memoryTargets ?? []);
  const client = $derived(createMdxClient({ baseUrl: "/api/kernel", session: data.session }));
  const actor = $derived(data.session?.user_id ?? "local_user");

  let evidenceBusy = $state("");
  let evidenceFlash = $state(null);
  let judgmentBusy = $state("");
  let judgmentFlash = $state(null);
  let memoryBusy = $state("");
  let memoryFlash = $state(null);
  let activationBusy = $state("");
  let activationFlash = $state(null);
  let retireBusy = $state("");
  let retireFlash = $state(null);
  let retireOpenFor = $state("");
  let retireReason = $state("");
  let retireOwner = $state("");
  // Governed "steer casting" state: a lesson a person lets shape future fleet
  // plans, and the withdraw that takes it back.
  const castingGrants = $derived(data.castingGrants ?? []);
  let steerBusy = $state("");
  let steerFlash = $state(null);
  let steerOpenFor = $state("");
  let steerReason = $state("");
  let steerOwner = $state("");
  let steerSlot = $state("");
  let ratifyBusy = $state("");
  let ratifyFlash = $state(null);
  // Edit-and-confirm state for drafted lessons: per-candidate edits, the
  // in-flight step label, the receipts each save leaves behind, and the
  // set-aside form.
  let draftEdits = $state({});
  let saveBusy = $state("");
  let saveStep = $state("");
  let saveFlash = $state(null);
  let savedReceipts = $state(null);
  let rejectOpenFor = $state("");
  let rejectReason = $state("");
  let rejectBusy = $state("");

  const pendingConsolidations = $derived(data.pendingConsolidations ?? []);
  const candidateRejections = $derived(data.candidateRejections ?? []);
  const memoryTargetTypes = $derived(
    data.memoryPromotionContract?.valid_target_types ?? ["decision_record"]
  );

  function draftFieldsFor(candidate) {
    const edits = draftEdits[candidate.candidate_id] ?? {};
    return {
      lesson_summary: (edits.lesson_summary ?? candidate.lesson_summary ?? "").trim(),
      target_type: edits.target_type ?? candidate.target_type ?? "decision_record",
      target_path: edits.target_type
        ? targetPathFor(edits.target_type)
        : candidate.target_path || targetPathFor(candidate.target_type),
      evidence_refs: (edits.evidence_refs ?? candidate.evidence_refs ?? "").trim(),
      applicability_work_tiers: (
        edits.applicability_work_tiers ??
        candidate.applicability_work_tiers ??
        ""
      ).trim(),
      applicability_language_packs: (
        edits.applicability_language_packs ??
        candidate.applicability_language_packs ??
        ""
      ).trim()
    };
  }

  function editDraft(candidate, field, value) {
    draftEdits = {
      ...draftEdits,
      [candidate.candidate_id]: {
        ...(draftEdits[candidate.candidate_id] ?? {}),
        [field]: value
      }
    };
  }

  function targetPathFor(targetType) {
    return (
      {
        page: "docs/MDX-LEARNING-LOOP.md",
        decision_record: "generated/learning/forge-outcome-memory-targets.json",
        generated_contract: "generated/learning/mdx-learning-loop-contract.json",
        source_map: "generated/source-map/mdx-source-map.json",
        model_scorecard: "generated/learning/model-worker-scorecard-targets.json",
        worker_scorecard: "generated/learning/model-worker-scorecard-targets.json"
      }[targetType] ?? "generated/learning/forge-outcome-memory-targets.json"
    );
  }

  // The one confirm that carries a drafted lesson through its three governed
  // writes in order: judgment, memory candidate, active memory. Each write
  // returns its own receipt; all three are shown. If a step refuses, we stop
  // there and say which receipts already exist.
  async function keepDraft(candidate) {
    if (saveBusy || rejectBusy) return;
    const fields = draftFieldsFor(candidate);
    if (!fields.lesson_summary) {
      saveFlash = { ok: false, line: "Write the lesson first; an empty one cannot be kept." };
      return;
    }
    saveBusy = candidate.candidate_id;
    saveFlash = null;
    savedReceipts = null;
    try {
      saveStep = "Recording your judgment";
      const judgment = await client.write(
        "/learning/judgment-decisions.json",
        {
          actor_id: actor,
          judgment_id: candidate.judgment_id,
          promotion_id: candidate.promotion_id,
          decision: "promote_candidate",
          rationale: "Read and edited on the Learn page, worth keeping.",
          evidence_refs: fields.evidence_refs || candidate.source_receipt_id
        },
        { receiptIntent: "learning_judgment_decision" }
      );
      if (judgment.status !== "RECORDED") {
        saveFlash = { ok: false, line: judgment.reason ?? "The judgment did not record, so nothing else was written." };
        return;
      }
      saveStep = "Recording the memory candidate";
      const promotion = await client.write(
        "/learning/memory-promotions.json",
        {
          actor_id: actor,
          judgment_decision_id: judgment.judgment_decision_id,
          judgment_decision_receipt_id: judgment.judgment_decision_receipt_id,
          judgment_id: candidate.judgment_id,
          promotion_id: candidate.promotion_id,
          target_type: fields.target_type,
          target_path: fields.target_path,
          lesson_summary: fields.lesson_summary,
          evidence_refs: [fields.evidence_refs || candidate.source_receipt_id],
          review_cadence: "review before memory activation",
          expiry_rule: "retire this lesson when it stops holding",
          applicability_work_tiers: fields.applicability_work_tiers,
          applicability_language_packs: fields.applicability_language_packs
        },
        { receiptIntent: "learning_memory_promotion" }
      );
      if (promotion.status !== "RECORDED") {
        saveFlash = {
          ok: false,
          line: promotion.reason ?? "The memory candidate did not record. Your judgment is already on the record."
        };
        return;
      }
      saveStep = "Recording the active memory";
      const activation = await client.write(
        "/learning/memory-activations.json",
        {
          actor_id: actor,
          memory_promotion_id: promotion.memory_promotion_id,
          memory_promotion_receipt_id: promotion.memory_promotion_receipt_id,
          judgment_decision_receipt_id: judgment.judgment_decision_receipt_id,
          target_type: fields.target_type,
          target_path: fields.target_path,
          lesson_summary: fields.lesson_summary,
          evidence_refs: [
            promotion.memory_promotion_receipt_id,
            judgment.judgment_decision_receipt_id,
            fields.evidence_refs || candidate.source_receipt_id
          ],
          activation_basis: "A person read and edited this lesson on the Learn page.",
          rollback_plan: "Retire this lesson through a later receipt if it stops holding.",
          local_checks: ["make learning-loop-check"],
          approval_refs: [promotion.memory_promotion_receipt_id],
          review_owner: actor,
          applicability_work_tiers: fields.applicability_work_tiers,
          applicability_language_packs: fields.applicability_language_packs
        },
        { receiptIntent: "learning_memory_activation" }
      );
      if (activation.status !== "RECORDED") {
        saveFlash = {
          ok: false,
          line: activation.reason ?? "The activation did not record. The judgment and memory candidate are already on the record."
        };
        return;
      }
      savedReceipts = {
        judgment: judgment.judgment_decision_receipt_id,
        promotion: promotion.memory_promotion_receipt_id,
        activation: activation.active_memory_receipt_id
      };
      saveFlash = {
        ok: true,
        line: "Lesson kept. Your judgment, the memory candidate, and the active memory each carry a receipt below."
      };
      await invalidateAll();
    } catch (error) {
      saveFlash = { ok: false, line: "Nothing was recorded. Try again." };
    } finally {
      saveBusy = "";
      saveStep = "";
    }
  }

  function openReject(candidate) {
    rejectOpenFor = candidate.candidate_id;
    rejectReason = "";
  }

  function closeReject() {
    rejectOpenFor = "";
    rejectReason = "";
  }

  // The accountable no: a governed write that cites the source receipt, so a
  // rejected draft stays in the record but never comes back as a suggestion.
  async function rejectDraft(candidate) {
    if (rejectBusy || saveBusy) return;
    const reason = rejectReason.trim();
    if (!reason) {
      saveFlash = { ok: false, line: "Say why this one is not worth keeping." };
      return;
    }
    rejectBusy = candidate.candidate_id;
    saveFlash = null;
    try {
      const packet = await client.write(
        "/learning/candidate-rejections.json",
        {
          actor_id: actor,
          candidate_id: candidate.candidate_id,
          source_receipt_id: candidate.source_receipt_id,
          reason,
          review_owner: actor
        },
        { receiptIntent: "learning_candidate_rejection" }
      );
      if (packet.status === "RECORDED") {
        saveFlash = {
          ok: true,
          line: "Rejected, with a receipt. It stays in the record and will not be suggested again."
        };
        closeReject();
        await invalidateAll();
      } else {
        saveFlash = { ok: false, line: packet.reason ?? "That rejection did not record." };
      }
    } catch (error) {
      saveFlash = { ok: false, line: "Nothing was recorded. Try again." };
    }
    rejectBusy = "";
  }

  function scopeLabel(scope) {
    return (
      {
        team_memory: "Team",
        company_memory: "Company",
        project_memory: "Project",
        agent_operational_memory: "Operations"
      }[scope] ?? "Shared"
    );
  }

  async function ratifyConsolidation(memory, decision) {
    if (ratifyBusy) return;
    ratifyBusy = `${memory.memory_id}:${decision}`;
    ratifyFlash = null;
    try {
      const packet = await client.write(
        "/memory/consolidation-ratifications.json",
        {
          actor_id: actor,
          memory_id: memory.memory_id,
          decision,
          note:
            decision === "approve"
              ? "Reviewed on the Learn page and worth remembering."
              : "Reviewed on the Learn page and not worth remembering."
        },
        { receiptIntent: "memory_consolidation_ratification" }
      );
      if (packet.status === "RECORDED") {
        ratifyFlash = {
          ok: true,
          line:
            decision === "approve"
              ? "Approved. This memory can now inform recall for its scope."
              : "Declined. It stays in the record but will not be recalled."
        };
        await invalidateAll();
      } else {
        ratifyFlash = {
          ok: false,
          line: packet.reason ?? "That review did not record."
        };
      }
    } catch (error) {
      ratifyFlash = { ok: false, line: "Nothing was recorded. Try again." };
    }
    ratifyBusy = "";
  }

  function canReview(memory) {
    return memory.proposed_by?.replace("human:", "") !== actor;
  }

  async function askForMoreEvidence(record) {
    if (evidenceBusy) return;
    evidenceBusy = record.judgment_id;
    evidenceFlash = null;
    try {
      const requestedEvidence = (record.missing_evidence ?? []).join("; ");
      const packet = await client.write(
        "/learning/evidence-requests.json",
        {
          actor_id: actor,
          judgment_id: record.judgment_id,
          promotion_id: record.promotion_id,
          reason: "Judgment is not ready until the missing evidence is attached.",
          requested_evidence: requestedEvidence || "Attach the missing evaluation and receipt evidence."
        },
        { receiptIntent: "learning_evidence_request" }
      );
      if (packet.status === "RECORDED") {
        evidenceFlash = {
          ok: true,
          line: "Evidence request recorded. Memory and adaptation are still blocked."
        };
        await invalidateAll();
      } else {
        evidenceFlash = {
          ok: false,
          line: packet.reason ?? "That evidence request did not record."
        };
      }
    } catch (error) {
      evidenceFlash = { ok: false, line: "Nothing was recorded. Try again." };
    }
    evidenceBusy = "";
  }

  async function recordJudgmentDecision(record, decision) {
    if (judgmentBusy) return;
    judgmentBusy = `${record.judgment_id}:${decision}`;
    judgmentFlash = null;
    try {
      const packet = await client.write(
        "/learning/judgment-decisions.json",
        {
          actor_id: actor,
          judgment_id: record.judgment_id,
          promotion_id: record.promotion_id,
          decision,
          rationale: rationaleFor(decision, record),
          evidence_refs: evidenceRefsFor(record)
        },
        { receiptIntent: "learning_judgment_decision" }
      );
      if (packet.status === "RECORDED") {
        judgmentFlash = {
          ok: true,
          line:
            packet.memory_queue_state === "ready_for_memory"
              ? "Judgment recorded. The lesson is queued for memory review, but no memory was written."
              : "Judgment recorded. Memory and adaptation remain blocked."
        };
        await invalidateAll();
      } else {
        judgmentFlash = {
          ok: false,
          line: packet.reason ?? "That judgment decision did not record."
        };
      }
    } catch (error) {
      judgmentFlash = { ok: false, line: "Nothing was recorded. Try again." };
    }
    judgmentBusy = "";
  }

  async function requestMemoryPromotion(item) {
    if (memoryBusy) return;
    memoryBusy = item.judgment_decision_id;
    memoryFlash = null;
    try {
      const packet = await client.write(
        "/learning/memory-promotions.json",
        {
          actor_id: actor,
          judgment_decision_id: item.judgment_decision_id,
          judgment_decision_receipt_id: item.judgment_decision_receipt_id,
          judgment_id: item.judgment_id,
          promotion_id: item.promotion_id,
          target_type: item.target_type,
          target_path: item.target_path,
          lesson_summary: item.lesson_summary,
          evidence_refs: item.evidence_refs ? [item.evidence_refs] : ["judgment decision receipt"],
          review_cadence: "review before memory activation",
          expiry_rule: "supersede this candidate when the source lesson becomes stale"
        },
        { receiptIntent: "learning_memory_promotion" }
      );
      if (packet.status === "RECORDED") {
        memoryFlash = {
          ok: true,
          line: "Memory candidate recorded. Active memory and adaptation remain blocked."
        };
        await invalidateAll();
      } else {
        memoryFlash = {
          ok: false,
          line: packet.reason ?? "That memory promotion did not record."
        };
      }
    } catch (error) {
      memoryFlash = { ok: false, line: "Nothing was recorded. Try again." };
    }
    memoryBusy = "";
  }

  async function activateMemory(candidate) {
    if (activationBusy) return;
    activationBusy = candidate.memory_promotion_id;
    activationFlash = null;
    try {
      const evidenceRefs = [
        candidate.memory_promotion_receipt_id,
        candidate.judgment_decision_receipt_id,
        candidate.evidence_refs
      ].filter(Boolean);
      const packet = await client.write(
        "/learning/memory-activations.json",
        {
          actor_id: actor,
          memory_promotion_id: candidate.memory_promotion_id,
          memory_promotion_receipt_id: candidate.memory_promotion_receipt_id,
          judgment_decision_receipt_id: candidate.judgment_decision_receipt_id,
          target_type: candidate.target_type,
          target_path: candidate.target_path,
          lesson_summary: candidate.lesson_summary,
          evidence_refs: evidenceRefs,
          activation_basis: "Memory candidate reviewed with required local proof.",
          rollback_plan: candidate.expiry_rule || "Supersede through a later memory receipt.",
          local_checks: ["make learning-loop-check", "make source-map-check"],
          approval_refs: [candidate.memory_promotion_receipt_id],
          review_owner: actor
        },
        { receiptIntent: "learning_memory_activation" }
      );
      if (packet.status === "RECORDED") {
        activationFlash = {
          ok: true,
          line: "Active memory recorded. Runtime behavior and adaptation remain blocked."
        };
        await invalidateAll();
      } else {
        activationFlash = {
          ok: false,
          line: packet.reason ?? "That memory activation did not record."
        };
      }
    } catch (error) {
      activationFlash = { ok: false, line: "Nothing was recorded. Try again." };
    }
    activationBusy = "";
  }

  function openRetire(memory) {
    retireOpenFor = memory.active_memory_receipt_id;
    retireReason = "";
    retireOwner = actor;
    retireFlash = null;
  }

  function closeRetire() {
    retireOpenFor = "";
    retireReason = "";
  }

  async function retireLesson(memory) {
    if (retireBusy) return;
    const reason = retireReason.trim();
    if (!reason) {
      retireFlash = { ok: false, line: "Say why this lesson should stop guiding work." };
      return;
    }
    retireBusy = memory.active_memory_receipt_id;
    retireFlash = null;
    try {
      const packet = await client.write(
        "/learning/memory-supersedes.json",
        {
          actor_id: actor,
          activation_receipt_id: memory.active_memory_receipt_id,
          reason,
          review_owner: retireOwner.trim() || actor
        },
        { receiptIntent: "learning_memory_supersede" }
      );
      if (packet.status === "RECORDED") {
        retireFlash = {
          ok: true,
          line: "Lesson retired. It stays in the record but no longer guides new work."
        };
        closeRetire();
        await invalidateAll();
      } else {
        retireFlash = {
          ok: false,
          line: packet.reason ?? "That retirement did not record."
        };
      }
    } catch (error) {
      retireFlash = { ok: false, line: "Nothing was recorded. Try again." };
    }
    retireBusy = "";
  }

  // The open casting grant for a lesson, if a person has let it steer casting.
  function castingGrantFor(memory) {
    return castingGrants.find(
      (grant) => grant.open && grant.activation_receipt_id === memory.active_memory_receipt_id
    );
  }

  // A casting grant names one of the two scorecard target types. A lesson
  // aimed at a worker scorecard keeps that; everything else steers the model
  // scorecard.
  function castingTargetType(memory) {
    return memory.target_type === "worker_scorecard" ? "worker_scorecard" : "model_scorecard";
  }

  function openSteer(memory) {
    steerOpenFor = memory.active_memory_receipt_id;
    steerReason = "";
    steerOwner = "";
    steerSlot = "";
    steerFlash = null;
  }

  function closeSteer() {
    steerOpenFor = "";
    steerReason = "";
    steerOwner = "";
    steerSlot = "";
  }

  async function steerCasting(memory) {
    if (steerBusy) return;
    const reason = steerReason.trim();
    if (!reason) {
      steerFlash = { ok: false, line: "Say why this lesson should shape future plans." };
      return;
    }
    steerBusy = memory.active_memory_receipt_id;
    steerFlash = null;
    try {
      const packet = await client.write(
        "/learning/adaptation-grants.json",
        {
          actor_id: actor,
          activation_receipt_id: memory.active_memory_receipt_id,
          adaptation_type: "fleet_casting",
          target_type: castingTargetType(memory),
          preferred_builder_slot: steerSlot.trim(),
          reason,
          review_owner: steerOwner.trim() || actor
        },
        { receiptIntent: "learning_adaptation_grant" }
      );
      if (packet.status === "RECORDED") {
        steerFlash = {
          ok: true,
          line: "Done. Future fleet plans may prefer or avoid the runners this lesson names - nothing else."
        };
        closeSteer();
        await invalidateAll();
      } else {
        steerFlash = { ok: false, line: packet.reason ?? "That did not record." };
      }
    } catch (error) {
      steerFlash = { ok: false, line: "Nothing was recorded. Try again." };
    }
    steerBusy = "";
  }

  async function withdrawCasting(memory) {
    const grant = castingGrantFor(memory);
    if (!grant || steerBusy) return;
    steerBusy = memory.active_memory_receipt_id;
    steerFlash = null;
    try {
      const packet = await client.write(
        "/learning/adaptation-supersedes.json",
        {
          actor_id: actor,
          grant_receipt_id: grant.adaptation_grant_receipt_id,
          reason: "Withdrawn from casting on review.",
          review_owner: actor
        },
        { receiptIntent: "learning_adaptation_supersede" }
      );
      if (packet.status === "RECORDED") {
        steerFlash = {
          ok: true,
          line: "Withdrawn. Plans stop preferring the runners this lesson named, starting with the next plan."
        };
        await invalidateAll();
      } else {
        steerFlash = { ok: false, line: packet.reason ?? "That did not record." };
      }
    } catch (error) {
      steerFlash = { ok: false, line: "Nothing was recorded. Try again." };
    }
    steerBusy = "";
  }

  function hasMemoryCandidate(item) {
    return memoryCandidates.some(
      (candidate) => candidate.judgment_decision_id === item.judgment_decision_id
    );
  }

  function hasActiveMemory(candidate) {
    return activeMemories.some(
      (memory) => memory.memory_promotion_id === candidate.memory_promotion_id
    );
  }

  function evidenceRefsFor(record) {
    const refs = (record.missing_evidence ?? []).filter((item) => item !== "human judgment");
    return refs.length > 0 ? refs.join("; ") : "judgment card reviewed; make learning-loop-check";
  }

  function rationaleFor(decision, record) {
    if (decision === "promote_candidate") {
      return `Queue ${record.title} for memory review; judgment receipt is now recorded.`;
    }
    if (decision === "reject_candidate") {
      return `Reject ${record.title} as learning memory for now.`;
    }
    return `Supersede ${record.title}; a newer lesson should replace this candidate.`;
  }
</script>

<svelte:head><title>Learn - MDx</title></svelte:head>

<section class="learn" data-route-state="ready">
  <header class="learn-head mdx-page-head">
    <div>
      <h1>Learn</h1>
      <p class="mdx-page-sub">
        What MDx is learning from real work, what is still only a candidate, and what waits for review.
      </p>
    </div>
    <a class="primary-action" href="/forge">Review Forge path</a>
  </header>

  <section class="summary" aria-label="Learning summary">
    <div>
      <strong>{ledger.summary.signals}</strong>
      <span>signals</span>
    </div>
    <div>
      <strong>{candidateLessons.length}</strong>
      <span>drafted lessons</span>
    </div>
    <div>
      <strong>{forgeOutcomes.length}</strong>
      <span>Forge outcomes</span>
    </div>
    <div>
      <strong>{lessonPromotions.length}</strong>
      <span>promotion items</span>
    </div>
    <div>
      <strong>{judgmentRecords.length}</strong>
      <span>judgment records</span>
    </div>
    <div>
      <strong>{memoryTargets.length}</strong>
      <span>memory targets</span>
    </div>
    <div>
      <strong>{adaptationProposals.length}</strong>
      <span>adaptation proposals</span>
    </div>
    <div>
      <strong>{evidenceRequests.length}</strong>
      <span>evidence requests</span>
    </div>
    <div>
      <strong>{judgmentDecisions.length}</strong>
      <span>judgment decisions</span>
    </div>
    <div>
      <strong>{memoryQueue.length}</strong>
      <span>ready for memory</span>
    </div>
    <div>
      <strong>{memoryCandidates.length}</strong>
      <span>memory candidates</span>
    </div>
    <div>
      <strong>{activeMemories.length}</strong>
      <span>active memories</span>
    </div>
  </section>

  <section class="primary-path" aria-label="First learning path">
    <div>
      <span class="eyebrow">First path</span>
      <h2>Forge outcome to next casting</h2>
      <p>
        A finished Forge run should produce an outcome record, get evaluated, earn a human or policy
        judgment, and only then shape the next worker mix, model preference, budget, or review depth.
      </p>
    </div>
    <ol>
      {#each firstPath.steps as step}
        <li>{step}</li>
      {/each}
    </ol>
  </section>

  <section class="ledger-section" aria-label="Lessons waiting on you">
    <div class="section-head">
      <h2>Lessons waiting on you</h2>
      <p>
        Drafted from real work: finished builds, feedback people sent, and comparisons an outside
        runner won. Read one, edit it in your own words, then keep it or reject it. Nothing moves
        without you.
      </p>
    </div>
    {#if candidateLessons.length > 0}
      <ul class="candidate-list">
        {#each candidateLessons as candidate (candidate.candidate_id)}
          <li class="candidate">
            <div class="item-head">
              <div>
                <span class="surface">
                  {candidate.source === "beta_feedback"
                    ? "From feedback"
                    : candidate.source === "fleet_distillation"
                      ? "From a fleet comparison"
                      : "From a Forge run"}
                </span>
                <h3>{candidate.title || candidate.lesson_summary}</h3>
              </div>
              <span class="state blocked">Waiting on you</span>
            </div>
            {#if candidate.draft_flagged}
              <p class="draft-warning">
                Look closer before keeping this one: {candidate.draft_flag_reason}. Machine text
                never becomes memory on its own; your edit and confirmation are the gate.
              </p>
            {/if}
            <div class="draft-form">
              <label>
                The lesson, in your words
                <textarea
                  rows="3"
                  value={draftFieldsFor(candidate).lesson_summary}
                  oninput={(event) => editDraft(candidate, "lesson_summary", event.currentTarget.value)}
                ></textarea>
              </label>
              <div class="draft-form-row">
                <label>
                  Where it lands
                  <select
                    value={draftFieldsFor(candidate).target_type}
                    onchange={(event) => editDraft(candidate, "target_type", event.currentTarget.value)}
                  >
                    {#each memoryTargetTypes as targetType}
                      <option value={targetType}>{targetType.replaceAll("_", " ")}</option>
                    {/each}
                  </select>
                </label>
                <label>
                  Evidence
                  <input
                    type="text"
                    value={draftFieldsFor(candidate).evidence_refs}
                    oninput={(event) => editDraft(candidate, "evidence_refs", event.currentTarget.value)}
                  />
                </label>
              </div>
              <div class="draft-form-row">
                <label>
                  Applies to work sizes
                  <input
                    type="text"
                    placeholder="small, medium, large - empty means all work"
                    value={draftFieldsFor(candidate).applicability_work_tiers}
                    oninput={(event) =>
                      editDraft(candidate, "applicability_work_tiers", event.currentTarget.value)}
                  />
                </label>
                <label>
                  Applies to stacks
                  <input
                    type="text"
                    placeholder="rust-cargo, node - empty means all stacks"
                    value={draftFieldsFor(candidate).applicability_language_packs}
                    oninput={(event) =>
                      editDraft(candidate, "applicability_language_packs", event.currentTarget.value)}
                  />
                </label>
              </div>
            </div>
            {#if rejectOpenFor === candidate.candidate_id}
              <div class="draft-form">
                <label>
                  Why is it not worth keeping?
                  <textarea
                    rows="2"
                    bind:value={rejectReason}
                    placeholder="What is wrong, overfit, or already covered about it"
                  ></textarea>
                </label>
                <div class="action-row">
                  <button
                    type="button"
                    class="primary-decision"
                    onclick={() => rejectDraft(candidate)}
                    disabled={Boolean(rejectBusy || saveBusy)}
                  >
                    {rejectBusy === candidate.candidate_id ? "Recording the rejection" : "Reject it"}
                  </button>
                  <button type="button" onclick={closeReject} disabled={Boolean(rejectBusy)}>
                    Keep looking
                  </button>
                </div>
              </div>
            {:else}
              <div class="action-row">
                <button
                  type="button"
                  class="primary-decision"
                  onclick={() => keepDraft(candidate)}
                  disabled={Boolean(saveBusy || rejectBusy)}
                >
                  {saveBusy === candidate.candidate_id ? saveStep || "Recording" : "Keep this lesson"}
                </button>
                <button
                  type="button"
                  onclick={() => openReject(candidate)}
                  disabled={Boolean(saveBusy || rejectBusy)}
                >
                  Reject
                </button>
              </div>
            {/if}
            <p class="candidate-next">
              Keeping it records your judgment, the memory candidate, and the active memory in
              order, each with its own receipt. It still cannot change behavior, grant permission,
              or run on its own.
            </p>
            <details>
              <summary>view the evidence</summary>
              <ul class="source-list">
                {#if candidate.run_id}<li>Run: <code>{candidate.run_id}</code></li>{/if}
                {#if candidate.disposition}<li>Outcome: {candidate.disposition}</li>{/if}
                {#if candidate.surface}<li>Surface: {candidate.surface}</li>{/if}
                {#if candidate.category}<li>Kind: {candidate.category}</li>{/if}
                {#if candidate.winning_runner_id}<li>Winner: {candidate.winning_runner_id}</li>{/if}
                {#if candidate.distillation_reason}<li>Reason: {candidate.distillation_reason}</li>{/if}
                {#if candidate.lesson_source}<li>Drafted: {candidate.lesson_source === "model_distilled" ? "with a model pass over the run evidence" : "directly from the run evidence"}</li>{/if}
                {#if candidate.evidence_refs}<li>{candidate.evidence_refs}</li>{/if}
                {#if candidate.source_receipt_id}<li><code>{candidate.source_receipt_id}</code></li>{/if}
              </ul>
            </details>
          </li>
        {/each}
      </ul>
    {:else}
      <div class="empty-outcomes">
        <span class="eyebrow">Nothing waiting</span>
        <h3>No drafted lessons right now</h3>
        <p>
          When a build finishes, feedback lands, or an outside runner wins a comparison, the
          lesson it suggests appears here for you to edit and decide on.
        </p>
      </div>
    {/if}
    {#if saveFlash}
      <p class:ok={saveFlash.ok} class:error={!saveFlash.ok} class="action-flash">
        {saveFlash.line}
      </p>
    {/if}
    {#if savedReceipts}
      <details class="quiet-more" open>
        <summary>view the receipts</summary>
        <ul class="source-list">
          <li>Judgment: <code>{savedReceipts.judgment}</code></li>
          <li>Memory candidate: <code>{savedReceipts.promotion}</code></li>
          <li>Active memory: <code>{savedReceipts.activation}</code></li>
        </ul>
      </details>
    {/if}
  </section>

  {#if candidateRejections.length > 0}
    <section class="ledger-section" aria-label="Set aside">
      <div class="section-head">
        <h2>Set aside</h2>
        <p>Drafts someone decided not to keep. Each carries its receipt and will not be suggested again.</p>
      </div>
      <ul class="candidate-list">
        {#each candidateRejections as rejection (rejection.candidate_rejection_receipt_id)}
          <li>
            <div class="item-head">
              <div>
                <span class="surface">{rejection.review_owner}</span>
                <h3>{rejection.reason}</h3>
              </div>
              <span class="state">Not kept</span>
            </div>
            <details>
              <summary>view the receipt</summary>
              <ul class="source-list">
                <li><code>{rejection.candidate_rejection_receipt_id}</code></li>
                <li><code>{rejection.source_receipt_id}</code></li>
                <li><code>{rejection.candidate_id}</code></li>
              </ul>
            </details>
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  <section class="ledger-section" aria-label="Forge outcomes">
    <div class="section-head">
      <h2>Forge outcomes</h2>
      <p>Runs Learn can see today, folded into outcome records before anything changes.</p>
    </div>
    {#if forgeOutcomes.length > 0}
      <ul class="outcome-list">
        {#each forgeOutcomes as outcome}
          <li>
            <div class="item-head">
              <div>
                <span class="surface">Forge</span>
                <h3>{outcome.status_label}</h3>
              </div>
              <span class="state" class:blocked={outcome.promotion_status === "not_promotable"}>
                {outcome.promotion_status}
              </span>
            </div>
            <dl class="outcome-facts">
              <div>
                <dt>Intent</dt>
                <dd>{outcome.intent_summary}</dd>
              </div>
              <div>
                <dt>Execution</dt>
                <dd>{outcome.execution_summary}</dd>
              </div>
              <div>
                <dt>Evaluation</dt>
                <dd>{outcome.evaluation_summary}</dd>
              </div>
              <div>
                <dt>Judgment</dt>
                <dd>{outcome.human_judgment_status}</dd>
              </div>
            </dl>
            <p>{outcome.lesson_candidate}</p>
            <details>
              <summary>what is missing</summary>
              <ul class="source-list">
                {#each outcome.missing_evidence as missing}
                  <li>{missing}</li>
                {/each}
              </ul>
            </details>
            <details>
              <summary>view the record</summary>
              <ul class="source-list">
                <li><code>{outcome.evidence.route}</code></li>
                <li><code>{outcome.evidence.receipt_kind}</code></li>
                <li>{outcome.evidence.events_observed} observed events</li>
                {#if outcome.evidence.model}<li>{outcome.evidence.model}</li>{/if}
                {#if outcome.evidence.branch}<li>{outcome.evidence.branch}</li>{/if}
              </ul>
            </details>
          </li>
        {/each}
      </ul>
    {:else}
      <div class="empty-outcomes">
        <span class="eyebrow">Waiting</span>
        <h3>No Forge outcomes yet</h3>
        <p>
          The outcome contract is ready. The first completed Forge run will appear here with the
          evidence it has and the gaps that still block promotion.
        </p>
      </div>
    {/if}
  </section>

  <section class="ledger-section" aria-label="Lesson promotion lane">
    <div class="section-head">
      <h2>Promotion lane</h2>
      <p>Candidate lessons waiting for evidence and judgment before they can become MDx memory.</p>
    </div>
    <ul class="promotion-list">
      {#each lessonPromotions as promotion}
        <li>
          <div class="item-head">
            <div>
              <span class="surface">{promotion.surface}</span>
              <h3>{promotion.title}</h3>
            </div>
            <span class="state" class:blocked={promotion.state === "needs_evidence"}>
              {promotion.state}
            </span>
          </div>
          <p>{promotion.proposed_memory}</p>
          <dl class="promotion-steps">
            <div>
              <dt>Judgment</dt>
              <dd>{promotion.judgment_status}</dd>
            </div>
            <div>
              <dt>Next</dt>
              <dd>{promotion.allowed_next_step}</dd>
            </div>
          </dl>
          <details>
            <summary>what is missing</summary>
            <ul class="source-list">
              {#each promotion.missing_evidence as missing}
                <li>{missing}</li>
              {/each}
            </ul>
          </details>
          <details>
            <summary>what stays blocked</summary>
            <ul class="source-list">
              {#each promotion.blocked_changes as change}
                <li>{change}</li>
              {/each}
            </ul>
          </details>
        </li>
      {/each}
    </ul>
  </section>

  <section class="ledger-section" aria-label="Judgment records">
    <div class="section-head">
      <h2>Judgment records</h2>
      <p>The decision shape for each promotion item, visible before any approval action exists.</p>
    </div>
    <ul class="judgment-list">
      {#each judgmentRecords as record}
        <li>
          <div class="item-head">
            <div>
              <span class="surface">{record.surface}</span>
              <h3>{record.title}</h3>
            </div>
            <span class="state" class:blocked={record.state === "not_ready"}>{record.state}</span>
          </div>
          <dl class="judgment-facts">
            <div>
              <dt>Actor</dt>
              <dd>{record.accountable_actor_required ? "required" : "not required"}</dd>
            </div>
            <div>
              <dt>Rationale</dt>
              <dd>{record.rationale_required ? "required" : "not required"}</dd>
            </div>
            <div>
              <dt>Receipt</dt>
              <dd>{record.receipt_required ? "required" : "not required"}</dd>
            </div>
            <div>
              <dt>Adaptation</dt>
              <dd>{record.adaptation_scope}</dd>
            </div>
          </dl>
          <p>{record.safe_next}</p>
          <div class="action-row">
            <button
              type="button"
              onclick={() => askForMoreEvidence(record)}
              disabled={Boolean(evidenceBusy || judgmentBusy)}
            >
              {evidenceBusy === record.judgment_id ? "Recording evidence request" : "Ask for more evidence"}
            </button>
            <button
              type="button"
              onclick={() => recordJudgmentDecision(record, "reject_candidate")}
              disabled={Boolean(evidenceBusy || judgmentBusy)}
            >
              {judgmentBusy === `${record.judgment_id}:reject_candidate` ? "Recording rejection" : "Reject"}
            </button>
            <button
              type="button"
              onclick={() => recordJudgmentDecision(record, "supersede_candidate")}
              disabled={Boolean(evidenceBusy || judgmentBusy)}
            >
              {judgmentBusy === `${record.judgment_id}:supersede_candidate` ? "Recording supersession" : "Supersede"}
            </button>
            <button
              type="button"
              class="primary-decision"
              onclick={() => recordJudgmentDecision(record, "promote_candidate")}
              disabled={Boolean(evidenceBusy || judgmentBusy || record.state !== "ready_for_judgment")}
            >
              {judgmentBusy === `${record.judgment_id}:promote_candidate` ? "Recording judgment" : "Mark ready for memory"}
            </button>
          </div>
          <details>
            <summary>decision options</summary>
            <ul class="source-list">
              {#each record.decision_options as option}
                <li>{option}</li>
              {/each}
            </ul>
          </details>
          <details>
            <summary>what is missing</summary>
            <ul class="source-list">
              {#each record.missing_evidence as missing}
                <li>{missing}</li>
              {/each}
            </ul>
          </details>
          <details>
            <summary>what stays blocked</summary>
            <ul class="source-list">
              {#each record.blocked_changes as change}
                <li>{change}</li>
              {/each}
            </ul>
          </details>
        </li>
      {/each}
    </ul>
    {#if evidenceFlash}
      <p class:ok={evidenceFlash.ok} class:error={!evidenceFlash.ok} class="action-flash">
        {evidenceFlash.line}
      </p>
    {/if}
    {#if judgmentFlash}
      <p class:ok={judgmentFlash.ok} class:error={!judgmentFlash.ok} class="action-flash">
        {judgmentFlash.line}
      </p>
    {/if}
  </section>

  <section class="ledger-section" aria-label="Evidence requests">
    <div class="section-head">
      <h2>Evidence requests</h2>
      <p>Receipt-backed asks that keep judgment honest without promoting memory or applying adaptation.</p>
    </div>
    {#if evidenceRequests.length > 0}
      <ul class="evidence-request-list">
        {#each evidenceRequests as request}
          <li>
            <div class="item-head">
              <div>
                <span class="surface">{request.actor_id}</span>
                <h3>{request.reason}</h3>
              </div>
              <span class="state blocked">evidence requested</span>
            </div>
            <p>{request.requested_evidence}</p>
            <details>
              <summary>view the receipt</summary>
              <ul class="source-list">
                <li><code>{request.evidence_request_receipt_id}</code></li>
                <li><code>{request.judgment_id}</code></li>
                <li><code>{request.promotion_id}</code></li>
              </ul>
            </details>
          </li>
        {/each}
      </ul>
    {:else}
      <div class="empty-outcomes">
        <span class="eyebrow">No asks yet</span>
        <h3>No evidence requests recorded</h3>
        <p>Use a judgment card when the missing proof should be captured as a receipt.</p>
      </div>
    {/if}
  </section>

  <section class="ledger-section" aria-label="Judgment decisions">
    <div class="section-head">
      <h2>Judgment decisions</h2>
      <p>Receipt-backed decisions that can queue memory review but still cannot write memory.</p>
    </div>
    {#if judgmentDecisions.length > 0}
      <ul class="judgment-decision-list">
        {#each judgmentDecisions as decision}
          <li>
            <div class="item-head">
              <div>
                <span class="surface">{decision.surface}</span>
                <h3>{decision.title}</h3>
              </div>
              <span class="state" class:blocked={decision.memory_queue_state !== "ready_for_memory"}>
                {decision.decision_label}
              </span>
            </div>
            <p>{decision.rationale}</p>
            <dl class="decision-facts">
              <div>
                <dt>Memory</dt>
                <dd>{decision.memory_queue_label}</dd>
              </div>
              <div>
                <dt>Evidence</dt>
                <dd>{decision.evidence_refs}</dd>
              </div>
            </dl>
            <details>
              <summary>view the receipt</summary>
              <ul class="source-list">
                <li><code>{decision.judgment_decision_receipt_id}</code></li>
                <li><code>{decision.judgment_id}</code></li>
                <li><code>{decision.promotion_id}</code></li>
              </ul>
            </details>
          </li>
        {/each}
      </ul>
    {:else}
      <div class="empty-outcomes">
        <span class="eyebrow">No decisions yet</span>
        <h3>No judgment decisions recorded</h3>
        <p>Use a judgment card to reject, supersede, or queue a candidate for memory review.</p>
      </div>
    {/if}
  </section>

  <section class="ledger-section" aria-label="Ready for memory">
    <div class="section-head">
      <h2>Ready for memory</h2>
      <p>Lessons with a promote judgment. This queue still cannot write the source of truth.</p>
    </div>
    {#if memoryQueue.length > 0}
      <ul class="memory-queue-list">
        {#each memoryQueue as item}
          <li>
            <div class="item-head">
              <div>
                <span class="surface">{item.surface}</span>
                <h3>{item.title}</h3>
              </div>
              <span class="state blocked">write blocked</span>
            </div>
            <dl class="decision-facts">
              <div>
                <dt>Target</dt>
                <dd>{item.target_type}</dd>
              </div>
              <div>
                <dt>Path</dt>
                <dd>{item.target_path}</dd>
              </div>
            </dl>
            <p>{item.safe_next}</p>
            <div class="action-row">
              <button
                type="button"
                class="primary-decision"
                onclick={() => requestMemoryPromotion(item)}
                disabled={Boolean(memoryBusy || hasMemoryCandidate(item))}
              >
                {memoryBusy === item.judgment_decision_id
                  ? "Recording memory candidate"
                  : hasMemoryCandidate(item)
                    ? "Memory candidate recorded"
                    : "Request memory promotion"}
              </button>
            </div>
            <details>
              <summary>what stays blocked</summary>
              <ul class="source-list">
                <li>memory write</li>
                <li>generated artifact refresh</li>
                <li>adaptation</li>
              </ul>
            </details>
          </li>
        {/each}
      </ul>
    {:else}
      <div class="empty-outcomes">
        <span class="eyebrow">Blocked</span>
        <h3>No lessons queued for memory</h3>
        <p>A promote judgment can queue a lesson here, but the memory write is a later route.</p>
      </div>
    {/if}
    {#if memoryFlash}
      <p class:ok={memoryFlash.ok} class:error={!memoryFlash.ok} class="action-flash">
        {memoryFlash.line}
      </p>
    {/if}
  </section>

  <section class="ledger-section" aria-label="Memory candidates">
    <div class="section-head">
      <h2>Memory candidates</h2>
      <p>Receipt-backed promotion requests. These are still not active memory.</p>
    </div>
    {#if memoryCandidates.length > 0}
      <ul class="memory-candidate-list">
        {#each memoryCandidates as candidate}
          <li>
            <div class="item-head">
              <div>
                <span class="surface">{candidate.surface}</span>
                <h3>{candidate.title}</h3>
              </div>
              <span class="state blocked">{candidate.state_label}</span>
            </div>
            <p>{candidate.lesson_summary}</p>
            <dl class="decision-facts">
              <div>
                <dt>Target</dt>
                <dd>{candidate.target_label}</dd>
              </div>
              <div>
                <dt>Path</dt>
                <dd>{candidate.target_path}</dd>
              </div>
            </dl>
            <p>{candidate.safe_next}</p>
            <div class="action-row">
              <button
                type="button"
                class="primary-decision"
                onclick={() => activateMemory(candidate)}
                disabled={Boolean(activationBusy || hasActiveMemory(candidate))}
              >
                {activationBusy === candidate.memory_promotion_id
                  ? "Recording active memory"
                  : hasActiveMemory(candidate)
                    ? "Active memory recorded"
                    : "Activate memory"}
              </button>
            </div>
            <details>
              <summary>view the receipt</summary>
              <ul class="source-list">
                <li><code>{candidate.memory_promotion_receipt_id}</code></li>
                <li><code>{candidate.judgment_decision_receipt_id}</code></li>
                <li>{candidate.review_cadence}</li>
                <li>{candidate.expiry_rule}</li>
              </ul>
            </details>
            <details>
              <summary>what stays blocked</summary>
              <ul class="source-list">
                {#each candidate.blocked_changes as change}
                  <li>{change}</li>
                {/each}
              </ul>
            </details>
          </li>
        {/each}
      </ul>
    {:else}
      <div class="empty-outcomes">
        <span class="eyebrow">No candidates yet</span>
        <h3>No memory promotion requests recorded</h3>
        <p>Use a ready-for-memory item to create a candidate without activating memory.</p>
      </div>
    {/if}
    {#if activationFlash}
      <p class:ok={activationFlash.ok} class:error={!activationFlash.ok} class="action-flash">
        {activationFlash.line}
      </p>
    {/if}
  </section>

  <section class="ledger-section" aria-label="Shared memory waiting on review">
    <div class="section-head">
      <h2>Shared memory waiting on review</h2>
      <p>
        What recent messages, pages, and build requests proposed to remember for the whole team.
        Nothing here informs recall until someone other than the proposer approves it.
      </p>
    </div>
    {#if pendingConsolidations.length > 0}
      <ul class="active-memory-list">
        {#each pendingConsolidations as memory (memory.memory_id)}
          <li>
            <div class="item-head">
              <div>
                <span class="surface">{scopeLabel(memory.memory_scope)}</span>
                <h3>{memory.content}</h3>
              </div>
              <span class="state">Waiting on review</span>
            </div>
            <p class="quiet">Proposed by {memory.proposed_by || "an unknown actor"}.</p>
            {#if !canReview(memory)}
              <p class="quiet">A different person must review this. Your own proposal cannot approve itself.</p>
            {/if}
            <div class="action-row">
              <button
                type="button"
                class="primary-decision"
                onclick={() => ratifyConsolidation(memory, "approve")}
                disabled={Boolean(ratifyBusy) || !canReview(memory)}
              >
                {ratifyBusy === `${memory.memory_id}:approve` ? "Approving" : "Approve"}
              </button>
              <button
                type="button"
                onclick={() => ratifyConsolidation(memory, "decline")}
                disabled={Boolean(ratifyBusy) || !canReview(memory)}
              >
                {ratifyBusy === `${memory.memory_id}:decline` ? "Declining" : "Decline"}
              </button>
            </div>
            <details>
              <summary>view the receipt</summary>
              <ul class="source-list">
                <li><code>{memory.gate_receipt_id}</code></li>
                <li><code>{memory.source_receipt_id}</code></li>
              </ul>
            </details>
          </li>
        {/each}
      </ul>
    {:else}
      <div class="empty-outcomes">
        <span class="eyebrow">Nothing waiting</span>
        <h3>No shared memory needs review</h3>
        <p>
          When a message, page, or build request proposes a team-visible memory, it appears here
          for a second person to approve before anything recalls it.
        </p>
      </div>
    {/if}
    {#if ratifyFlash}
      <p class:ok={ratifyFlash.ok} class:error={!ratifyFlash.ok} class="action-flash">
        {ratifyFlash.line}
      </p>
    {/if}
  </section>

  <section class="ledger-section" aria-label="Active memory">
    <div class="section-head">
      <h2>Active memory</h2>
      <p>Receipt-backed MDx memory that can inform later proposals but still cannot change behavior.</p>
    </div>
    {#if activeMemories.length > 0}
      <ul class="active-memory-list">
        {#each activeMemories as memory}
          <li>
            <div class="item-head">
              <div>
                <span class="surface">{memory.surface}</span>
                <h3>{memory.title}</h3>
              </div>
              <span class="state">{memory.state_label}</span>
            </div>
            <p>{memory.lesson_summary}</p>
            <dl class="decision-facts">
              <div>
                <dt>Target</dt>
                <dd>{memory.target_label}</dd>
              </div>
              <div>
                <dt>Path</dt>
                <dd>{memory.target_path}</dd>
              </div>
              <div>
                <dt>Owner</dt>
                <dd>{memory.review_owner}</dd>
              </div>
            </dl>
            <p>{memory.safe_next}</p>
            {#if memory.active_memory_state === "active"}
              {#if castingGrantFor(memory)}
                <div class="steer-active">
                  <p class="steer-note">
                    This lesson steers casting. Future fleet plans may prefer or avoid the runners
                    it names - nothing else. Model routing, budgets, checks, and review depth stay
                    the same.
                  </p>
                  <div class="action-row">
                    <button
                      type="button"
                      onclick={() => withdrawCasting(memory)}
                      disabled={Boolean(steerBusy)}
                    >
                      {steerBusy === memory.active_memory_receipt_id ? "Withdrawing" : "Withdraw"}
                    </button>
                  </div>
                </div>
              {:else if steerOpenFor === memory.active_memory_receipt_id}
                <div class="steer-form">
                  <p class="steer-note">
                    Future fleet plans may prefer or avoid the runners this lesson names - nothing
                    else. It stays reversible, and you can withdraw it at any time.
                  </p>
                  <label>
                    Why should this lesson shape future plans?
                    <textarea
                      rows="2"
                      bind:value={steerReason}
                      placeholder="What this lesson proved about which runner to use"
                    ></textarea>
                  </label>
                  <label>
                    Which runner should plans prefer? (optional)
                    <input type="text" bind:value={steerSlot} placeholder="e.g. OPUS, or leave blank" />
                  </label>
                  <label>
                    Who reviewed this call?
                    <input type="text" bind:value={steerOwner} />
                  </label>
                  <div class="action-row">
                    <button
                      type="button"
                      class="primary-decision"
                      onclick={() => steerCasting(memory)}
                      disabled={Boolean(steerBusy)}
                    >
                      {steerBusy === memory.active_memory_receipt_id
                        ? "Recording"
                        : "Let this lesson steer casting"}
                    </button>
                    <button type="button" onclick={closeSteer} disabled={Boolean(steerBusy)}>
                      Not now
                    </button>
                  </div>
                </div>
              {:else}
                <div class="action-row">
                  <button
                    type="button"
                    onclick={() => openSteer(memory)}
                    disabled={Boolean(steerBusy)}
                  >
                    Let this lesson steer casting
                  </button>
                </div>
              {/if}
              {#if retireOpenFor === memory.active_memory_receipt_id}
                <div class="retire-form">
                  <label>
                    Why should this lesson stop guiding work?
                    <textarea
                      rows="2"
                      bind:value={retireReason}
                      placeholder="What turned out to be wrong or outdated about it"
                    ></textarea>
                  </label>
                  <label>
                    Who reviewed this call?
                    <input type="text" bind:value={retireOwner} />
                  </label>
                  <div class="action-row">
                    <button
                      type="button"
                      class="primary-decision"
                      onclick={() => retireLesson(memory)}
                      disabled={Boolean(retireBusy)}
                    >
                      {retireBusy === memory.active_memory_receipt_id
                        ? "Recording retirement"
                        : "Retire it"}
                    </button>
                    <button type="button" onclick={closeRetire} disabled={Boolean(retireBusy)}>
                      Keep the lesson
                    </button>
                  </div>
                </div>
              {:else}
                <div class="action-row">
                  <button
                    type="button"
                    onclick={() => openRetire(memory)}
                    disabled={Boolean(retireBusy)}
                  >
                    Retire this lesson
                  </button>
                </div>
              {/if}
            {/if}
            <details>
              <summary>view the receipt</summary>
              <ul class="source-list">
                <li><code>{memory.active_memory_receipt_id}</code></li>
                <li><code>{memory.memory_promotion_receipt_id}</code></li>
                <li><code>{memory.judgment_decision_receipt_id}</code></li>
              </ul>
            </details>
            <details>
              <summary>activation basis</summary>
              <ul class="source-list">
                <li>{memory.activation_basis}</li>
                <li>{memory.local_checks}</li>
                <li>{memory.rollback_plan}</li>
              </ul>
            </details>
            <details>
              <summary>what stays blocked</summary>
              <ul class="source-list">
                {#each memory.blocked_changes as change}
                  <li>{change}</li>
                {/each}
              </ul>
            </details>
          </li>
        {/each}
      </ul>
    {:else}
      <div class="empty-outcomes">
        <span class="eyebrow">No active memory yet</span>
        <h3>No memory activations recorded</h3>
        <p>Use a memory candidate to record active MDx memory without changing behavior.</p>
      </div>
    {/if}
    {#if retireFlash}
      <p class:ok={retireFlash.ok} class:error={!retireFlash.ok} class="action-flash">
        {retireFlash.line}
      </p>
    {/if}
    {#if steerFlash}
      <p class:ok={steerFlash.ok} class:error={!steerFlash.ok} class="action-flash">
        {steerFlash.line}
      </p>
    {/if}
  </section>

  <section class="ledger-section" aria-label="Memory targets">
    <div class="section-head">
      <h2>Memory targets</h2>
      <p>Where approved lessons would land as MDx-owned memory, before anything is written.</p>
    </div>
    <ul class="memory-list">
      {#each memoryTargets as target}
        <li>
          <div class="item-head">
            <div>
              <span class="surface">{target.surface}</span>
              <h3>{target.title}</h3>
            </div>
            <span class="state" class:blocked={target.state === "waiting_for_judgment"}>
              {target.state}
            </span>
          </div>
          <dl class="memory-facts">
            <div>
              <dt>Target</dt>
              <dd>{target.target_type}</dd>
            </div>
            <div>
              <dt>Update</dt>
              <dd>{target.source_of_truth_update_required ? "required" : "not required"}</dd>
            </div>
            <div>
              <dt>Checks</dt>
              <dd>{target.local_checks_required.join(", ")}</dd>
            </div>
          </dl>
          <p>{target.safe_next}</p>
          <details>
            <summary>view the target</summary>
            <ul class="source-list">
              <li><code>{target.target_path}</code></li>
              <li>{target.rollback_rule}</li>
              <li>{target.supersession_rule}</li>
            </ul>
          </details>
          <details>
            <summary>sovereignty rule</summary>
            <p class="quiet">{target.sovereignty_rule}</p>
          </details>
          <details>
            <summary>what stays blocked</summary>
            <ul class="source-list">
              {#each target.blocked_changes as change}
                <li>{change}</li>
              {/each}
            </ul>
          </details>
        </li>
      {/each}
    </ul>
  </section>

  <section class="ledger-section" aria-label="Adaptation proposals">
    <div class="section-head">
      <h2>Adaptation proposals</h2>
      <p>Behavior MDx could change later, held until memory, judgment, and an approval route exist.</p>
    </div>
    <ul class="adaptation-proposal-list">
      {#each adaptationProposals as proposal}
        <li>
          <div class="item-head">
            <div>
              <span class="surface">{proposal.consumer_surface}</span>
              <h3>{proposal.proposed_change}</h3>
            </div>
            <span class="state blocked">{proposal.execution_state}</span>
          </div>
          <dl class="proposal-facts">
            <div>
              <dt>Type</dt>
              <dd>{proposal.adaptation_label}</dd>
            </div>
            <div>
              <dt>Memory</dt>
              <dd>{proposal.required_memory_state}</dd>
            </div>
            <div>
              <dt>Judgment</dt>
              <dd>{proposal.required_judgment_state}</dd>
            </div>
            <div>
              <dt>Checks</dt>
              <dd>{proposal.required_checks.join(", ")}</dd>
            </div>
          </dl>
          <p>{proposal.blocked_reason}</p>
          <details>
            <summary>risk boundary</summary>
            <p class="quiet">{proposal.risk_boundary}</p>
          </details>
          <details>
            <summary>rollback path</summary>
            <p class="quiet">{proposal.rollback_path}</p>
          </details>
        </li>
      {/each}
    </ul>
  </section>

  <section class="ledger-section" aria-label="Learning signals">
    <div class="section-head">
      <h2>Learning signals</h2>
      <p>The strongest things MDx can already see, with their current state made plain.</p>
    </div>
    <ul class="signal-list">
      {#each ledger.learning_signals as signal}
        <li>
          <div class="item-head">
            <div>
              <span class="surface">{signal.surface}</span>
              <h3>{signal.title}</h3>
            </div>
            <span class="state">{signal.status}</span>
          </div>
          <p>{signal.line}</p>
          <details>
            <summary>view the sources</summary>
            <ul class="source-list">
              {#each signal.evidence as source}
                <li><code>{source}</code></li>
              {/each}
            </ul>
          </details>
        </li>
      {/each}
    </ul>
  </section>

  <section class="ledger-grid" aria-label="Seed record">
    <div class="lane">
      <div class="section-head">
        <h2>Candidate lessons from the seed record</h2>
        <p>The lessons this surface shipped with, kept for history. New lessons start at the top of the page, from live work.</p>
      </div>
      <ul>
        {#each ledger.candidate_lessons as lesson}
          <li>
            <span class="surface">{lesson.surface}</span>
            <h3>{lesson.title}</h3>
            <p>{lesson.lesson}</p>
            <span class="quiet">{lesson.next_step}</span>
          </li>
        {/each}
      </ul>
    </div>

    <div class="lane">
      <div class="section-head">
        <h2>Promoted memory from the seed record</h2>
        <p>The one promoted lesson this surface shipped with, kept for history alongside its contract.</p>
      </div>
      <ul>
        {#each ledger.promoted_memory as memory}
          <li>
            <span class="surface">{memory.surface}</span>
            <h3>{memory.title}</h3>
            <p>{memory.line}</p>
            <details>
              <summary>view the sources</summary>
              <ul class="source-list">
                {#each memory.evidence as source}
                  <li><code>{source}</code></li>
                {/each}
              </ul>
            </details>
          </li>
        {/each}
      </ul>
    </div>
  </section>

  <section class="ledger-section" aria-label="Adaptation queue">
    <div class="section-head">
      <h2>Blocked changes</h2>
      <p>Behavior MDx could improve later, held until the evidence and approval path exist.</p>
    </div>
    <ul class="adapt-list">
      {#each ledger.adaptation_queue as item}
        <li>
          <div class="item-head">
            <div>
              <span class="surface">{item.surface}</span>
              <h3>{item.title}</h3>
            </div>
            <span class="state blocked">{item.state}</span>
          </div>
          <p>{item.safe_next}</p>
          <details>
            <summary>what stays held</summary>
            <ul class="source-list">
              {#each item.blocked_authority as boundary}
                <li>{boundary}</li>
              {/each}
            </ul>
          </details>
        </li>
      {/each}
    </ul>
  </section>

  <section class="track-record" aria-label="Model and worker track record">
    <span class="eyebrow">Model and worker track record</span>
    <h2>Waiting for governed traces</h2>
    <p>{ledger.model_worker_track_record.line}</p>
  </section>

  <footer class="quiet-line">
    Boundary: {data.boundary} Safe next: {data.safeNext} Blocked changes: {blockedCount}.
    <details class="quiet-more">
      <summary>view the record</summary>
      <span>
        Source: {ledger.source}. Check: {ledger.checks.join(", ")}.
      </span>
    </details>
  </footer>
</section>

<style>
  .learn {
    display: grid;
    gap: 22px;
    align-content: start;
    max-width: 1180px;
    margin: 0 auto;
    padding: 4vh 0 44px;
  }

  .learn-head {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 18px;
  }

  .learn-head > div {
    display: grid;
    gap: 5px;
    min-width: 0;
  }

  .primary-action {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 40px;
    padding: 0 14px;
    border: 1px solid rgba(127, 211, 148, 0.45);
    border-radius: var(--mdx-radius-md);
    background: rgba(127, 211, 148, 0.14);
    color: var(--mdx-text-primary);
    font-size: 13px;
    font-weight: 650;
    text-decoration: none;
    white-space: nowrap;
  }

  .summary {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(132px, 1fr));
    gap: 8px;
  }

  .summary div,
  .primary-path,
  .signal-list > li,
  .outcome-list > li,
  .promotion-list > li,
  .judgment-list > li,
  .memory-list > li,
  .adaptation-proposal-list > li,
  .evidence-request-list > li,
  .judgment-decision-list > li,
  .memory-queue-list > li,
  .memory-candidate-list > li,
  .active-memory-list > li,
  .lane,
  .adapt-list > li,
  .track-record,
  .empty-outcomes {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
  }

  .summary div {
    display: grid;
    gap: 2px;
    padding: 14px;
  }

  .summary strong {
    font-family: var(--mdx-font-display);
    font-size: 24px;
    line-height: 1;
  }

  .summary span {
    color: var(--mdx-text-muted);
    font-size: 12px;
  }

  .primary-path {
    display: grid;
    grid-template-columns: minmax(0, 0.9fr) minmax(280px, 1.1fr);
    gap: 22px;
    padding: 20px;
  }

  .primary-path h2,
  .section-head h2,
  .track-record h2 {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: 18px;
    font-weight: 700;
    letter-spacing: 0;
  }

  .primary-path p,
  .section-head p,
  .track-record p,
  .signal-list p,
  .lane p,
  .adapt-list p {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 13px;
    line-height: 1.55;
  }

  .primary-path ol {
    display: grid;
    gap: 7px;
    margin: 0;
    padding-left: 20px;
    color: var(--mdx-text-secondary);
    font-size: 13px;
    line-height: 1.5;
  }

  .eyebrow,
  .surface {
    color: var(--mdx-text-tertiary);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0;
    text-transform: uppercase;
  }

  .ledger-section,
  .lane {
    display: grid;
    gap: 12px;
  }

  .section-head {
    display: grid;
    gap: 4px;
  }

  .signal-list,
  .outcome-list,
  .candidate-list,
  .promotion-list,
  .judgment-list,
  .memory-list,
  .adaptation-proposal-list,
  .evidence-request-list,
  .judgment-decision-list,
  .memory-queue-list,
  .memory-candidate-list,
  .active-memory-list,
  .adapt-list,
  .lane ul {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .signal-list > li,
  .candidate-list > li,
  .outcome-list > li,
  .promotion-list > li,
  .judgment-list > li,
  .memory-list > li,
  .adaptation-proposal-list > li,
  .evidence-request-list > li,
  .judgment-decision-list > li,
  .memory-queue-list > li,
  .memory-candidate-list > li,
  .active-memory-list > li,
  .adapt-list > li {
    display: grid;
    gap: 10px;
    padding: 16px;
  }

  .outcome-facts {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
    margin: 0;
  }

  .promotion-steps {
    display: grid;
    grid-template-columns: minmax(0, 0.8fr) minmax(0, 1.2fr);
    gap: 8px;
    margin: 0;
  }

  .judgment-facts {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
    margin: 0;
  }

  .memory-facts {
    display: grid;
    grid-template-columns: minmax(0, 0.8fr) minmax(0, 0.7fr) minmax(0, 1.5fr);
    gap: 8px;
    margin: 0;
  }

  .proposal-facts {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
    margin: 0;
  }

  .decision-facts {
    display: grid;
    grid-template-columns: minmax(0, 0.7fr) minmax(0, 1.3fr);
    gap: 8px;
    margin: 0;
  }

  .memory-facts div {
    display: grid;
    gap: 3px;
    padding: 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
  }

  .memory-facts dt {
    color: var(--mdx-text-tertiary);
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
  }

  .memory-facts dd {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    line-height: 1.4;
    overflow-wrap: anywhere;
  }

  .proposal-facts div {
    display: grid;
    gap: 3px;
    padding: 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
  }

  .proposal-facts dt {
    color: var(--mdx-text-tertiary);
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
  }

  .proposal-facts dd {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    line-height: 1.4;
    overflow-wrap: anywhere;
  }

  .decision-facts div {
    display: grid;
    gap: 3px;
    padding: 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
  }

  .decision-facts dt {
    color: var(--mdx-text-tertiary);
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
  }

  .decision-facts dd {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    line-height: 1.4;
    overflow-wrap: anywhere;
  }

  .judgment-facts div {
    display: grid;
    gap: 3px;
    padding: 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
  }

  .judgment-facts dt {
    color: var(--mdx-text-tertiary);
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
  }

  .judgment-facts dd {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    line-height: 1.4;
  }

  .promotion-steps div {
    display: grid;
    gap: 3px;
    padding: 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
  }

  .promotion-steps dt {
    color: var(--mdx-text-tertiary);
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
  }

  .promotion-steps dd {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    line-height: 1.4;
  }

  .outcome-facts div {
    display: grid;
    gap: 3px;
    padding: 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
  }

  .outcome-facts dt {
    color: var(--mdx-text-tertiary);
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
  }

  .outcome-facts dd {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    line-height: 1.4;
  }

  .item-head {
    display: flex;
    align-items: start;
    justify-content: space-between;
    gap: 16px;
  }

  h3 {
    margin: 2px 0 0;
    font-family: var(--mdx-font-display);
    font-size: 15px;
    font-weight: 700;
    letter-spacing: 0;
  }

  .state {
    flex: none;
    padding: 4px 8px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: 999px;
    color: var(--mdx-text-secondary);
    font-size: 11.5px;
    font-weight: 650;
    white-space: nowrap;
  }

  .state.blocked {
    border-color: rgba(244, 174, 96, 0.38);
    color: var(--mdx-accent-warning);
  }

  .action-row {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-start;
    gap: 8px;
  }

  .action-row button {
    min-height: 34px;
    padding: 0 12px;
    border: 1px solid rgba(127, 211, 148, 0.42);
    border-radius: var(--mdx-radius-md);
    background: rgba(127, 211, 148, 0.12);
    color: var(--mdx-text-primary);
    font-size: 12.5px;
    font-weight: 700;
    cursor: pointer;
  }

  .action-row button:disabled {
    cursor: progress;
    opacity: 0.62;
  }

  .action-row button.primary-decision {
    border-color: rgba(127, 211, 148, 0.58);
    background: rgba(127, 211, 148, 0.2);
  }

  .retire-form,
  .draft-form {
    display: grid;
    gap: 10px;
    padding: 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
  }

  .retire-form label,
  .draft-form label {
    display: grid;
    gap: 5px;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    font-weight: 650;
  }

  .retire-form textarea,
  .retire-form input,
  .draft-form textarea,
  .draft-form input,
  .draft-form select {
    padding: 8px 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font: inherit;
    font-weight: 450;
    resize: vertical;
  }

  .draft-form-row {
    display: grid;
    grid-template-columns: minmax(0, 0.8fr) minmax(0, 1.2fr);
    gap: 10px;
  }

  .draft-warning {
    margin: 0;
    padding: 10px 12px;
    border: 1px solid rgba(244, 174, 96, 0.42);
    border-radius: var(--mdx-radius-md);
    background: rgba(244, 174, 96, 0.08);
    color: var(--mdx-accent-warning);
    font-size: 12.5px;
    line-height: 1.5;
  }

  .action-flash {
    margin: 0;
    padding: 10px 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
  }

  .action-flash.ok {
    border-color: rgba(127, 211, 148, 0.36);
  }

  .action-flash.error {
    border-color: rgba(244, 174, 96, 0.42);
  }

  details {
    color: var(--mdx-text-muted);
    font-size: 12px;
  }

  summary {
    cursor: pointer;
  }

  .source-list {
    display: grid;
    gap: 4px;
    margin: 8px 0 0;
    padding-left: 16px;
    color: var(--mdx-text-tertiary);
  }

  code {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
  }

  .ledger-grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 12px;
  }

  .lane {
    padding: 16px;
  }

  .lane li {
    display: grid;
    gap: 7px;
    padding: 13px 0;
    border-top: 1px solid var(--mdx-border-subtle);
  }

  .lane li:first-child {
    border-top: none;
  }

  .quiet {
    color: var(--mdx-text-tertiary);
    font-size: 12px;
    line-height: 1.45;
  }

  .track-record {
    display: grid;
    gap: 7px;
    padding: 18px;
  }

  .empty-outcomes {
    display: grid;
    gap: 7px;
    padding: 18px;
  }

  .quiet-line {
    color: var(--mdx-text-muted);
    font-size: 12px;
    line-height: 1.6;
  }

  .quiet-more {
    margin-top: 6px;
  }

  @media (max-width: 920px) {
    .learn-head,
    .primary-path {
      grid-template-columns: 1fr;
    }

    .learn-head {
      align-items: start;
    }

    .summary,
    .ledger-grid,
    .outcome-facts,
    .promotion-steps,
    .judgment-facts,
    .memory-facts,
    .proposal-facts,
    .decision-facts {
      grid-template-columns: 1fr 1fr;
    }
  }

  @media (max-width: 620px) {
    .learn {
      padding: 24px 0 36px;
    }

    .summary,
    .ledger-grid,
    .outcome-facts,
    .promotion-steps,
    .judgment-facts,
    .memory-facts,
    .proposal-facts,
    .decision-facts,
    .draft-form-row {
      grid-template-columns: 1fr;
    }

    .primary-action {
      width: 100%;
    }

    .item-head {
      display: grid;
    }
  }
  .candidate-next {
    font-size: 12px;
    color: var(--mdx-text-tertiary);
  }
</style>
