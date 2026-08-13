import test from "node:test";
import assert from "node:assert/strict";
import { POST } from "../src/routes/api/telemetry/+server.js";

function telemetryRequest(events) {
  return new Request("https://beta.example/api/telemetry", {
    method: "POST",
    headers: { "content-type": "application/json", origin: "https://beta.example" },
    body: JSON.stringify({ events })
  });
}

test("authenticated Source-B telemetry forwards to the governed kernel route", async () => {
  const previous = process.env.MDX_HOST_TELEMETRY_WRITE_ENABLED;
  process.env.MDX_HOST_TELEMETRY_WRITE_ENABLED = "0";
  const forwarded = [];
  try {
    const response = await POST({
      request: telemetryRequest([
        {
          type: "activation_step",
          route: "/welcome/beta",
          ts: "2026-07-20T20:00:00Z",
          step: "first_result_seen",
          completed: true,
          release: "mdx-host-v2"
        }
      ]),
      url: new URL("https://beta.example/api/telemetry"),
      locals: { session: { authenticated: true } },
      fetch: async (path, init) => {
        forwarded.push({ path, body: JSON.parse(init.body) });
        return Response.json({ status: "BETA_TELEMETRY_RECORDED" });
      }
    });
    assert.equal(response.status, 200);
    assert.deepEqual(await response.json(), { stored: 0, recorded: 1, rejected: 0 });
    assert.deepEqual(forwarded, [
      {
        path: "/api/kernel/beta/telemetry-events.json",
        body: {
          type: "activation_step",
          route: "/welcome/beta",
          ts: "2026-07-20T20:00:00Z",
          step: "first_result_seen",
          completed: true,
          app_version: "mdx-host-v2"
        }
      }
    ]);
  } finally {
    if (previous == null) delete process.env.MDX_HOST_TELEMETRY_WRITE_ENABLED;
    else process.env.MDX_HOST_TELEMETRY_WRITE_ENABLED = previous;
  }
});

test("remote telemetry stays closed without an authenticated session", async () => {
  const previous = process.env.MDX_HOST_TELEMETRY_WRITE_ENABLED;
  process.env.MDX_HOST_TELEMETRY_WRITE_ENABLED = "0";
  try {
    const response = await POST({
      request: telemetryRequest([{ type: "session_start", route: "/", ts: "2026-07-20T20:00:00Z" }]),
      url: new URL("https://beta.example/api/telemetry"),
      locals: { session: { authenticated: false } },
      fetch: async () => {
        throw new Error("must not forward");
      }
    });
    assert.equal(response.status, 403);
  } finally {
    if (previous == null) delete process.env.MDX_HOST_TELEMETRY_WRITE_ENABLED;
    else process.env.MDX_HOST_TELEMETRY_WRITE_ENABLED = previous;
  }
});

test("kernel failure keeps the browser batch pending", async () => {
  const previous = process.env.MDX_HOST_TELEMETRY_WRITE_ENABLED;
  process.env.MDX_HOST_TELEMETRY_WRITE_ENABLED = "0";
  try {
    const response = await POST({
      request: telemetryRequest([{ type: "session_start", route: "/", ts: "2026-07-20T20:00:00Z" }]),
      url: new URL("https://beta.example/api/telemetry"),
      locals: { session: { authenticated: true } },
      fetch: async () => new Response("unavailable", { status: 503 })
    });
    assert.equal(response.status, 503);
  } finally {
    if (previous == null) delete process.env.MDX_HOST_TELEMETRY_WRITE_ENABLED;
    else process.env.MDX_HOST_TELEMETRY_WRITE_ENABLED = previous;
  }
});
