import test from "node:test";
import assert from "node:assert/strict";
import {
  archivePathForMacRelease,
  diskImagePathForMacRelease,
  dmgReadyMacReleaseManifest,
  macAppcastXML,
  sparkleReadyMacReleaseManifest,
  validMacReleaseManifest
} from "../src/lib/server/macosRelease.server.js";
import { load } from "../src/routes/download/macos/+page.server.js";
import { GET as getManifest } from "../src/routes/download/macos/manifest.json/+server.js";
import { GET as getAppcast } from "../src/routes/download/macos/appcast.xml/+server.js";
import { GET as getInstaller } from "../src/routes/download/macos/installer.dmg/+server.js";
import { GET as getUpdate } from "../src/routes/download/macos/update.zip/+server.js";

const sparkleSignature = `${"A".repeat(86)}==`;
const dmgMetadata = {
  dmg_sha256: "d".repeat(64),
  dmg_size_bytes: 45_000_000,
  dmg_notarized_at: "2026-08-14T19:00:00Z"
};

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

test("the Mac release accepts a valid Sparkle EdDSA signature without requiring it for browser downloads", () => {
  const manifest = validMacReleaseManifest({
    version: "0.9.3",
    build: "5",
    sha256: "c".repeat(64),
    size_bytes: 44_000_000,
    sparkle_ed_signature: sparkleSignature,
    notarized_at: "2026-08-13T18:00:00Z"
  });
  assert.equal(manifest.sparkle_ed_signature, sparkleSignature);
  assert.deepEqual(
    sparkleReadyMacReleaseManifest({ ...manifest, sparkle_ed_signature: "not-a-signature" }),
    null
  );
});

test("the Mac release accepts only complete notarized DMG metadata", () => {
  const manifest = validMacReleaseManifest({
    version: "0.9.3",
    build: "7",
    sha256: "c".repeat(64),
    size_bytes: 44_000_000,
    notarized_at: "2026-08-14T18:00:00Z",
    ...dmgMetadata
  });
  assert.deepEqual(dmgReadyMacReleaseManifest(manifest), manifest);
  assert.equal(dmgReadyMacReleaseManifest({ ...manifest, dmg_sha256: "short" }), null);
  assert.equal(validMacReleaseManifest({ ...manifest, dmg_size_bytes: 0 }), null);
  assert.equal(validMacReleaseManifest({ ...manifest, dmg_notarized_at: "pending" }), null);
  assert.equal(validMacReleaseManifest({
    version: "0.9.3",
    build: "7",
    sha256: "c".repeat(64),
    size_bytes: 44_000_000,
    notarized_at: "2026-08-14T18:00:00Z",
    dmg_size_bytes: "not-a-size"
  }), null);
});

test("the authenticated appcast points Sparkle at the same-origin update gate", () => {
  const xml = macAppcastXML({
    version: "0.9.3",
    build: "5",
    sha256: "c".repeat(64),
    size_bytes: 44_000_000,
    notarized_at: "2026-08-13T18:00:00Z",
    sparkle_ed_signature: sparkleSignature
  }, "https://mdx-os.com");
  assert.match(xml, /url="https:\/\/mdx-os\.com\/download\/macos\/update\.zip"/);
  assert.match(xml, /sparkle:version="5"/);
  assert.match(xml, /sparkle:shortVersionString="0\.9\.3"/);
  assert.match(xml, new RegExp(`sparkle:edSignature="${sparkleSignature}"`));
  assert.match(xml, /sparkle:minimumSystemVersion>14\.0</);
});

test("the Mac download resolves the immutable archive named by the manifest", () => {
  assert.equal(
    archivePathForMacRelease({ version: "0.9.1", build: "1401" }),
    "releases/macos/canary/0.9.1/1401/MDx.zip"
  );
  assert.equal(
    diskImagePathForMacRelease({ version: "0.9.3", build: "7" }),
    "releases/macos/canary/0.9.3/7/MDx.dmg"
  );
});

test("the Mac download fails closed on a malformed manifest", () => {
  assert.equal(validMacReleaseManifest({ version: "0.9.1", build: "1401", sha256: "short", size_bytes: 1 }), null);
  assert.equal(validMacReleaseManifest({ version: "0.9.1", build: "1401", sha256: "a".repeat(64), size_bytes: 0 }), null);
  assert.equal(validMacReleaseManifest({ version: "../private", build: "1401", sha256: "a".repeat(64), size_bytes: 1 }), null);
  assert.equal(validMacReleaseManifest({ version: "0.9.1", build: "../1401", sha256: "a".repeat(64), size_bytes: 1 }), null);
});

test("the Mac download page defers archive signing until the authenticated download request", async () => {
  const signed = [];
  const manifest = {
    version: "0.9.3",
    build: "5",
    sha256: "c".repeat(64),
    size_bytes: 44_000_000,
    notarized_at: "2026-08-13T18:00:00Z",
    sparkle_ed_signature: sparkleSignature
  };
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
    fetch: async () => Response.json(manifest)
  });
  assert.equal(result.available, true);
  assert.equal(result.downloadUrl, "/download/macos/update.zip");
  assert.equal(result.downloadFilename, "MDx.zip");
  assert.match(result.macSteps[0], /ZIP/);
  assert.deepEqual(signed, ["latest/manifest.json"]);

  const response = await getUpdate({
    locals,
    fetch: async (url) => String(url).includes("latest/manifest.json")
      ? Response.json(manifest)
      : new Response("archive-bytes", { headers: { "content-type": "application/zip" } })
  });
  assert.equal(response.status, 200);
  assert.equal(response.headers.get("content-disposition"), 'attachment; filename="MDx.zip"');
  assert.equal(await response.text(), "archive-bytes");
  assert.deepEqual(signed, [
    "latest/manifest.json",
    "latest/manifest.json",
    "releases/macos/canary/0.9.3/5/MDx.zip"
  ]);
});

test("the Mac download page switches to the DMG only after complete installer metadata is published", async () => {
  const signed = [];
  const manifest = {
    version: "0.9.3",
    build: "7",
    sha256: "c".repeat(64),
    size_bytes: 44_000_000,
    notarized_at: "2026-08-14T18:00:00Z",
    sparkle_ed_signature: sparkleSignature,
    ...dmgMetadata
  };
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
    fetch: async () => Response.json(manifest)
  });
  assert.equal(result.downloadUrl, "/download/macos/installer.dmg");
  assert.equal(result.downloadFilename, "MDx.dmg");
  assert.match(result.macSteps[0], /MDx\.dmg/);
  assert.match(result.macSteps[1], /Drag MDx/);
  assert.deepEqual(signed, ["latest/manifest.json"]);
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

test("the private Sparkle appcast requires an authenticated beta session", async () => {
  const response = await getAppcast({
    locals: {},
    fetch: async () => Response.json({}),
    url: new URL("https://mdx-os.com/download/macos/appcast.xml")
  });
  assert.equal(response.status, 401);
});

test("the private Sparkle archive requires an authenticated beta session", async () => {
  const response = await getUpdate({
    locals: {},
    fetch: async () => {
      throw new Error("must not fetch without an authenticated beta session");
    }
  });
  assert.equal(response.status, 401);
});

test("the private DMG installer requires an authenticated beta session", async () => {
  const response = await getInstaller({
    locals: {},
    fetch: async () => {
      throw new Error("must not fetch without an authenticated beta session");
    }
  });
  assert.equal(response.status, 401);
});

test("the private Sparkle appcast serves the signed latest release", async () => {
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
  const response = await getAppcast({
    locals,
    fetch: async () => Response.json({
      version: "0.9.3",
      build: "5",
      sha256: "c".repeat(64),
      size_bytes: 44_000_000,
      notarized_at: "2026-08-13T18:00:00Z",
      sparkle_ed_signature: sparkleSignature
    }),
    url: new URL("https://mdx-os.com/download/macos/appcast.xml")
  });
  assert.equal(response.status, 200);
  assert.match(response.headers.get("content-type"), /application\/rss\+xml/);
  assert.match(await response.text(), /sparkle:edSignature/);
});

test("the Sparkle update gate streams the exact manifest-pinned archive without leaking the beta bearer", async () => {
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
  const response = await getUpdate({
    locals,
    fetch: async (url, options = {}) => {
      if (String(url).includes("latest/manifest.json")) {
        return Response.json({
        version: "0.9.3",
        build: "5",
        sha256: "c".repeat(64),
        size_bytes: 44_000_000,
        notarized_at: "2026-08-13T18:00:00Z",
        sparkle_ed_signature: sparkleSignature
        });
      }
      assert.equal(options.headers.authorization, undefined);
      return new Response("archive-bytes", { headers: { "content-type": "application/zip" } });
    }
  });
  assert.equal(response.status, 200);
  assert.equal(response.headers.get("content-type"), "application/zip");
  assert.equal(await response.text(), "archive-bytes");
  assert.deepEqual(signed, [
    "latest/manifest.json",
    "releases/macos/canary/0.9.3/5/MDx.zip"
  ]);
});

test("the DMG gate streams the exact notarized installer without leaking the beta bearer", async () => {
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
  const response = await getInstaller({
    locals,
    fetch: async (url, options = {}) => {
      if (String(url).includes("latest/manifest.json")) {
        return Response.json({
          version: "0.9.3",
          build: "7",
          sha256: "c".repeat(64),
          size_bytes: 44_000_000,
          notarized_at: "2026-08-14T18:00:00Z",
          sparkle_ed_signature: sparkleSignature,
          ...dmgMetadata
        });
      }
      assert.equal(options.headers.authorization, undefined);
      assert.equal(options.headers.accept, "application/x-apple-diskimage");
      return new Response("dmg-bytes", {
        headers: { "content-type": "application/x-apple-diskimage" }
      });
    }
  });
  assert.equal(response.status, 200);
  assert.equal(response.headers.get("content-type"), "application/x-apple-diskimage");
  assert.equal(response.headers.get("content-disposition"), 'attachment; filename="MDx.dmg"');
  assert.equal(await response.text(), "dmg-bytes");
  assert.deepEqual(signed, [
    "latest/manifest.json",
    "releases/macos/canary/0.9.3/7/MDx.dmg"
  ]);
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
