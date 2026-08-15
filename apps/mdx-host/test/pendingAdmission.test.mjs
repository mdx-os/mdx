import test from "node:test";
import assert from "node:assert/strict";
import { GET as finishOAuth } from "../src/routes/auth/callback/+server.js";
import { actions, load } from "../src/routes/welcome/pending/+page.server.js";

function actionRequest(next = "/redeem?invite=invite_123") {
  const body = new FormData();
  body.set("next", next);
  return new Request("https://mdx.test/welcome/pending", { method: "POST", body });
}

function pendingSupabase(claims = {}) {
  return {
    auth: {
      getUser: async () => ({
        data: { user: { email: "canary@example.test", user_metadata: { name: "Canary" } } },
        error: null
      }),
      refreshSession: async () => ({ data: { session: {} }, error: null }),
      getClaims: async () => ({ data: { claims }, error: null })
    }
  };
}

async function redirectFrom(run) {
  try {
    await run();
  } catch (error) {
    assert.equal(error?.status, 303);
    return error.location;
  }
  assert.fail("expected a redirect");
}

test("the pending room preserves the governed invite destination", async () => {
  const previous = process.env.MDX_EXPERIENCE_PROFILE;
  process.env.MDX_EXPERIENCE_PROFILE = "public-beta";
  try {
    const result = await load({
      locals: { session: { authenticated: false }, supabase: pendingSupabase() },
      url: new URL("https://mdx.test/welcome/pending?next=%2Fredeem%3Finvite%3Dinvite_123")
    });
    assert.equal(result.next, "/redeem?invite=invite_123");
    assert.equal(result.identity.name, "Canary");
  } finally {
    if (previous === undefined) delete process.env.MDX_EXPERIENCE_PROFILE;
    else process.env.MDX_EXPERIENCE_PROFILE = previous;
  }
});

test("OAuth sends an unadmitted identity to the waiting room with its invite intact", async () => {
  const previous = process.env.MDX_EXPERIENCE_PROFILE;
  process.env.MDX_EXPERIENCE_PROFILE = "public-beta";
  try {
    const location = await redirectFrom(() =>
      finishOAuth({
        locals: {
          session: { authenticated: false },
          supabase: {
            auth: {
              exchangeCodeForSession: async () => ({ error: null }),
              getClaims: async () => ({ data: { claims: {} }, error: null })
            }
          }
        },
        url: new URL(
          "https://mdx.test/auth/callback?code=proof&next=%2Fredeem%3Finvite%3Dinvite_123"
        )
      })
    );
    assert.equal(
      location,
      "/welcome/pending?next=%2Fredeem%3Finvite%3Dinvite_123"
    );
  } finally {
    if (previous === undefined) delete process.env.MDX_EXPERIENCE_PROFILE;
    else process.env.MDX_EXPERIENCE_PROFILE = previous;
  }
});

test("a failed first sign-in returns to a human recovery screen", async () => {
  const location = await redirectFrom(() =>
    finishOAuth({
      locals: {
        session: { authenticated: false },
        supabase: {
          auth: {
            exchangeCodeForSession: async () => ({ error: new Error("provider exchange failed") })
          }
        }
      },
      url: new URL("https://mdx.test/auth/callback?code=proof&next=%2Fdownload%2Fmacos")
    })
  );
  assert.equal(location, "/auth/sign-in?next=%2Fdownload%2Fmacos&auth=exchange-failed");
});

test("checking access keeps an unadmitted provider session in the waiting room", async () => {
  const result = await actions.default({
    locals: { supabase: pendingSupabase() },
    request: actionRequest()
  });
  assert.equal(result.pending, true);
  assert.equal(result.next, "/redeem?invite=invite_123");
});

test("checking access continues the invite after receipt-backed admission", async () => {
  const location = await redirectFrom(() =>
    actions.default({
      locals: {
        supabase: pendingSupabase({
          mdx_tenant_id: "tenant_personal_beta",
          mdx_role: "viewer",
          mdx_actor_kind: "human"
        })
      },
      request: actionRequest()
    })
  );
  assert.equal(location, "/redeem?invite=invite_123");
});

test("checking access refuses an unsafe cross-origin destination", async () => {
  const location = await redirectFrom(() =>
    actions.default({
      locals: {
        supabase: pendingSupabase({
          mdx_tenant_id: "tenant_personal_beta",
          mdx_role: "viewer",
          mdx_actor_kind: "human"
        })
      },
      request: actionRequest("//attacker.example")
    })
  );
  assert.equal(location, "/welcome/beta");
});
