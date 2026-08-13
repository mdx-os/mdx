import test from "node:test";
import assert from "node:assert/strict";
import {
  deliverInvite,
  inviteDeliveryReadiness,
  inviteMessage
} from "../src/lib/server/inviteDelivery.server.js";

const configured = {
  MDX_BETA_INVITE_EMAIL_PROVIDER: "resend",
  MDX_RESEND_API_KEY: "test-only-key",
  MDX_BETA_INVITE_FROM: "MDx <beta@example.com>"
};

test("invite delivery stays closed without every server-side input", () => {
  const readiness = inviteDeliveryReadiness({ MDX_BETA_INVITE_EMAIL_PROVIDER: "resend" });
  assert.equal(readiness.ready, false);
  assert.deepEqual(readiness.missing, ["MDX_RESEND_API_KEY", "MDX_BETA_INVITE_FROM"]);
});

test("invite copy points at the governed redemption route", () => {
  const message = inviteMessage({
    fullName: "Ada Lovelace",
    email: "ADA@example.com",
    inviteId: "invite_123",
    origin: "https://mdx.example"
  });
  assert.equal(message.to, "ada@example.com");
  assert.match(message.text, /https:\/\/mdx\.example\/redeem\?invite=invite_123/);
  assert.match(message.text, /choose Google or Apple for this invited email address/);
  assert.match(message.text, /same MDx workspace on web, Mac, and iPhone/);
  assert.match(message.text, /reply to this thread when it is ready/);
  assert.match(message.text, /choose Check access there, or open this redemption link again/);
  assert.match(message.text, /notarized Mac download and the TestFlight iPhone handoff/);
  assert.match(message.text, /connect Apple later from You/);
  assert.match(message.text, /Reply to this email/);
  assert.doesNotMatch(message.text, /test-only-key/);
});

test("Resend delivery uses a stable idempotency key", async () => {
  let request;
  const result = await deliverInvite(
    { to: "ada@example.com", subject: "Invite", text: "Body" },
    "invite_123",
    async (url, init) => {
      request = { url, init };
      return new Response(JSON.stringify({ id: "email_123" }), {
        status: 200,
        headers: { "content-type": "application/json" }
      });
    },
    configured
  );
  assert.equal(result.ok, true);
  assert.equal(result.id, "email_123");
  assert.equal(request.url, "https://api.resend.com/emails");
  assert.equal(request.init.headers["idempotency-key"], "mdx-beta-invite/invite_123");
  assert.equal(JSON.parse(request.init.body).to[0], "ada@example.com");
});
