import { randomUUID } from "node:crypto";
import { error, fail } from "@sveltejs/kit";
import { adminAccess } from "../../../lib/session.server.js";
import {
  deliverInvite,
  inviteDeliveryReadiness,
  inviteMessage
} from "../../../lib/server/inviteDelivery.server.js";
import { latestMacReleaseManifest } from "../../../lib/server/macosRelease.server.js";

export const csr = false;

// The beta operating dashboard. Reads only governed projections and safe
// telemetry aggregates through the same-origin proxy; it renders counts,
// kinds, reasons, and states, never a prompt, message, page body, output,
// secret, or raw receipt payload. Every read fails soft to null so a
// kernel-down dashboard still opens and says so. See
// docs/BETA-OPERATING-DASHBOARD-SPEC.md.
async function readJson(fetchImpl, path) {
  try {
    const response = await fetchImpl(`/api/kernel${path}`, {
      headers: { accept: "application/json" },
      signal: AbortSignal.timeout(8000)
    });
    if (!response.ok) return null;
    return await response.json();
  } catch (error) {
    return null;
  }
}

function countBy(rows, key) {
  const out = {};
  for (const row of Array.isArray(rows) ? rows : []) {
    const k = String(row?.[key] ?? "unknown");
    out[k] = (out[k] ?? 0) + 1;
  }
  return out;
}

// Contact identity for the public waitlist lives provider-side (the
// beta_signups table), never in the kernel. Operators read it here with
// their own session token: row-level security admits only tokens carrying
// the owner claim, so the host needs no service-role key.
async function readSignups(locals) {
  if (!locals?.supabase) return { available: false, rows: [] };
  try {
    const { data, error } = await locals.supabase
      .from("beta_signups")
      .select("id, created_at, full_name, email, email_hash, role, build_goal, referral_source, invited_at, invite_id, invite_delivery_status, invite_delivery_provider, invite_delivery_id, invite_delivery_attempted_at")
      .order("created_at", { ascending: false })
      .limit(200);
    if (error) return { available: false, rows: [] };
    return { available: true, rows: Array.isArray(data) ? data : [] };
  } catch (error) {
    return { available: false, rows: [] };
  }
}

async function readLatestMacRelease(locals, fetchImpl) {
  if (!locals?.session?.authenticated || !locals?.supabase) return null;
  try {
    return await latestMacReleaseManifest({ locals, fetch: fetchImpl });
  } catch (error) {
    return null;
  }
}

function participantNextAction(journey) {
  if (journey?.product_analytics_consent === false) return "Support directly - product telemetry is off";
  if (Number(journey?.event_count ?? 0) === 0) return "Confirm their first signed-in launch";
  if (journey?.first_value_observed !== true) return "Help them reach a first Forge result";
  if (journey?.return_session_observed !== true) return "Ask them back for a second session";
  if (!journey?.last_app_version) return "Confirm the Mac build they are using";
  return "Review feedback and keep observing";
}

function participantLastSeen(value) {
  if (!value) return "not seen yet";
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? String(value)
    : new Intl.DateTimeFormat("en", { dateStyle: "medium", timeStyle: "short" }).format(date);
}

export async function load({ fetch, locals }) {
  const signups = await readSignups(locals);
  const [
    enrollments,
    telemetry,
    flywheel,
    metrics,
    watchtower,
    feedback,
    runs,
    outcomes,
    candidates,
    activeMemory,
    installed,
    kernelVersion,
    waitlist,
    inviteRequests,
    inviteRedemptions,
    latestMacRelease
  ] = await Promise.all([
    readJson(fetch, "/beta/enrollments/projection.json"),
    readJson(fetch, "/beta/telemetry-events/projection.json"),
    readJson(fetch, "/forge/flywheel-proof.json"),
    readJson(fetch, "/runtime/metrics.json"),
    readJson(fetch, "/runtime/watchtower.json"),
    readJson(fetch, "/feedback/captures/projection.json"),
    readJson(fetch, "/forge/runs/projection.json"),
    readJson(fetch, "/forge/outcome-signals/projection.json"),
    readJson(fetch, "/learning/forge-outcome-candidates/projection.json"),
    readJson(fetch, "/learning/memory-activations/projection.json"),
    readJson(fetch, "/marketplace/installed-capabilities/projection.json"),
    readJson(fetch, "/version.json"),
    readJson(fetch, "/beta/waitlist/projection.json"),
    readJson(fetch, "/auth/invite-requests/projection.json"),
    readJson(fetch, "/auth/invite-redemptions/projection.json"),
    readLatestMacRelease(locals, fetch)
  ]);

  const reachable = enrollments !== null || flywheel !== null;

  // 1. Cohort health from the enrollment projection (the source of truth).
  const enrolled = Array.isArray(enrollments?.latest_active) ? enrollments.latest_active : [];
  const cohort = {
    activeCount: Number(enrollments?.active_count ?? enrolled.length),
    recordCount: Number(enrollments?.record_count ?? 0),
    byCohort: countBy(enrolled, "cohort_id"),
    byRole: countBy(enrolled, "role"),
    analyticsOptOut: enrolled.filter((e) => e?.product_analytics_consent === false).length,
    participants: enrolled.slice(0, 12)
  };
  const captures = Array.isArray(feedback?.reports)
    ? feedback.reports
    : Array.isArray(feedback?.captures)
      ? feedback.captures
      : Array.isArray(feedback?.feedback)
        ? feedback.feedback
        : [];
  const unresolvedCriticalCount = captures.filter((capture) => {
    const severity = String(capture?.severity ?? "").toUpperCase();
    const status = String(capture?.status ?? "").toLowerCase();
    return ["P0", "P1"].includes(severity) && !["resolved", "closed"].includes(status);
  }).length;

  // 2 and 6. Telemetry aggregates feed surface health and reliability.
  const tele = {
    available: telemetry !== null,
    acceptedKinds: Array.isArray(telemetry?.accepted_event_kinds) ? telemetry.accepted_event_kinds : [],
    operationalKinds: Array.isArray(telemetry?.operational_security_event_kinds)
      ? telemetry.operational_security_event_kinds
      : [],
    eventCounts: telemetry?.event_counts ?? {},
    rejectionCounts: telemetry?.rejection_counts ?? {},
    cohortCounts: telemetry?.cohort_counts ?? {},
    ttlDays: Number(telemetry?.raw_event_ttl_days ?? 14),
    unsafePersisted: telemetry === null ? null : telemetry?.unsafe_payload_persisted === true,
    journeys: (Array.isArray(telemetry?.participant_journeys) ? telemetry.participant_journeys : [])
      .map((journey) => ({
        ...journey,
        lastSeenLabel: participantLastSeen(journey.last_seen_at),
        signalSummary: [
          journey.first_value_observed === true ? "first value" : "first value waiting",
          journey.return_session_observed === true ? "returned" : "one session",
          journey.last_app_version || "Mac build unseen"
        ].join(" · "),
        nextAction: participantNextAction(journey)
      }))
  };

  const latestMacLabel = latestMacRelease
    ? `${latestMacRelease.version} (${latestMacRelease.build})`
    : "";
  const latestMacCount = tele.journeys.filter((journey) => journey.last_app_version === latestMacLabel).length;
  const firstValueCount = tele.journeys.filter((journey) => journey.first_value_observed === true).length;
  const returnCount = tele.journeys.filter((journey) => journey.return_session_observed === true).length;
  const canary = {
    targetMin: 3,
    targetMax: 5,
    participantCount: cohort.activeCount,
    firstValueCount,
    returnCount,
    latestMacCount,
    supportBlockerCount: unresolvedCriticalCount,
    latestMacLabel,
    stage:
      cohort.activeCount < 3
        ? "Forming the cohort"
        : firstValueCount < cohort.activeCount
          ? "Helping everyone reach first value"
          : returnCount < Math.min(3, cohort.activeCount)
            ? "Observing return sessions"
            : !latestMacLabel
              ? "Waiting for a proven Mac release"
              : latestMacCount < cohort.activeCount
                ? "Moving everyone to the latest Mac build"
                : unresolvedCriticalCount > 0
                  ? "Resolving critical support findings"
                  : "Ready for founder review",
    gates: [
      { label: "3-5 active participants", met: cohort.activeCount >= 3 && cohort.activeCount <= 5 },
      { label: "Every participant reached first value", met: cohort.activeCount >= 3 && firstValueCount === cohort.activeCount },
      { label: "At least 3 returned on another day", met: returnCount >= 3 },
      { label: "Every participant on latest Mac build", met: Boolean(latestMacLabel) && cohort.activeCount >= 3 && latestMacCount === cohort.activeCount },
      { label: "No unresolved P0 or P1 findings", met: unresolvedCriticalCount === 0 },
      { label: "No unsafe telemetry persisted", met: tele.available && tele.unsafePersisted === false }
    ]
  };

  // 3. Forge health.
  const runRows = Array.isArray(runs?.runs) ? runs.runs : [];
  const forge = {
    runCount: runRows.length,
    checksPassed: runRows.reduce((n, r) => n + Number(r?.checks_passed ?? 0), 0),
    checksFailed: runRows.reduce((n, r) => n + Number(r?.checks_failed ?? 0), 0),
    withOutcome: runRows.filter((r) => String(r?.outcome_disposition ?? "")).length,
    localReal: runs?.local_real ?? null,
    outcomeCount: Number(outcomes?.outcome_signal_count ?? 0)
  };

  // 4. Learning and flywheel ladder.
  const compounding = flywheel?.compounding_proof ?? {};
  const measured = flywheel?.measured_improvement_proof ?? {};
  const learning = {
    outcomes: Number(flywheel?.outcome_signal_count ?? 0),
    candidates: Number(candidates?.candidate_count ?? flywheel?.candidate_lesson_count ?? 0),
    activeMemories: Number(activeMemory?.active_memory_count ?? flywheel?.active_memory_count ?? 0),
    citationEvents: Number(compounding?.lesson_citation_event_count ?? 0),
    comparablePairs: Number(measured?.comparable_lap_pair_count ?? 0),
    measuredStatus: String(measured?.status ?? "INSUFFICIENT_SAMPLES"),
    ladder: {
      wired: flywheel?.status === "LOCAL_REAL_FLYWHEEL_PROOF_READY",
      turns: compounding?.status === "OBSERVED_TWO_LAP_CITATION_LOOP",
      compounds: measured?.status === "OBSERVED_MEASURED_IMPROVEMENT",
      selfImproves: flywheel?.learning_posture?.adaptation_allowed === true
    }
  };

  // 5. Trust and safety from the operator pulse.
  const pulse = watchtower?.pulse ?? watchtower ?? {};
  const trust = {
    blocked: pulse?.blocked ?? pulse?.blocked_counts ?? null,
    refusals: pulse?.refusals ?? pulse?.refusal_counts ?? null,
    quarantines: Number(installed?.blocked_execution_count ?? 0),
    capabilityExecutionAllowed: installed?.capability_execution_allowed === true,
    raw: pulse
  };

  // 6. Reliability: latency and honest unmeasured gaps.
  const reliability = {
    notYetInstrumented: Array.isArray(metrics?.not_yet_instrumented) ? metrics.not_yet_instrumented : [],
    interactionBudgetMs: 200,
    raw: metrics
  };

  // 7. Support from the safe feedback captures. The projection carries the
  // triage lifecycle (received / triaged / working / resolved) per report.
  const support = {
    count: captures.length,
    bySeverity: countBy(captures, "severity"),
    byCategory: countBy(captures, "category"),
    bySurface: countBy(captures, "surface"),
    recent: captures.slice(-12).reverse()
  };

  // The front gate: waitlist queue -> issued invites -> redemptions. Rows
  // fail soft to empty; the honesty flags (email delivery closed, local
  // stub) render on the page per the kernel's ui_handoff contract.
  // Field names verified against the live projections (walkthrough finding:
  // the earlier guesses read "Waitlist 0" while the kernel held the entry).
  const admissions = {
    waitlist: Array.isArray(waitlist?.records) ? waitlist.records : [],
    invites: Array.isArray(inviteRequests?.invites) ? inviteRequests.invites : [],
    redemptions: Array.isArray(inviteRedemptions?.redemptions) ? inviteRedemptions.redemptions : [],
    signups
  };

  return {
    reachable,
    admissions,
    kernel: kernelVersion
      ? {
          version: String(kernelVersion.kernel_version ?? ""),
          fingerprint: String(kernelVersion.contract_fingerprint ?? ""),
          routes: Number(kernelVersion.route_count ?? 0),
          minApp: String(kernelVersion.min_app_version ?? "")
        }
      : null,
    cohort,
    canary,
    telemetry: tele,
    forge,
    learning,
    trust,
    reliability,
    support,
    inviteDelivery: inviteDeliveryReadiness(),
    owners: {
      cohort: "beta lead",
      surface: "product owner",
      forge: "Forge owner",
      learning: "learning-loop owner",
      trust: "security and governance owner",
      reliability: "infra and runtime owner",
      support: "beta support owner"
    },
    boundary:
      "This room only reads and summarizes governed projections and safe telemetry aggregates. It shows counts, kinds, reasons, and states - never a prompt, message, page body, model output, secret, or raw receipt payload.",
    safeNext:
      "Enroll a cohort and connect a model, then watch activation, learning, and reliability fill in here as real beta work lands."
  };
}

export const actions = {
  invite: async ({ request, fetch, locals, url }) => {
    const access = adminAccess(locals.session);
    if (!access.allowed) error(403, "Console requires an operator role");

    const form = await request.formData();
    const signupId = String(form.get("signup_id") ?? "").trim();
    if (!signupId || !locals.supabase) return fail(400, { inviteError: "Choose a signup first." });

    const { data: signup, error: signupError } = await locals.supabase
      .from("beta_signups")
      .select("id, full_name, email, email_hash, role, invite_id, invite_delivery_status")
      .eq("id", signupId)
      .single();
    if (signupError || !signup) return fail(404, { inviteError: "That signup is no longer available." });
    if (!/^sha256:[a-f0-9]{64}$/.test(String(signup.email_hash ?? ""))) {
      return fail(422, { inviteError: "That signup has an invalid identity hash." });
    }
    if (signup.invite_delivery_status === "sent") {
      return { inviteSuccess: `Invite ${signup.invite_id} was already delivered.` };
    }

    const inviteId = String(signup.invite_id || `invite_${randomUUID().replaceAll("-", "")}`);
    if (!signup.invite_id) {
      const response = await fetch("/api/kernel/auth/invite-requests.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          invite_id: inviteId,
          invited_actor_id: `applicant_${String(signup.email_hash).replace("sha256:", "").slice(0, 16)}`,
          invited_email_hash: signup.email_hash,
          requested_role: signup.role || "engineer",
          expires_at: new Date(Date.now() + 14 * 86400000).toISOString()
        })
      });
      const packet = await response.json().catch(() => null);
      if (!response.ok || !packet?.invite_request_receipt_id) {
        return fail(502, { inviteError: packet?.reason ?? "The governed invite did not record." });
      }
      const { error: recordError } = await locals.supabase
        .from("beta_signups")
        .update({ invite_id: inviteId, invited_at: new Date().toISOString() })
        .eq("id", signupId);
      if (recordError) return fail(502, { inviteError: "The invite recorded, but delivery bookkeeping did not." });
    }

    const message = inviteMessage({ ...signup, inviteId, origin: url.origin });
    const { error: sendingRecordError } = await locals.supabase
      .from("beta_signups")
      .update({ invite_delivery_status: "sending", invite_delivery_attempted_at: new Date().toISOString() })
      .eq("id", signupId);
    if (sendingRecordError) {
      return fail(502, { inviteError: "Invite delivery could not record its attempt." });
    }
    const delivered = await deliverInvite(message, inviteId);
    const { error: deliveryRecordError } = await locals.supabase
      .from("beta_signups")
      .update({
        invite_delivery_status: delivered.ok ? "sent" : "failed",
        invite_delivery_provider: delivered.provider,
        invite_delivery_id: delivered.id,
        invite_delivery_attempted_at: new Date().toISOString()
      })
      .eq("id", signupId);
    if (deliveryRecordError) return fail(502, { inviteError: "Email delivery finished, but its receipt did not record." });
    if (!delivered.ok) return fail(502, { inviteError: delivered.reason });
    return { inviteSuccess: `Invite ${inviteId} delivered to ${signup.email}.` };
  }
};
