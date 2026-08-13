import test from "node:test";
import assert from "node:assert/strict";
import { actions, load } from "../src/routes/redeem/+page.server.js";

function redeemRequest(inviteId = "invite_123") {
  const body = new FormData();
  body.set("invite_id", inviteId);
  return new Request("https://mdx.example/redeem", { method: "POST", body });
}

test("invite redemption page distinguishes pending from admitted identity", () => {
  assert.deepEqual(
    load({ locals: { session: { authenticated: false } }, url: new URL("https://mdx.example/redeem?invite=invite_123") }),
    { authenticated: false, invite: "invite_123" }
  );
  assert.deepEqual(
    load({ locals: { session: { authenticated: true } }, url: new URL("https://mdx.example/redeem") }),
    { authenticated: true, invite: "" }
  );
});

test("pending identity cannot write an invite redemption", async () => {
  const result = await actions.default({
    request: redeemRequest(),
    locals: { session: { authenticated: false } },
    fetch: async () => {
      throw new Error("pending redemption must not reach the kernel");
    }
  });
  assert.equal(result.status, 401);
  assert.match(result.data.error, /Sign in with the invited account/);
});

test("admitted identity redeems through the governed kernel without actor claims in the body", async () => {
  let observed;
  const result = await actions.default({
    request: redeemRequest(),
    locals: { session: { authenticated: true } },
    fetch: async (path, init) => {
      observed = { path, body: JSON.parse(init.body) };
      return Response.json({ invite_redemption_receipt_id: "redemption_receipt_123" });
    }
  });
  assert.equal(result.success, true);
  assert.equal(result.receiptId, "redemption_receipt_123");
  assert.equal(observed.path, "/api/kernel/auth/invite-redemptions.json");
  assert.equal(observed.body.invite_id, "invite_123");
  assert.equal("authenticated_actor_id" in observed.body, false);
  assert.equal("tenant_id" in observed.body, false);
  assert.equal("role" in observed.body, false);
});
