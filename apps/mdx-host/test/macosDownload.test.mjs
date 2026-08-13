import test from "node:test";
import assert from "node:assert/strict";
import {
  archivePathForMacRelease,
  validMacReleaseManifest
} from "../src/lib/server/macosRelease.server.js";
import { load } from "../src/routes/download/macos/+page.server.js";
import { GET as getManifest } from "../src/routes/download/macos/manifest.json/+server.js";

test("the Mac download accepts a complete notarized release manifest", () => {
  assert.deepEqual(
    validMacReleaseManifest({
      version: "0.9.1",
      build: "1401",
      sha256: "a".repeat(64),
      size_bytes: 42_000_000,
      notarized_at: "2026-07-20T20:00:00Z"
    }),
    {
      version: "0.9.1",
      build: "1401",
      sha256: "a".repeat(64),
      size_bytes: 42_000_000,
      notarized_at: "2026-07-20T20:00:00Z"
    }
  );
});

test("the Mac download resolves the immutable archive named by the manifest", () => {
  assert.equal(
    archivePathForMacRelease({ version: "0.9.1", build: "1401" }),
    "releases/macos/canary/0.9.1/1401/MDx.zip"
  );
});

test("the Mac download fails closed on a malformed manifest", () => {
  assert.equal(validMacReleaseManifest({ version: "0.9.1", build: "1401", sha256: "short", size_bytes: 1 }), null);
  assert.equal(validMacReleaseManifest({ version: "0.9.1", build: "1401", sha256: "a".repeat(64), size_bytes: 0 }), null);
  assert.equal(validMacReleaseManifest({ version: "../private", build: "1401", sha256: "a".repeat(64), size_bytes: 1 }), null);
  assert.equal(validMacReleaseManifest({ version: "0.9.1", build: "../1401", sha256: "a".repeat(64), size_bytes: 1 }), null);
});

test("the Mac download signs the manifest-pinned archive after validating the manifest", async () => {
  const signed = [];
  const locals = {
    session: { authenticated: true },
    supabase: {
      storage: {
        from: () => ({
          createSignedUrl: async (path) => {
            signed.push(path);
            return { data: { signedUrl: `https://storage.example/${path}` }, error: null };
          }
        })
      }
    }
  };
  const result = await load({
    locals,
    setHeaders: () => {},
    fetch: async () =>
      new Response(
        JSON.stringify({
          version: "0.9.1",
          build: "1401",
          sha256: "a".repeat(64),
          size_bytes: 42_000_000
        }),
        { status: 200, headers: { "content-type": "application/json" } }
      )
  });
  assert.equal(result.available, true);
  assert.deepEqual(signed, [
    "latest/manifest.json",
    "releases/macos/canary/0.9.1/1401/MDx.zip"
  ]);
});

test("the private Mac manifest endpoint returns only validated release metadata", async () => {
  const locals = {
    session: { authenticated: true },
    supabase: {
      storage: {
        from: () => ({
          createSignedUrl: async () => ({
            data: { signedUrl: "https://storage.example/latest/manifest.json" },
            error: null
          })
        })
      }
    }
  };
  const response = await getManifest({
    locals,
    fetch: async () => Response.json({
      version: "0.9.2",
      build: "2",
      sha256: "b".repeat(64),
      size_bytes: 43_000_000,
      notarized_at: "2026-07-20T21:00:00Z"
    })
  });
  assert.equal(response.status, 200);
  assert.match(response.headers.get("cache-control"), /private/);
  assert.deepEqual(await response.json(), {
    available: true,
    manifest: {
      version: "0.9.2",
      build: "2",
      sha256: "b".repeat(64),
      size_bytes: 43_000_000,
      notarized_at: "2026-07-20T21:00:00Z"
    }
  });
});

test("the private Mac manifest endpoint requires an authenticated beta session", async () => {
  const response = await getManifest({ locals: {}, fetch: async () => Response.json({}) });
  assert.equal(response.status, 401);
});

test("an unavailable manifest is an honest empty update check, not an app error", async () => {
  const response = await getManifest({
    locals: {
      session: { authenticated: true },
      supabase: {
        storage: {
          from: () => ({
            createSignedUrl: async () => ({ data: null, error: new Error("missing") })
          })
        }
      }
    },
    fetch: async () => {
      throw new Error("must not fetch without a signed manifest URL");
    }
  });
  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), { available: false, manifest: null });
});
