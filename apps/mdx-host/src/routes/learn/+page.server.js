import adaptationProposalContract from "../../../../../generated/learning/adaptation-proposal-contract.json";
import contract from "../../../../../generated/learning/mdx-learning-loop-contract.json";
import evidenceRequestContract from "../../../../../generated/learning/evidence-request-contract.json";
import forgeOutcomeContract from "../../../../../generated/learning/forge-outcome-record-contract.json";
import judgmentDecisionContract from "../../../../../generated/learning/judgment-decision-contract.json";
import judgmentRecordContract from "../../../../../generated/learning/judgment-record-contract.json";
import ledger from "../../../../../generated/learning/mdx-learning-ledger-v0.json";
import lessonPromotionContract from "../../../../../generated/learning/lesson-promotion-lane-contract.json";
import memoryActivationContract from "../../../../../generated/learning/memory-activation-contract.json";
import memoryPromotionContract from "../../../../../generated/learning/memory-promotion-contract.json";
import memorySupersedeContract from "../../../../../generated/learning/memory-supersede-contract.json";
import memoryPromotionTargetContract from "../../../../../generated/learning/memory-promotion-target-contract.json";
import {
  deriveActiveMemories,
  deriveAdaptationProposals,
  deriveForgeOutcomes,
  deriveJudgmentDecisions,
  deriveJudgmentRecords,
  deriveLessonPromotions,
  deriveMemoryCandidates,
  deriveMemoryQueue,
  deriveMemoryTargets
} from "../../lib/learningLedger.js";

async function readJson(fetchImpl, path) {
  try {
    const response = await fetchImpl(`/api/kernel${path}`, {
      signal: AbortSignal.timeout(1500)
    });
    return response.ok ? await response.json() : null;
  } catch (error) {
    return null;
  }
}

export async function load({ fetch, parent }) {
  const [
    { session },
    runs,
    evidenceRequests,
    judgmentDecisionProjection,
    memoryPromotionProjection,
    memoryActivationProjection,
    candidateLessonProjection,
    consolidationProjection,
    rejectionProjection,
    adaptationGrantProjection
  ] = await Promise.all([
    parent(),
    readJson(fetch, "/forge/runs/projection.json"),
    readJson(fetch, "/learning/evidence-requests/projection.json"),
    readJson(fetch, "/learning/judgment-decisions/projection.json"),
    readJson(fetch, "/learning/memory-promotions/projection.json"),
    readJson(fetch, "/learning/memory-activations/projection.json"),
    // The drafted lessons the kernel folds from live receipts: Forge
    // outcomes, feedback captures, and fleet comparisons an external runner
    // won. Each waits on a human before it can guide anything.
    readJson(fetch, "/learning/forge-outcome-candidates/projection.json"),
    // Shared-scope memory that landed pending: stored, but recalled by
    // nothing until someone other than the proposer approves it.
    readJson(fetch, "/memory/consolidation-ratifications/projection.json"),
    // Candidates a person set aside, each with its receipt. They stay in
    // the record and never come back as suggestions.
    readJson(fetch, "/learning/candidate-rejections/projection.json"),
    // The lessons a person has let steer future fleet casting, each a
    // reversible receipt-backed grant. Empty until one is opened.
    readJson(fetch, "/learning/adaptation-grants/projection.json")
  ]);
  const forgeOutcomes = deriveForgeOutcomes(runs);
  // Promotion items are the kernel's drafted candidates, not templated ids
  // over the seed file. The seed ledger stays display-only history below.
  const lessonPromotions = deriveLessonPromotions(candidateLessonProjection);
  const judgmentRecords = deriveJudgmentRecords(lessonPromotions);
  const judgmentDecisions = deriveJudgmentDecisions(judgmentDecisionProjection, judgmentRecords);
  const memoryTargets = deriveMemoryTargets(judgmentRecords, judgmentDecisions);
  const memoryQueue = deriveMemoryQueue(judgmentDecisions, memoryTargets);
  const memoryCandidates = deriveMemoryCandidates(memoryPromotionProjection, memoryQueue);
  const activeMemories = deriveActiveMemories(memoryActivationProjection, memoryCandidates);
  const adaptationProposals = deriveAdaptationProposals(memoryTargets);
  // The kernel's own candidate lessons from Forge outcomes - citation-only,
  // each one waiting on a human before any plan can cite it.
  const candidateLessons = Array.isArray(candidateLessonProjection?.candidates)
    ? candidateLessonProjection.candidates
    : [];
  const pendingConsolidations = Array.isArray(consolidationProjection?.pending)
    ? consolidationProjection.pending
    : [];
  const candidateRejections = Array.isArray(rejectionProjection?.rejections)
    ? rejectionProjection.rejections
    : [];
  // The receipt-backed casting grants a person has opened over activated
  // lessons. The active-memory section joins each open grant to its lesson so
  // it can offer withdraw.
  const castingGrants = Array.isArray(adaptationGrantProjection?.grants)
    ? adaptationGrantProjection.grants
    : [];
  return {
    activeMemories,
    castingGrants,
    pendingConsolidations,
    candidateLessons,
    candidateRejections,
    adaptationProposalContract,
    adaptationProposals,
    contract,
    evidenceRequestContract,
    evidenceRequests,
    forgeOutcomeContract,
    forgeOutcomes,
    judgmentDecisionContract,
    judgmentDecisions,
    judgmentRecordContract,
    judgmentRecords,
    ledger,
    lessonPromotionContract,
    lessonPromotions,
    memoryCandidates,
    memoryActivationContract,
    memoryPromotionContract,
    memorySupersedeContract,
    memoryPromotionTargetContract,
    memoryQueue,
    memoryTargets,
    boundary:
      "Learn can ask for stronger evidence, record judgment decisions, request memory promotion, activate MDx-owned memory receipts, reject a drafted lesson, and retire a lesson that no longer holds. It can also let an activated lesson steer future fleet casting - preferring or avoiding the runners it names - and take that back at any time. It changes nothing else about how MDx behaves.",
    session,
    safeNext:
      "Review the Forge path, then add outcome records before any casting, check, budget, or review behavior changes."
  };
}
