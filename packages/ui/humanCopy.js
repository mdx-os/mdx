export const humanStatus = (value) =>
  ({
    "GENERATED-FALLBACK": "Using local sample data",
    "LOCAL-COMPLETE": "Complete",
    "LOCAL-PROOF-CURRENT": "Ready",
    "LOCAL-READY": "Ready",
    "LIVE-LOCAL": "Working locally",
    "PENDING-LIVE-RUN": "Waiting on live proof",
    "BLOCKED_ON_HUMAN_PRODUCT_RATIFICATION": "Waiting on human product review",
    "BLOCKED_ON_LIVE_WORKER_EXECUTION": "Waiting on live worker proof",
    "BLOCKED_ON_TALENT_AUTHORITY": "Waiting on Talent authority",
    "BLOCKED-BY-RAILS": "Blocked until proof is present",
    CHECKED_LOCAL_AUTHORITY: "Checked local authority",
    COMPLETED: "Complete",
    "LIVE-WORKER-RUNTIME-BLOCKED": "Live worker runtime still blocked",
    PARTIAL: "Partly ready",
    MISSING: "Missing",
    "deterministic-local-stub": "Read-only local session",
    MINTED: "Ready",
    OK: "Ready"
  })[String(value)] ?? humanToken(value).toLowerCase();

export const humanStatusKind = (value) =>
  ({
    "GENERATED-FALLBACK": "sample-data",
    "LOCAL-COMPLETE": "ready",
    "LOCAL-PROOF-CURRENT": "ready",
    "LOCAL-READY": "ready",
    "LIVE-LOCAL": "working-local",
    "PENDING-LIVE-RUN": "waiting-live",
    "BLOCKED_ON_HUMAN_PRODUCT_RATIFICATION": "blocked",
    "BLOCKED_ON_LIVE_WORKER_EXECUTION": "blocked",
    "BLOCKED_ON_TALENT_AUTHORITY": "blocked",
    PARTIAL: "partial",
    MISSING: "missing",
    "deterministic-local-stub": "working-local",
    BLOCKED: "blocked",
    "BLOCKED-BY-RAILS": "blocked",
    MINTED: "ready",
    OK: "ready"
  })[String(value)] ?? "sample-data";

export const humanToken = (value) => String(value ?? "").replace(/[_-]+/g, " ");

export const humanAction = (value) => humanToken(value).toLowerCase();

export const humanActionList = (items) =>
  items.length > 0 ? items.map(humanAction).join(", ") : "no blocked actions";

export const humanSource = (value, localEvidenceApiBase) =>
  value === "generated fallback" ? "local sample data" : String(value).replace(localEvidenceApiBase, "local API");

export const isFallbackSource = (value) => value === "generated fallback";

export const humanWhen = (value) => (value === "generated" ? "sample data" : value);

export const humanRevision = (value) => (value === "generated" ? "sample build" : value);

export const humanBranch = (value) => (value === "generated" ? "local branch" : humanToken(value));

export const humanSourceTrail = (observedAt, source, localEvidenceApiBase) =>
  observedAt === "generated" && source === "generated fallback"
    ? "Using local sample data"
    : `${humanWhen(observedAt)} via ${humanSource(source, localEvidenceApiBase)}`;

export const fallbackGuidance = {
  label: "Using local sample data",
  recovery: "Run make local-proof or make local-ui-proof to refresh local evidence.",
  boundary: "Sample data is safe for layout review, not a live operating claim."
};

export const fallbackAppears = [
  "current operating status with a timestamp",
  "live route counts from the local API",
  "local proof readiness and the next safe move"
];

export const humanSentence = (value) =>
  String(value ?? "").replace(/receipt_[a-z0-9_]+/g, (token) => humanToken(token));

export const missingList = (items) => (items.length > 0 ? items.map(humanToken).join(", ") : "nothing missing");

export const pluralize = (count, singular, plural = `${singular}s`) => (Number(count) === 1 ? singular : plural);

export const humanProofCount = (count) => `${count} ${pluralize(count, "saved proof")}`;

export const humanSession = (session) => ({
  tenant: session.tenant_id === "local_tenant" ? "Local tenant" : humanToken(session.tenant_id),
  user: session.user_id === "local_user" ? "Local operator" : humanToken(session.user_id),
  role: humanToken(session.role),
  status: session.status === "deterministic-local-stub" ? "Read-only local session" : humanStatus(session.status)
});
