// The host's own telemetry sink: same origin, no third parties. Events
// land as NDJSON on the machine the host runs on (or wherever the
// operator points MDX_HOST_TELEMETRY_DIR). Every line is rebuilt from an
// explicit field allowlist - unknown fields and unknown event types never
// reach disk, message strings are truncated, and oversized batches are
// refused rather than trimmed silently.
import { appendFile, mkdir, readdir, readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { json } from "@sveltejs/kit";

const MAX_EVENTS_PER_BATCH = 100;
const MAX_BODY_BYTES = 64 * 1024;
const MAX_MESSAGE_CHARS = 200;
const LOCAL_HOSTS = new Set(["localhost", "127.0.0.1", "::1"]);
const FORBIDDEN_VALUE_MARKERS = [
  "secret",
  "password",
  "passwd",
  "api_key",
  "apikey",
  "api key",
  "bearer ",
  "authorization:",
  "private key",
  "-----begin",
  "client_secret",
  "access_token",
  "refresh_token",
  "credential",
  "token=",
  "token:",
  "system prompt",
  "prompt:",
  "model output",
  "output:",
  "completion:",
  "assistant:",
  "ignore previous instructions",
  "message body",
  "page body",
  "message_body",
  "page_body",
  "draft_text"
];

const FIELDS_BY_TYPE = {
  route_timing: ["route", "duration_ms", "budget_ms"],
  web_vital: ["name", "value", "rating", "budget_ms"],
  relay_lag: ["stream", "cursor", "lag_ms", "budget_ms"],
  interaction_timing: ["route", "duration_ms", "budget_ms", "interaction_kind"],
  client_error: ["route", "source", "message"],
  activation_step: ["route", "ts", "step", "completed"],
  surface_visit: ["route", "ts", "surface"],
  session_start: ["route", "ts"],
  session_end: ["route", "ts", "duration_ms"],
  flow_abandoned: ["route", "ts", "flow", "last_step"],
  action_abandoned: ["route", "ts", "action", "reason_category"],
  feedback_opened: ["route", "ts", "surface"]
};
const BETA_EVENT_TYPES = new Set([
  "activation_step",
  "surface_visit",
  "session_start",
  "session_end",
  "flow_abandoned",
  "action_abandoned",
  "feedback_opened",
  "interaction_timing"
]);

const telemetryDir = resolve(
  process.env.MDX_HOST_TELEMETRY_DIR ?? resolve(process.cwd(), "../../.mdx-local/host-telemetry")
);

function tokenLike(value) {
  const trimmed = value.replace(/^[^A-Za-z0-9_.-]+|[^A-Za-z0-9_.-]+$/g, "");
  if (trimmed.length < 12) return false;
  const lower = trimmed.toLowerCase();
  if (["sk-", "xai-", "ghp_", "gho_", "github_pat_", "akia"].some((prefix) => lower.startsWith(prefix))) {
    return true;
  }
  return lower.startsWith("eyj") && (trimmed.match(/\./g) ?? []).length >= 2;
}

function unsafeMarker(value) {
  const lower = value.toLowerCase();
  const marker = FORBIDDEN_VALUE_MARKERS.find((entry) => lower.includes(entry));
  if (marker) return marker;
  for (const word of value.split(/[\s"'`]+/)) {
    if (tokenLike(word)) return "token_like_string";
  }
  return null;
}

function safeRoute(value) {
  if (typeof value !== "string") return "";
  if (!value.startsWith("/") || value.includes("?") || value.includes("#") || /\s/.test(value) || value.includes("://")) {
    return "";
  }
  if (unsafeMarker(value)) return "";
  return value.slice(0, MAX_MESSAGE_CHARS);
}

function safeAttemptedKind(value) {
  if (typeof value !== "string" || unsafeMarker(value)) return "unknown";
  return value.slice(0, 64);
}

function rejection(event, reason) {
  return {
    type: "telemetry_rejected",
    attempted_event_kind: safeAttemptedKind(event?.type),
    rejection_reason: reason,
    route: safeRoute(event?.route),
    unsafe_payload_persisted: false,
    received_at: new Date().toISOString()
  };
}

function sanitize(event) {
  const fields = FIELDS_BY_TYPE[event?.type];
  if (!fields) {
    return rejection(event, "unknown_event_kind");
  }
  const acceptedKeys = new Set(["type", "release", "recorded_at", ...fields]);
  if (Object.keys(event ?? {}).some((key) => !acceptedKeys.has(key))) {
    return rejection(event, "unknown_field");
  }
  const line = { type: event.type };
  for (const field of fields) {
    const value = event[field];
    if (typeof value === "string") {
      if (unsafeMarker(value)) return rejection(event, "unsafe_value");
      if (field === "route" && safeRoute(value) !== value.slice(0, MAX_MESSAGE_CHARS)) {
        return rejection(event, "invalid_route");
      }
      line[field] = value.slice(0, MAX_MESSAGE_CHARS);
    } else if (typeof value === "number" && Number.isFinite(value)) {
      line[field] = value;
    } else if (typeof value === "boolean") {
      line[field] = value;
    } else if (value == null) {
      line[field] = null;
    }
  }
  if (typeof event.release === "string") {
    if (unsafeMarker(event.release)) return rejection(event, "unsafe_value");
    line.release = event.release.slice(0, 64);
  }
  if (typeof event.recorded_at === "string") {
    if (unsafeMarker(event.recorded_at)) return rejection(event, "unsafe_value");
    line.recorded_at = event.recorded_at.slice(0, 32);
  }
  line.received_at = new Date().toISOString();
  return line;
}

function telemetryWritesAllowed(request, url) {
  const explicit = process.env.MDX_HOST_TELEMETRY_WRITE_ENABLED;
  if (explicit === "0" || explicit === "false") return false;
  if (explicit === "1" || explicit === "true") return true;
  if (!LOCAL_HOSTS.has(url.hostname)) return false;
  const origin = request.headers.get("origin");
  return !origin || origin === url.origin;
}

export async function POST({ request, url, fetch: fetchImpl, locals }) {
  const localWritesAllowed = telemetryWritesAllowed(request, url);
  const authenticated = locals?.session?.authenticated === true;
  const raw = await request.text();
  if (raw.length > MAX_BODY_BYTES) {
    return json({ error: "batch too large" }, { status: 413 });
  }

  let body;
  try {
    body = JSON.parse(raw);
  } catch (error) {
    return json({ error: "invalid json" }, { status: 400 });
  }
  if (!Array.isArray(body?.events)) {
    return json({ error: "events array required" }, { status: 400 });
  }
  if (body.events.length > MAX_EVENTS_PER_BATCH) {
    return json({ error: "too many events" }, { status: 413 });
  }

  const lines = body.events.map(sanitize).filter(Boolean);
  if (lines.length === 0) {
    return json({ stored: 0 });
  }

  const betaLines = authenticated
    ? lines.filter((line) => BETA_EVENT_TYPES.has(line.type))
    : [];
  if (!localWritesAllowed && betaLines.length === 0) {
    return json({ error: "telemetry writes disabled" }, { status: 403 });
  }

  let recorded = 0;
  let kernelRejected = 0;
  if (betaLines.length > 0) {
    try {
      for (const line of betaLines) {
        const payload = {
          type: line.type,
          route: line.route || "/",
          ts: line.ts || line.recorded_at || line.received_at
        };
        for (const field of FIELDS_BY_TYPE[line.type] ?? []) {
          if (!matchesKernelIdentityField(field) && line[field] != null) {
            payload[field] = line[field];
          }
        }
        if (typeof line.release === "string" && line.release) {
          payload.app_version = line.release;
        }
        const response = await fetchImpl("/api/kernel/beta/telemetry-events.json", {
          method: "POST",
          headers: { "content-type": "application/json", accept: "application/json" },
          body: JSON.stringify(payload)
        });
        if (!response.ok) throw new Error("kernel telemetry unavailable");
        const result = await response.json();
        if (result?.status === "BETA_TELEMETRY_RECORDED") recorded += 1;
        else kernelRejected += 1;
      }
    } catch (error) {
      return json({ error: "kernel telemetry unavailable" }, { status: 503 });
    }
  }

  // When the kernel accepted an authenticated Source-B event it is already
  // durable in the receipt ledger. Do not also append it to ephemeral host
  // disk, which would duplicate it on retries. Performance-only and rejected
  // events retain the original local NDJSON behavior.
  const localLines = betaLines.length > 0
    ? lines.filter((line) => !BETA_EVENT_TYPES.has(line.type))
    : lines;
  if (localWritesAllowed && localLines.length > 0) {
    try {
      await mkdir(telemetryDir, { recursive: true });
      const day = new Date().toISOString().slice(0, 10);
      await appendFile(
        resolve(telemetryDir, `events-${day}.ndjson`),
        localLines.map((line) => JSON.stringify(line)).join("\n") + "\n",
        "utf8"
      );
    } catch (error) {
      // The sink could not write; the client keeps its events pending.
      return json({ error: "sink unavailable" }, { status: 503 });
    }
  }

  return json({
    stored: localLines.filter((line) => line.type !== "telemetry_rejected").length,
    recorded,
    rejected: localLines.filter((line) => line.type === "telemetry_rejected").length + kernelRejected
  });
}

function matchesKernelIdentityField(field) {
  return field === "type" || field === "route" || field === "ts";
}

function percentile(sorted, pct) {
  if (sorted.length === 0) return 0;
  const rank = Math.max(1, Math.ceil(sorted.length * pct));
  return sorted[Math.min(rank, sorted.length) - 1];
}

async function readEvents() {
  let files = [];
  try {
    files = (await readdir(telemetryDir))
      .filter((file) => /^events-\d{4}-\d{2}-\d{2}\.ndjson$/.test(file))
      .sort()
      .slice(-7);
  } catch (error) {
    return [];
  }
  const events = [];
  for (const file of files) {
    try {
      const text = await readFile(resolve(telemetryDir, file), "utf8");
      for (const line of text.split("\n")) {
        if (!line.trim()) continue;
        try {
          events.push(JSON.parse(line));
        } catch (error) {
          // Ignore malformed local lines; they do not become green signal.
        }
      }
    } catch (error) {
      // Missing or unreadable file: keep the summary honest from what remains.
    }
  }
  return events.slice(-2000);
}

export async function GET() {
  const events = await readEvents();
  const interactionSamples = events
    .filter((event) => event?.type === "interaction_timing" && Number.isFinite(event.duration_ms))
    .map((event) => Number(event.duration_ms))
    .sort((a, b) => a - b);
  const routeSamples = events
    .filter((event) => event?.type === "route_timing" && Number.isFinite(event.duration_ms))
    .map((event) => Number(event.duration_ms))
    .sort((a, b) => a - b);
  const inpSamples = events
    .filter((event) => event?.type === "web_vital" && event.name === "INP" && Number.isFinite(event.value))
    .map((event) => Number(event.value))
    .sort((a, b) => a - b);

  return json({
    name: "mdx-host-telemetry-summary",
    status: events.length > 0 ? "OK" : "NOT_OBSERVED",
    event_count: events.length,
    rejected_event_count: events.filter((event) => event?.type === "telemetry_rejected").length,
    measurement_source: "host_ndjson_same_origin",
    interaction_p75_observed: interactionSamples.length > 0 || inpSamples.length > 0,
    interaction_p75_ms:
      interactionSamples.length > 0 ? percentile(interactionSamples, 0.75) : percentile(inpSamples, 0.75),
    interaction_sample_count: interactionSamples.length,
    inp_sample_count: inpSamples.length,
    route_transition_p75_ms: percentile(routeSamples, 0.75),
    route_transition_sample_count: routeSamples.length,
    production_write_allowed: false
  });
}
