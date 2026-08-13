// The first real JS test floor: the typed write client is the one rail every
// governed UI write rides, so its admission and catalog contracts get
// executable tests instead of string gates. Runs on node:test, zero deps.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  MdxClientError,
  buildActorAdmission,
  createMdxClient,
  createRequestId,
  routeCatalogFromGenerated
} from "../src/index.js";

const generatedRoutes = JSON.parse(
  readFileSync(new URL("../../../generated/routes/mdx-local-routes.json", import.meta.url), "utf8")
);
const session = { tenant_id: "local_tenant", user_id: "local_user", role: "owner" };

test("request ids are unique and prefixed", () => {
  const first = createRequestId("test");
  const second = createRequestId("test");
  assert.ok(first.startsWith("test-"));
  assert.notEqual(first, second);
});

test("actor admission carries the session and the source route", () => {
  const admission = buildActorAdmission({ session, sourceRoute: "/feedback/captures.json" });
  assert.equal(admission.tenant_id, "local_tenant");
  assert.equal(admission.actor_id, "local_user");
  assert.equal(admission.actor_role, "owner");
  assert.equal(admission.source_route, "/feedback/captures.json");
  assert.ok(admission.request_id.length > 0);
});

test("actor admission refuses a session missing identity fields", () => {
  assert.throws(
    () => buildActorAdmission({ session: { tenant_id: "t" }, sourceRoute: "/x" }),
    (error) => error instanceof MdxClientError && error.details.reason === "missing_session_fields"
  );
});

test("the real generated catalog knows its governed writes", () => {
  const catalog = routeCatalogFromGenerated(generatedRoutes);
  assert.ok(catalog.isGovernedWrite("/feedback/captures.json"));
  assert.ok(catalog.isGovernedWrite("/pages/edit-drafts.json"));
  assert.ok(!catalog.isGovernedWrite("/status.json"));
  assert.ok(catalog.get("GET", "/health"));
  assert.equal(catalog.get("POST", "/route/that/does/not/exist.json"), null);
});

test("a write refuses an undeclared route before any network call", async () => {
  let fetched = false;
  const client = createMdxClient({
    fetchImpl: async () => {
      fetched = true;
      return new Response("{}", { status: 200 });
    },
    routeCatalog: routeCatalogFromGenerated(generatedRoutes),
    session
  });
  await assert.rejects(
    client.write("/route/that/does/not/exist.json", {}),
    (error) => error instanceof MdxClientError && error.details.reason === "undeclared_route"
  );
  assert.equal(fetched, false, "an undeclared route must never reach the wire");
});

test("a governed write without a buildable admission refuses before the wire", async () => {
  let fetched = false;
  const client = createMdxClient({
    fetchImpl: async () => {
      fetched = true;
      return new Response("{}", { status: 200 });
    },
    routeCatalog: routeCatalogFromGenerated(generatedRoutes),
    session: null
  });
  // The refusal is synchronous - admission is built before any promise -
  // so wrap the call to observe it either way.
  await assert.rejects(async () => client.write("/feedback/captures.json", { category: "idea" }));
  assert.equal(fetched, false);
});

test("a governed write sends the admission header and the body", async () => {
  let captured = null;
  const client = createMdxClient({
    baseUrl: "/api/kernel",
    fetchImpl: async (url, init) => {
      captured = { url, init };
      return new Response(JSON.stringify({ status: "OK" }), {
        status: 200,
        headers: { "content-type": "application/json" }
      });
    },
    routeCatalog: routeCatalogFromGenerated(generatedRoutes),
    session
  });
  const packet = await client.write("/feedback/captures.json", { category: "idea", note: "test" });
  assert.equal(packet.status, "OK");
  assert.equal(captured.url, "/api/kernel/feedback/captures.json");
  const admission = JSON.parse(captured.init.headers["X-MDX-Actor-Admission"]);
  assert.equal(admission.actor_id, "local_user");
  assert.equal(JSON.parse(captured.init.body).category, "idea");
});

test("a non-2xx answer surfaces as a typed error with the status", async () => {
  const client = createMdxClient({
    fetchImpl: async () => new Response("nope", { status: 503 }),
    session
  });
  await assert.rejects(
    client.read("/status.json"),
    (error) => error instanceof MdxClientError && error.details.status === 503
  );
});
