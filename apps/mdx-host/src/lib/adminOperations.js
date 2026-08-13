import routes from "../../../../generated/routes/mdx-local-routes.json";
import localStatus from "../../../../generated/status/mdx-local-status.json";
import runtimeBlockers from "../../../../generated/apps/mdx-runtime-turn-on-blockers.json";
import ciEvidence from "../../../../generated/verification/ci-verification-evidence.json";
import handoffEvidence from "../../../../generated/evidence/handoff-evidence-chain.json";
import perfBudget from "../../scripts/perf-budget.json";
import { evalSuites, withVerdicts } from "./evalSuites.js";

const operationRoutes = [
  "/status.json",
  "/runtime/watchtower.json",
  "/runtime/metrics.json",
  "/runtime/http-metrics.json",
  "/runtime/load-proof-summary.json",
  "/runtime/queue-projection.json",
  "/runtime/operator-packet.json",
  "/local/evidence-index.json",
  "/local/activation-summary.json",
  "/local/activation-report.json",
  "/receipts.json",
  "/receipts/{receipt_id}"
];

function asArray(value) {
  return Array.isArray(value) ? value : [];
}

function routeByPath(path) {
  const route = (routes.routes ?? []).find((candidate) => candidate.local_path === path);
  return {
    method: route?.method ?? "GET",
    path,
    operation: route?.operation_id ?? "declared",
    receiptBacked: route?.receipt_backed === true
  };
}

function statusList(statusPacket) {
  return asArray((statusPacket ?? localStatus).live_substrate).map((entry) => ({
    id: entry.substrate,
    status: entry.status,
    localAdapter: entry.local_adapter,
    liveAdapter: entry.live_adapter,
    turnOnSignal: entry.turn_on_signal,
    live: String(entry.status ?? "").startsWith("LIVE"),
    blockingRails: asArray(entry.blocking_rails),
    firstProof: entry.first_local_proof
  }));
}

function blockerList() {
  return asArray(runtimeBlockers.apps).map((entry) => ({
    name: entry.legacy_app,
    status: entry.runtime_status,
    blockedBy: asArray(entry.blocked_by),
    proofs: asArray(entry.first_local_proofs)
  }));
}

function evidenceSummary(packet) {
  const counts = packet?.counts ?? packet?.summary ?? {};
  return {
    status: packet?.status ?? "waiting for kernel",
    total: Number(packet?.evidence_count ?? packet?.total ?? counts.total ?? 0),
    complete: Number(packet?.complete_count ?? counts.complete ?? 0),
    stale: Number(packet?.stale_count ?? counts.stale ?? 0),
    route: "/local/evidence-index.json"
  };
}

function metricSummary(metrics, httpMetrics) {
  const latency = httpMetrics?.latency_micros ?? metrics?.latency_micros ?? {};
  return {
    status: metrics?.status ?? httpMetrics?.status?.health ?? "waiting for kernel",
    requests: Number(httpMetrics?.requests_total ?? metrics?.requests_total ?? 0),
    p50Ms: Math.round(Number(latency.p50 ?? 0) / 100) / 10,
    p95Ms: Math.round(Number(latency.p95 ?? 0) / 100) / 10,
    failed: Number(httpMetrics?.status?.failed_5xx ?? metrics?.failed_5xx ?? 0),
    rateLimited: Number(httpMetrics?.status?.rate_limited_429 ?? 0)
  };
}

function cloudTraceLine(httpMetrics) {
  const exportStatus = httpMetrics?.otlp_export ?? {};
  const configured = exportStatus.configured === true;
  const exported = Number(exportStatus.spans_exported ?? 0);
  const failed = Number(exportStatus.spans_failed ?? 0);
  const dropped = Number(exportStatus.spans_dropped ?? 0);
  let state = "not configured for this install";
  if (exportStatus.status === "DEGRADED") {
    state = "degraded while product paths stayed available";
  } else if (configured && exported === 0) {
    state = "ready and waiting for the first request trace";
  } else if (exported > 0) {
    state = "accepted by the configured collector transport";
  }
  return `Cloud traces are ${state}: ${exported.toLocaleString()} exported, ${failed.toLocaleString()} failed, ${dropped.toLocaleString()} dropped. Content is excluded; live status needs a provider receipt.`;
}

function queueSummary(queuePacket, watchtower) {
  const queue = queuePacket ?? watchtower?.queue_pressure ?? {};
  return {
    status: queuePacket?.status ?? "waiting for kernel",
    admitted: Number(queue.admitted ?? 0),
    queued: Number(queue.queued ?? 0),
    shed: Number(queue.shed ?? 0),
    escalated: Number(queue.escalated ?? 0)
  };
}

function latencyBreakdown(metrics, httpMetrics, loadProof) {
  const httpLatency = httpMetrics?.latency_micros ?? metrics?.latency_micros ?? {};
  const observed = metrics?.observed_local ?? {};
  const sandbox = observed.sandbox_test_duration_ms ?? {};
  const phases = observed.forge_phase_duration_ms ?? {};
  const forgePlan = phases.plan ?? {};
  const forgeExecute = phases.execute ?? {};
  const forgeTotal = observed.forge_total_run_duration_ms ?? {};
  const p50Ms = Math.round(Number(httpLatency.p50 ?? 0) / 100) / 10;
  const p95Ms = Math.round(Number(httpLatency.p95 ?? 0) / 100) / 10;
  const sandboxMeanMs = Number(sandbox.mean_ms ?? 0);

  return [
    {
      name: "Admission",
      status: metrics ? "observed" : "waiting for kernel",
      value: Number(observed.admitted ?? 0),
      unit: "admitted",
      note: "local runtime admission count"
    },
    {
      name: "Queue",
      status: metrics ? "observed" : "waiting for kernel",
      value: Number(observed.queued ?? 0),
      unit: "queued",
      note: "local queue pressure before execution authority opens"
    },
    {
      name: "HTTP",
      status: p95Ms > 0 ? "observed" : "not instrumented",
      value: p95Ms,
      unit: "p95 ms",
      note: p50Ms > 0 ? `p50 ${p50Ms}ms from runtime HTTP telemetry` : "runtime HTTP latency packet missing"
    },
    {
      name: "Sandbox test",
      status: sandboxMeanMs > 0 ? "observed" : "not instrumented",
      value: sandboxMeanMs,
      unit: "mean ms",
      note: `${Number(sandbox.count ?? 0)} observed sandbox test samples`
    },
    {
      name: "Forge plan",
      status: Number(forgePlan.count ?? 0) > 0 ? "observed" : "not instrumented",
      value: Number(forgePlan.mean_ms ?? 0),
      unit: "mean ms",
      note: `${Number(forgePlan.count ?? 0)} timed model planning samples`
    },
    {
      name: "Forge execute",
      status: Number(forgeExecute.count ?? 0) > 0 ? "observed" : "not instrumented",
      value: Number(forgeExecute.mean_ms ?? 0),
      unit: "mean ms",
      note: `${Number(forgeExecute.count ?? 0)} timed tool or check samples`
    },
    {
      name: "Forge run",
      status: Number(forgeTotal.count ?? 0) > 0 ? "observed" : "not instrumented",
      value: Number(forgeTotal.mean_ms ?? 0),
      unit: "mean ms",
      note: `${Number(forgeTotal.count ?? 0)} completed local run samples`
    },
    {
      name: "Load proof",
      status: loadProof?.status ?? "waiting for kernel",
      value: loadProof?.proof_run === true ? 1 : 0,
      unit: loadProof?.proof_run === true ? "proof run" : "pending",
      note: loadProof?.human_summary ?? loadProof?.summary ?? "load proof packet not loaded"
    }
  ];
}

function budgetRows(hostTelemetry) {
  const interactionObserved = hostTelemetry?.interaction_p75_observed === true;
  const interactionP75 = Math.round(Number(hostTelemetry?.interaction_p75_ms ?? 0));
  const routeP75 = Math.round(Number(hostTelemetry?.route_transition_p75_ms ?? 0));
  return [
    {
      name: "Client assets",
      measured: perfBudget.latest_rebaseline?.measured_client_js_css_kb ?? 0,
      budget: perfBudget.max_client_js_css_kb,
      unit: "KB",
      stance: "advisory"
    },
    {
      name: "Cold first viewport",
      measured: 0,
      budget: perfBudget.max_cold_first_viewport_ms,
      unit: "ms",
      stance: "advisory"
    },
    {
      name: "Warm route transition",
      measured: routeP75,
      budget: perfBudget.max_warm_route_transition_ms,
      unit: "ms",
      stance:
        Number(hostTelemetry?.route_transition_sample_count ?? 0) > 0
          ? "observed from host telemetry"
          : "not measured yet"
    },
    {
      name: "Interaction p75",
      measured: interactionP75,
      budget: perfBudget.max_interaction_p75_ms,
      unit: "ms",
      stance: interactionObserved ? "observed from host telemetry" : "not measured yet"
    },
    {
      name: "Relay lag warning",
      measured: 0,
      budget: perfBudget.max_relay_lag_warn_ms,
      unit: "ms",
      stance: "warn only"
    }
  ];
}

function traceRows(queuePacket, operatorPacket) {
  const jobs = asArray(queuePacket?.jobs).slice(0, 6).map((job, index) => ({
    id: job.job_id ?? job.id ?? `queue-${index + 1}`,
    stage: job.status ?? job.state ?? "queued",
    summary: job.summary ?? job.kind ?? job.source_projection ?? "queued runtime job",
    receipt: job.receipt_id ?? job.source_receipt_id ?? "",
    cost: "not instrumented"
  }));
  if (jobs.length > 0) return jobs;

  return [
    {
      id: operatorPacket?.name ?? "runtime-operator-packet",
      stage: operatorPacket?.status ?? "waiting for kernel",
      summary: operatorPacket?.human_summary ?? "no runtime trace rows loaded",
      receipt: operatorPacket?.source_projection ?? "",
      cost: "not instrumented"
    }
  ];
}

function costSummary(metrics, httpMetrics) {
  const notInstrumented = asArray(metrics?.not_yet_instrumented);
  const traces = cloudTraceLine(httpMetrics);
  const tokens = metrics?.observed_local?.forge_model_tokens ?? {};
  const input = Number(tokens.input ?? 0);
  const output = Number(tokens.output ?? 0);
  if (input + output > 0) {
    return {
      status: "token attribution observed",
      line: `${input.toLocaleString()} input and ${output.toLocaleString()} output tokens are receipt-backed; dollar cost remains unpriced until provider pricing metadata is recorded. ${traces}`,
      missing: notInstrumented.filter((item) => /cost|provider|price/i.test(item)).slice(0, 8)
    };
  }
  return {
    status: notInstrumented.some((item) => /cost|token|provider|model/i.test(item))
      ? "declared not instrumented"
      : "waiting for kernel",
    line: `Cost, tokens, provider, and model routing stay visible as missing instrumentation until a receipt-backed source exists. ${traces}`,
    missing: notInstrumented.filter((item) => /cost|token|provider|model|latency|trace/i.test(item)).slice(0, 8)
  };
}

function evalHeatmap(receiptsPacket) {
  const receiptIds = asArray(receiptsPacket?.receipt_ids);
  const suites = withVerdicts(evalSuites(), receiptIds);
  const kernelKnown = receiptsPacket != null;
  const checked = suites.filter((suite) => suite.runCount > 0).length;

  const modes = suites.map((suite) => ({
    id: suite.id,
    name: suite.name,
    status: !kernelKnown ? "waiting for kernel" : suite.runCount > 0 ? "checked" : "not yet run",
    runCount: suite.runCount,
    checkCount: suite.checkCount,
    line: suite.line,
    receipt: suite.lastRuns[0] ?? "",
    cases: suite.checks.slice(0, 4).map((check) => check.phrase)
  }));

  const traceCases = suites.flatMap((suite) =>
    suite.checks.map((check) => ({
      id: check.token,
      suite: suite.name,
      phrase: check.phrase,
      status: !kernelKnown ? "waiting for kernel" : suite.runCount > 0 ? "trace linked" : "awaiting run",
      receipt: suite.lastRuns[0] ?? "",
      target: suite.lastRuns[0] ? `/api/kernel/receipts/${suite.lastRuns[0]}` : "/quality"
    }))
  );

  return {
    status: !kernelKnown ? "waiting for kernel" : checked > 0 ? "receipt-backed" : "awaiting first run",
    line: kernelKnown
      ? `${checked} of ${suites.length} quality gates have run evidence; not-yet-run gates are not treated as failures.`
      : "Operations could not read receipts, so eval state stays unknown instead of green.",
    checked,
    total: suites.length,
    modes,
    traceCases: traceCases.slice(0, 10),
    route: "/receipts.json"
  };
}

export function adminOperations(packets = {}) {
  const statusPacket = packets["/status.json"];
  const watchtower = packets["/runtime/watchtower.json"];
  const metrics = packets["/runtime/metrics.json"];
  const httpMetrics = packets["/runtime/http-metrics.json"];
  const loadProof = packets["/runtime/load-proof-summary.json"];
  const queue = packets["/runtime/queue-projection.json"];
  const operatorPacket = packets["/runtime/operator-packet.json"];
  const activationSummary = packets["/local/activation-summary.json"];
  const activationReport = packets["/local/activation-report.json"];
  const receipts = packets["/receipts.json"];
  const hostTelemetry = packets["/api/telemetry"];

  const substrates = statusList(statusPacket);
  const liveSubstrates = substrates.filter((entry) => entry.live);

  return {
    summary: {
      substrates: substrates.length,
      liveSubstrates: liveSubstrates.length,
      pendingLive: substrates.length - liveSubstrates.length,
      routes: (routes.routes ?? []).length,
      receiptBackedRoutes: (routes.routes ?? []).filter((route) => route.receipt_backed === true).length,
      blockedApps: blockerList().length
    },
    pulse: {
      status: watchtower?.health ?? "waiting for kernel",
      line:
        watchtower?.summary ??
        `${liveSubstrates.length} live local substrates; ${substrates.length - liveSubstrates.length} pending live runs.`,
      betaHealthy: watchtower?.is_beta_healthy?.escalated_to_human === 0,
      refusals: Number(watchtower?.what_is_blocked?.refusals_total ?? 0)
    },
    metrics: metricSummary(metrics, httpMetrics),
    queue: queueSummary(queue, watchtower),
    telemetry: {
      budgetStance: "advisory budgets never block operator action",
      rebaseline: perfBudget.latest_rebaseline,
      latencyBreakdown: latencyBreakdown(metrics, httpMetrics, loadProof),
      budgets: budgetRows(hostTelemetry),
      hostTelemetry: {
        status: hostTelemetry?.status ?? "not observed",
        eventCount: Number(hostTelemetry?.event_count ?? 0),
        interactionP75Observed: hostTelemetry?.interaction_p75_observed === true,
        interactionSampleCount: Number(hostTelemetry?.interaction_sample_count ?? 0),
        inpSampleCount: Number(hostTelemetry?.inp_sample_count ?? 0)
      },
      traceRows: traceRows(queue, operatorPacket),
      cost: costSummary(metrics, httpMetrics),
      evals: evalHeatmap(receipts),
      notYetInstrumented: asArray(metrics?.not_yet_instrumented),
      overloadPosture: asArray(metrics?.overload_posture),
      standingTargets: metrics?.standing_targets ?? {}
    },
    loadProof: {
      status: loadProof?.status ?? "waiting for kernel",
      route: "/runtime/load-proof-summary.json",
      summary: loadProof?.summary ?? loadProof?.name ?? "load proof packet not loaded"
    },
    evidence: evidenceSummary(packets["/local/evidence-index.json"]),
    activation: {
      status: activationSummary?.status ?? activationReport?.status ?? "waiting for kernel",
      summary: activationSummary?.summary ?? activationReport?.summary ?? "activation packet not loaded",
      reportRoute: "/local/activation-report.json",
      summaryRoute: "/local/activation-summary.json"
    },
    substrates,
    blockers: blockerList(),
    contracts: {
      runtimeRules: runtimeBlockers.rules,
      ciWorkflow: ciEvidence.required_workflow,
      ciJobs: ciEvidence.required_jobs,
      handoffName: handoffEvidence.name,
      handoffMode: handoffEvidence.mode
    },
    routes: operationRoutes.map(routeByPath),
    sources: [
      { label: "Status", value: "generated/status/mdx-local-status.json" },
      { label: "Runtime blockers", value: "generated/apps/mdx-runtime-turn-on-blockers.json" },
      { label: "CI evidence", value: "generated/verification/ci-verification-evidence.json" },
      { label: "Handoff chain", value: "generated/evidence/handoff-evidence-chain.json" },
      { label: "Eval suites", value: "generated/eval-suites/" },
      { label: "Routes", value: "generated/routes/mdx-local-routes.json" }
    ]
  };
}
