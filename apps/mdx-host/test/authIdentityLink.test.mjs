import test from "node:test";
import assert from "node:assert/strict";
import { GET as startAppleLink } from "../src/routes/auth/link/apple/+server.js";
import { GET as finishOAuth } from "../src/routes/auth/callback/+server.js";

async function redirectFrom(run) {
  try {
    await run();
  } catch (error) {
    assert.equal(error?.status, 303);
    return error.location;
  }
  assert.fail("expected a redirect");
}

test("Apple linking requires an existing verified MDx session", async () => {
  const location = await redirectFrom(() =>
    startAppleLink({
      locals: { session: { authenticated: false }, supabase: {} },
      url: new URL("https://mdx.test/auth/link/apple")
    })
  );
  assert.equal(location, "/auth/sign-in?next=%2Fyou");
});

test("Apple linking keeps the current account and returns to You", async () => {
  let credentials = null;
  const location = await redirectFrom(() =>
    startAppleLink({
      locals: {
        session: { authenticated: true },
        supabase: {
          auth: {
            linkIdentity: async (value) => {
              credentials = value;
              return { data: { url: "https://appleid.apple.com/auth/authorize?state=proof" }, error: null };
            }
          }
        }
      },
      url: new URL("https://mdx.test/auth/link/apple")
    })
  );
  assert.equal(location, "https://appleid.apple.com/auth/authorize?state=proof");
  assert.equal(credentials.provider, "apple");
  const callback = new URL(credentials.options.redirectTo);
  assert.equal(callback.origin, "https://mdx.test");
  assert.equal(callback.pathname, "/auth/callback");
  assert.equal(callback.searchParams.get("flow"), "link");
  assert.equal(callback.searchParams.get("next"), "/you?identity=apple-linked");
});

test("a failed Apple link preserves the admitted session and returns to You", async () => {
  const location = await redirectFrom(() =>
    finishOAuth({
      locals: {
        session: { authenticated: true },
        supabase: {
          auth: {
            exchangeCodeForSession: async () => ({ error: new Error("identity already exists") })
          }
        }
      },
      url: new URL("https://mdx.test/auth/callback?code=proof&flow=link")
    })
  );
  assert.equal(location, "/you?identity=apple-link-failed");
});
