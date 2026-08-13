// Marketplace server load: the curated catalog ships from the generated
// registry for an instant first paint; the folded live state (installs,
// approvals, review queue) reads from the kernel and fails soft - a
// kernel-down Marketplace still opens with the honest catalog.
import { platformProof } from "../../lib/platform.js";
import registry from "../../../../../generated/marketplace/capability-registry.json";
import packs from "../../../../../generated/marketplace/packs.json";

async function readJson(fetchImpl, path) {
  try {
    const response = await fetchImpl(`/api/kernel${path}`, {
      signal: AbortSignal.timeout(1500)
    });
    if (!response.ok) return null;
    return await response.json();
  } catch (error) {
    return null;
  }
}

export async function load({ fetch, locals }) {
  const [live, livePacks, forYou, installedPacks, impact, proposals, shelf, queue, installed, distilled, drafts] = await Promise.all([
    readJson(fetch, "/marketplace/capabilities.json"),
    readJson(fetch, "/marketplace/packs.json"),
    readJson(fetch, "/marketplace/for-you.json"),
    readJson(fetch, "/marketplace/installed-packs/projection.json"),
    readJson(fetch, "/marketplace/impact/projection.json"),
    readJson(fetch, "/marketplace/pack-proposals/projection.json"),
    readJson(fetch, "/marketplace/team-shelf.json"),
    readJson(fetch, "/marketplace/review-queue.json"),
    // The runtime-readable installed state: what the team can actually lean
    // on right now, at what scope and permission, and how isolated it stays.
    // This is what Forge and Twin read to shape plans. Fails soft to null.
    readJson(fetch, "/marketplace/installed-capabilities/projection.json"),
    // Skills distilled from runs the team already proved: procedures promoted
    // after a person signed off, each tracing back to its source run. Fails
    // soft to null so the Marketplace still opens without the kernel.
    readJson(fetch, "/marketplace/distilled-skills/projection.json"),
    readJson(fetch, "/marketplace/skill-drafts/projection.json")
  ]);

  return {
    proof: platformProof,
    session: locals.session,
    kernelReachable: live !== null,
    // The live fold when the kernel answers; the generated catalog when not.
    capabilities: live?.capabilities ?? registry.capabilities.map((entry) => ({
      ...entry,
      effective_status: entry.status,
      state: { installed: false, installed_scopes: [], tried: 0 }
    })),
    counts: {
      total: live?.capability_count ?? registry.capability_count,
      installed: live?.installed_count ?? 0,
      needsReview: live?.needs_review_count ?? registry.capabilities.filter((c) => c.status === "needs_review").length,
      updates: live?.updates_available_count ?? registry.capabilities.filter((c) => c.update).length,
      blocked: live?.blocked_count ?? registry.capabilities.filter((c) => c.status === "blocked").length
    },
    packs: Array.isArray(livePacks?.packs) ? livePacks.packs : packs.packs,
    recommendations: Array.isArray(forYou?.recommendations) ? forYou.recommendations : [],
    installedPacks: Array.isArray(installedPacks?.packs) ? installedPacks.packs : [],
    impact: impact ?? { pack_count: 0, packs: [] },
    packProposals: Array.isArray(proposals?.proposals) ? proposals.proposals : [],
    shelf,
    queue,
    // Runtime-readable installed state. Each entry names its scope, permission
    // class, risk, and isolation posture; execution stays closed here.
    installed: Array.isArray(installed?.installed) ? installed.installed : [],
    installedPosture: installed
      ? {
          count: Number(installed.installed_count ?? 0),
          executionAllowed: installed.capability_execution_allowed === true,
          secretAccessAllowed: installed.secret_access_allowed === true,
          inheritedPermissionsAllowed: installed.inherited_agent_permissions_allowed === true,
          strongerIsolationRequired:
            installed.untrusted_capability_execution_requires_stronger_isolation === true
        }
      : null,
    // Distilled skills: procedures your team's runs proved, promoted by a
    // person. Ratified ones are ready to lean on; drafts wait for a second
    // person to sign off. Each carries the run it came from.
    distilledSkills: Array.isArray(distilled?.distilled_skills) ? distilled.distilled_skills : [],
    skillDrafts: Array.isArray(drafts?.drafts) ? drafts.drafts : []
  };
}
