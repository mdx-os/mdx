import test from "node:test";
import assert from "node:assert/strict";
import { actions } from "../src/routes/waitlist/+page.server.js";

function requestFor(email = "ada@example.com") {
  const body = new FormData();
  body.set("name", "Ada Lovelace");
  body.set("email", email);
  body.set("role", "engineer");
  body.set("goal", "Try a bounded Forge build");
  body.set("consent", "on");
  return new Request("https://mdx.example/waitlist", { method: "POST", body });
}

const recordedReceipt = async () => Response.json({ waitlist_receipt_id: "waitlist_receipt_123" });

test("waitlist success requires a recoverable invitation contact", async () => {
  const result = await actions.default({
    request: requestFor(),
    fetch: recordedReceipt,
    locals: {
      supabase: {
        from: () => ({ insert: async () => ({ error: { code: "provider_unavailable" } }) })
      }
    }
  });

  assert.equal(result.status, 502);
  assert.match(result.data.error, /cannot send an invite yet/);
});

test("waitlist accepts a stored contact even when the governed receipt is temporarily unavailable", async () => {
  const result = await actions.default({
    request: requestFor(),
    fetch: async () => {
      throw new Error("kernel temporarily unavailable");
    },
    locals: {
      supabase: {
        from: () => ({ insert: async () => ({ error: null }) })
      }
    }
  });

  assert.equal(result.success, true);
  assert.equal(result.firstName, "Ada");
  assert.equal(result.receiptId, "");
});

test("waitlist treats a duplicate stored contact as a safe success", async () => {
  const result = await actions.default({
    request: requestFor("ADA@example.com"),
    fetch: recordedReceipt,
    locals: {
      supabase: {
        from: () => ({ insert: async () => ({ error: { code: "23505" } }) })
      }
    }
  });

  assert.equal(result.success, true);
  assert.equal(result.firstName, "Ada");
});
