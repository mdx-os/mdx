import test from "node:test";
import assert from "node:assert/strict";
import {
  resolveHostSession,
  applyKernelIdentityHeaders,
  safeReturnPath,
  resolveOAuthProvider,
  oauthProviderQueryParams,
  adminAccess
} from "../src/lib/session.server.js";
import { handle } from "../src/hooks.server.js";
import {
  publicRootEnabled,
  publicRootRedirect
} from "../src/lib/publicRoot.server.js";

async function withEnv(values, fn) {
  const previous = {
    MDX_DEPLOYMENT_MODE: process.env.MDX_DEPLOYMENT_MODE,
    MDX_HOST_AUTH_MODE: process.env.MDX_HOST_AUTH_MODE,
    MDX_HOST_TRUSTED_IDENTITY_HEADERS: process.env.MDX_HOST_TRUSTED_IDENTITY_HEADERS,
    MDX_SUPABASE_URL: process.env.MDX_SUPABASE_URL,
    MDX_SUPABASE_PUBLISHABLE_KEY: process.env.MDX_SUPABASE_PUBLISHABLE_KEY,
    MDX_SUPABASE_ANON_KEY: process.env.MDX_SUPABASE_ANON_KEY,
    MDX_PUBLIC_ROOT_MODE: process.env.MDX_PUBLIC_ROOT_MODE,
    MDX_EXPERIENCE_PROFILE: process.env.MDX_EXPERIENCE_PROFILE
  };
  if (!("MDX_EXPERIENCE_PROFILE" in values)) {
    values = { ...values, MDX_EXPERIENCE_PROFILE: undefined };
  }
  for (const [key, value] of Object.entries(values)) {
    if (value === undefined) {
      delete process.env[key];
    } else {
      process.env[key] = value;
    }
  }
  try {
    return await fn();
  } finally {
    for (const [key, value] of Object.entries(previous)) {
      if (value === undefined) {
        delete process.env[key];
      } else {
        process.env[key] = value;
      }
    }
  }
}

test("local host sessions resolve to the generated owner fixture", async () => {
  await withEnv(
    {
      MDX_DEPLOYMENT_MODE: undefined,
      MDX_HOST_AUTH_MODE: undefined,
      MDX_HOST_TRUSTED_IDENTITY_HEADERS: undefined
    },
    async () => {
      const { session, accessToken } = await resolveHostSession({ request: new Request("http://localhost/") });
      assert.equal(session.status, "local-proof-session");
      assert.equal(session.role, "owner");
      assert.equal(session.authenticated, true);
      assert.equal(accessToken, null);
      assert.equal(adminAccess(session).allowed, true);
    }
  );
});

test("production host sessions fail closed without verified identity", async () => {
  await withEnv({ MDX_DEPLOYMENT_MODE: "production", MDX_HOST_TRUSTED_IDENTITY_HEADERS: undefined }, async () => {
    const { session, accessToken } = await resolveHostSession({ request: new Request("http://localhost/") });
    assert.equal(session.status, "supabase_auth_not_configured");
    assert.equal(session.role, "viewer");
    assert.equal(session.authenticated, false);
    assert.equal(accessToken, null);
    assert.equal(adminAccess(session).allowed, false);
  });
});

test("trusted identity headers produce a verified operator session when enabled", async () => {
  await withEnv({ MDX_DEPLOYMENT_MODE: "production", MDX_HOST_TRUSTED_IDENTITY_HEADERS: "1" }, async () => {
    const { session } = await resolveHostSession({
      request: new Request("http://localhost/", {
        headers: {
          "x-mdx-session-verified": "true",
          "x-mdx-tenant-id": "tenant_1",
          "x-mdx-user-id": "user_1",
          "x-mdx-user-role": "operator"
        }
      })
    });
    assert.equal(session.status, "verified-host-session");
    assert.equal(session.role, "operator");
    assert.equal(session.verified, true);
    assert.equal(adminAccess(session).allowed, true);
  });
});

test("a verified Supabase session maps MDx claims and keeps tokens out of page session data", async () => {
  const supabase = {
    auth: {
      getClaims: async () => ({
        data: {
          claims: {
            sub: "user_7",
            email: "tester@example.com",
            mdx_tenant_id: "tenant_beta",
            mdx_role: "operator",
            mdx_actor_kind: "human",
            iat: 1_800_000_000,
            exp: 1_800_003_600
          }
        },
        error: null
      }),
      getSession: async () => ({
        data: { session: { access_token: "signed-test-access-token" } },
        error: null
      })
    }
  };
  await withEnv({ MDX_DEPLOYMENT_MODE: "production", MDX_HOST_TRUSTED_IDENTITY_HEADERS: undefined }, async () => {
    const { session, accessToken } = await resolveHostSession(
      { request: new Request("http://localhost/") },
      supabase
    );
    assert.equal(session.status, "verified-supabase-session");
    assert.equal(session.tenant_id, "tenant_beta");
    assert.equal(session.actor_id, "user_7");
    assert.equal(session.subject_actor_id, "user_7");
    assert.equal(session.role, "operator");
    assert.equal(session.identity_source, "supabase_signed_jwt");
    assert.equal(accessToken, "signed-test-access-token");
    assert.equal("access_token" in session, false);
    assert.equal("refresh_token" in session, false);
  });
});

test("a verified native bearer session reaches the private kernel without cookies", async () => {
  let verifiedJWT = "";
  let cookieSessionRead = false;
  const supabase = {
    auth: {
      getClaims: async (jwt) => {
        verifiedJWT = jwt;
        return {
          data: {
            claims: {
              sub: "native_user",
              email: "native@example.com",
              mdx_tenant_id: "tenant_beta",
              mdx_role: "operator",
              mdx_actor_kind: "human"
            }
          },
          error: null
        };
      },
      getSession: async () => {
        cookieSessionRead = true;
        return { data: { session: null }, error: null };
      }
    }
  };
  await withEnv({ MDX_DEPLOYMENT_MODE: "production" }, async () => {
    const { session, accessToken } = await resolveHostSession(
      {
        request: new Request("https://mdx.test/api/kernel/version.json", {
          headers: { authorization: "Bearer native-signed-access-token" }
        })
      },
      supabase
    );
    assert.equal(verifiedJWT, "native-signed-access-token");
    assert.equal(session.status, "verified-supabase-session");
    assert.equal(accessToken, "native-signed-access-token");
    assert.equal(cookieSessionRead, false);
  });
});

test("an invalid native bearer token fails closed", async () => {
  const supabase = {
    auth: {
      getClaims: async () => ({ data: { claims: null }, error: new Error("invalid JWT") }),
      getSession: async () => {
        throw new Error("invalid bearer must not fall back to a cookie session");
      }
    }
  };
  await withEnv({ MDX_DEPLOYMENT_MODE: "production" }, async () => {
    const { session, accessToken } = await resolveHostSession(
      {
        request: new Request("https://mdx.test/api/kernel/version.json", {
          headers: { authorization: "Bearer invalid-token" }
        })
      },
      supabase
    );
    assert.equal(session.status, "supabase_session_not_verified");
    assert.equal(accessToken, null);
  });
});

test("an incomplete Supabase claim set never reaches the private kernel", async () => {
  let sessionRead = false;
  const supabase = {
    auth: {
      getClaims: async () => ({
        data: {
          claims: {
            sub: "user_7",
            mdx_tenant_id: "tenant_beta",
            mdx_actor_kind: "human"
          }
        },
        error: null
      }),
      getSession: async () => {
        sessionRead = true;
        return { data: { session: { access_token: "must-not-forward" } }, error: null };
      }
    }
  };
  await withEnv({ MDX_DEPLOYMENT_MODE: "production", MDX_HOST_TRUSTED_IDENTITY_HEADERS: undefined }, async () => {
    const { session, accessToken } = await resolveHostSession(
      { request: new Request("http://localhost/") },
      supabase
    );
    assert.equal(session.status, "supabase_session_not_verified");
    assert.equal(session.authenticated, false);
    assert.equal(accessToken, null);
    assert.equal(sessionRead, false);
  });
});

test("admin access requires both authentication and an operator role", () => {
  assert.equal(adminAccess({ authenticated: true, role: "admin" }).allowed, true);
  assert.equal(adminAccess({ authenticated: true, role: "owner" }).allowed, true);
  assert.equal(adminAccess({ authenticated: true, role: "operator" }).allowed, true);
  assert.equal(adminAccess({ authenticated: true, role: "viewer" }).allowed, false);
  assert.equal(adminAccess({ authenticated: false, role: "operator" }).allowed, false);
});

test("the OAuth callback preserves a safe same-origin destination", () => {
  assert.equal(safeReturnPath("/forge?view=mine"), "/forge?view=mine");
});

test("the OAuth callback refuses scheme-relative and backslash destinations", () => {
  for (const next of ["//attacker.example", "/\\attacker.example"]) {
    assert.equal(safeReturnPath(next), "/");
  }
});

test("the OAuth entry accepts Apple without widening unknown providers", () => {
  assert.equal(resolveOAuthProvider("apple"), "apple");
  assert.equal(resolveOAuthProvider("azure"), "azure");
  assert.equal(resolveOAuthProvider("untrusted"), "google");
  assert.equal(resolveOAuthProvider(null, "apple"), "apple");
});

test("Google sign-in asks people to choose the invited account", () => {
  assert.deepEqual(oauthProviderQueryParams("google"), { prompt: "select_account" });
  assert.equal(oauthProviderQueryParams("apple"), undefined);
  assert.equal(oauthProviderQueryParams("azure"), undefined);
});

test("identity-provider returns never leak one-time URL values through referrers or caches", async () => {
  await withEnv(
    { MDX_DEPLOYMENT_MODE: "production", MDX_EXPERIENCE_PROFILE: "public-beta" },
    async () => {
      for (const path of [
        "/auth/callback?code=one-time-code",
        "/forge/connect/github?installation_id=42&state=one-time-state"
      ]) {
        const event = {
          request: new Request(`https://mdx.test${path}`, { headers: { accept: "text/html" } }),
          url: new URL(`https://mdx.test${path}`),
          locals: {}
        };
        const response = await handle({
          event,
          resolve: async () =>
            new Response(null, { status: 303, headers: { location: "/welcome/pending" } })
        });
        assert.equal(response.headers.get("referrer-policy"), "no-referrer");
        assert.equal(response.headers.get("cache-control"), "private, no-store");
      }
    }
  );
});

test("the production proxy forwards only the verified cookie token", async () => {
  await withEnv({ MDX_DEPLOYMENT_MODE: "production" }, async () => {
    const headers = applyKernelIdentityHeaders(
      {},
      new Request("https://mdx.test/api/kernel/messages/thread-messages.json", {
        headers: {
          authorization: "Bearer attacker-controlled-token",
          "x-mdx-actor-admission": "attacker-controlled-admission"
        }
      }),
      { kernelAccessToken: "verified-cookie-token" }
    );
    assert.equal(headers.Authorization, "Bearer verified-cookie-token");
    assert.equal("X-MDX-Actor-Admission" in headers, false);
  });
});

test("unknown live modes strip client identity and refuse private data requests", async () => {
  await withEnv(
    {
      MDX_DEPLOYMENT_MODE: "staging",
      MDX_SUPABASE_URL: undefined,
      MDX_SUPABASE_PUBLISHABLE_KEY: undefined,
      MDX_SUPABASE_ANON_KEY: undefined
    },
    async () => {
      const headers = applyKernelIdentityHeaders(
        {},
        new Request("https://mdx.test/api/kernel/twin/moving", {
          headers: {
            authorization: "Bearer attacker-controlled-token",
            "x-mdx-actor-admission": "attacker-controlled-admission"
          }
        }),
        {}
      );
      assert.deepEqual(headers, {});

      let routeRan = false;
      const event = {
        request: new Request("https://mdx.test/twin/moving", {
          method: "POST",
          headers: { accept: "application/json" }
        }),
        url: new URL("https://mdx.test/twin/moving"),
        locals: {}
      };
      const response = await handle({
        event,
        resolve: async () => {
          routeRan = true;
          return new Response("must not run");
        }
      });
      assert.equal(response.status, 401);
      assert.equal(routeRan, false);
      assert.equal(response.headers.get("cache-control"), "private, no-store");
    }
  );
});

test("local-secure forwards only its signed bearer token", async () => {
  await withEnv({ MDX_DEPLOYMENT_MODE: "local-secure" }, async () => {
    const headers = applyKernelIdentityHeaders(
      {},
      new Request("http://localhost/api/kernel/twin/moving", {
        headers: {
          authorization: "Bearer signed-local-secure-token",
          "x-mdx-actor-admission": "untrusted-admission"
        }
      }),
      {}
    );
    assert.equal(headers.Authorization, "Bearer signed-local-secure-token");
    assert.equal("X-MDX-Actor-Admission" in headers, false);
  });
});

test("the public beta leaves the funnel open and redirects private HTML", async () => {
  await withEnv(
    { MDX_DEPLOYMENT_MODE: "production", MDX_EXPERIENCE_PROFILE: "public-beta" },
    async () => {
      // A stranger's funnel: marketing, waitlist, redemption, and the
      // pending room all render without a session.
      for (const path of ["/landing", "/forge-product", "/open-source", "/downloads", "/security", "/waitlist", "/redeem", "/welcome/pending"]) {
        const publicEvent = {
          request: new Request(`https://mdx.test${path}`, { headers: { accept: "text/html" } }),
          url: new URL(`https://mdx.test${path}`),
          locals: {}
        };
        const publicResponse = await handle({
          event: publicEvent,
          resolve: async () =>
            new Response("public funnel page", { headers: { "content-type": "text/html" } })
        });
        assert.equal(publicResponse.status, 200, `${path} must stay public in the beta`);
      }

      const publicDataEvent = {
        request: new Request("https://mdx.test/security/__data.json", {
          headers: { accept: "application/json" }
        }),
        url: new URL("https://mdx.test/security/__data.json"),
        locals: {}
      };
      const publicDataResponse = await handle({
        event: publicDataEvent,
        resolve: async () =>
          new Response("public trust data", { headers: { "content-type": "application/json" } })
      });
      assert.equal(publicDataResponse.status, 200);

      const privateEvent = {
        request: new Request("https://mdx.test/twin", { headers: { accept: "text/html" } }),
        url: new URL("https://mdx.test/twin"),
        locals: {}
      };
      const privateResponse = await handle({
        event: privateEvent,
        resolve: async () => new Response("must not render")
      });
      assert.equal(privateResponse.status, 303);
      assert.equal(privateResponse.headers.get("location"), "/auth/sign-in?next=%2Ftwin");
    }
  );
});

test("an enterprise deployment exposes no public funnel at all", async () => {
  await withEnv({ MDX_DEPLOYMENT_MODE: "production" }, async () => {
    // Without the public-beta profile, production is an enterprise
    // deployment: marketing, waitlist, and redemption must not leak.
    for (const path of ["/security", "/open-source", "/downloads", "/waitlist", "/redeem", "/landing", "/welcome/pending"]) {
      const event = {
        request: new Request(`https://mdx.test${path}`, { headers: { accept: "text/html" } }),
        url: new URL(`https://mdx.test${path}`),
        locals: {}
      };
      const response = await handle({
        event,
        resolve: async () => new Response("must not render")
      });
      assert.equal(response.status, 303, `${path} must require sign-in in enterprise`);
      assert.equal(
        response.headers.get("location"),
        `/auth/sign-in?next=${encodeURIComponent(path)}`
      );
    }
  });
});

test("the hosted beta can expose a public root that redirects to the landing page", async () => {
  await withEnv(
    { MDX_DEPLOYMENT_MODE: "production", MDX_PUBLIC_ROOT_MODE: "landing" },
    async () => {
      let routeRan = false;
      const event = {
        request: new Request("https://mdx.test/", { headers: { accept: "text/html" } }),
        url: new URL("https://mdx.test/"),
        locals: {}
      };
      const response = await handle({
        event,
        resolve: async () => {
          routeRan = true;
          return new Response("root route");
        }
      });
      assert.equal(response.status, 200);
      assert.equal(routeRan, true);

      assert.equal(publicRootEnabled(), true);
      assert.equal(publicRootRedirect(), "/landing");
    }
  );
});

test("the public root mode refuses partial values", async () => {
  await withEnv({ MDX_PUBLIC_ROOT_MODE: "landing-preview" }, async () => {
    assert.equal(publicRootEnabled(), false);
    assert.equal(publicRootRedirect(), null);
  });
});
