// Content-Security-Policy itself is kit-managed (svelte.config.js csp,
// hash mode) so hydration scripts carry hashes; these are the remaining
// host headers, plus a CSP fallback if the framework ever stops emitting
// one - this host never ships without a Content-Security-Policy header.
import { resolveHostSession, verifiedHostSessionRequired } from "./lib/session.server.js";
import { publicRootEnabled } from "./lib/publicRoot.server.js";
import { publicFunnelEnabled } from "./lib/experienceProfile.server.js";
import { isPublicExperiencePath } from "./lib/publicExperience.js";
import { brotliCompressSync, gzipSync, constants as zlibConstants } from "node:zlib";

const fallbackCsp = "default-src 'self'; frame-ancestors 'none'; base-uri 'self'";
const securityHeaders = {
  "Cross-Origin-Opener-Policy": "same-origin",
  "Permissions-Policy": "camera=(), microphone=(), geolocation=(), payment=()",
  "Referrer-Policy": "strict-origin-when-cross-origin",
  "X-Content-Type-Options": "nosniff",
  "X-Frame-Options": "DENY"
};
// The public funnel exists only in the public-beta experience profile: a
// stranger can see the landing and marketing pages, join the waitlist,
// redeem an invite, and wait in the pending room while their access is
// activated. Local machines skip the funnel entirely; an enterprise
// deployment must never expose it.
const publicStaticRoutes = new Set([
  "/apple-touch-icon.png",
  "/icon-192.png",
  "/icon-512.png",
  "/manifest.webmanifest"
]);

function isPublicRequest(pathname) {
  if (publicRootEnabled() && (pathname === "/" || pathname === "/__data.json")) return true;
  if (pathname.startsWith("/auth/") || pathname.startsWith("/_app/")) return true;
  if (publicStaticRoutes.has(pathname)) return true;
  if (!publicFunnelEnabled()) return false;
  return isPublicExperiencePath(pathname);
}

// Dynamic responses (SSR HTML, __data.json, proxied kernel JSON) are the
// heavy payloads here - a run projection page measured 1.4MB uncompressed.
// Compress everything compressible; never touch streams (SSE) or bodies that
// are already encoded. Brotli at quality 4 is the latency-safe setting for
// per-request dynamic compression.
const COMPRESSIBLE = /^(text\/(?!event-stream)|application\/(json|javascript|xml)|image\/svg)/;
const MIN_COMPRESS_BYTES = 1024;
const sensitiveReturnPaths = new Set(["/auth/callback", "/forge/connect/github"]);

async function compress(event, response) {
  const contentType = response.headers.get("content-type") ?? "";
  if (
    !response.body ||
    event.request.method === "HEAD" ||
    response.headers.has("content-encoding") ||
    !COMPRESSIBLE.test(contentType)
  ) {
    return response;
  }
  const acceptEncoding = event.request.headers.get("accept-encoding") ?? "";
  const useBrotli = acceptEncoding.includes("br");
  if (!useBrotli && !acceptEncoding.includes("gzip")) return response;

  const raw = Buffer.from(await response.arrayBuffer());
  if (raw.byteLength < MIN_COMPRESS_BYTES) {
    return new Response(raw, { status: response.status, headers: response.headers });
  }
  const encoded = useBrotli
    ? brotliCompressSync(raw, {
        params: { [zlibConstants.BROTLI_PARAM_QUALITY]: 4 }
      })
    : gzipSync(raw);
  const headers = new Headers(response.headers);
  headers.set("Content-Encoding", useBrotli ? "br" : "gzip");
  headers.delete("Content-Length");
  headers.append("Vary", "Accept-Encoding");
  return new Response(encoded, { status: response.status, headers });
}

export async function handle({ event, resolve }) {
  const resolvedSession = await resolveHostSession(event);
  event.locals.session = resolvedSession.session;
  event.locals.supabase = resolvedSession.supabase;
  event.locals.kernelAccessToken = resolvedSession.accessToken;
  const wantsHtml = (event.request.headers.get("accept") ?? "").includes("text/html");
  const privateRequest = !isPublicRequest(event.url.pathname);
  let response;
  if (verifiedHostSessionRequired() && privateRequest && !event.locals.session.authenticated) {
    if (wantsHtml && event.request.method === "GET") {
      response = new Response(null, {
        status: 303,
        headers: {
          location: `/auth/sign-in?next=${encodeURIComponent(event.url.pathname + event.url.search)}`,
          "cache-control": "private, no-store"
        }
      });
    } else {
      response = new Response(JSON.stringify({ error: "verified host session required" }), {
        status: 401,
        headers: {
          "content-type": "application/json",
          "cache-control": "private, no-store"
        }
      });
    }
  } else {
    response = await resolve(event);
  }
  for (const [header, value] of Object.entries(securityHeaders)) {
    response.headers.set(header, value);
  }
  // OAuth authorization codes and GitHub installation state arrive in the
  // return URL. Keep those one-time values out of same-origin Referer headers,
  // browser caches, and intermediary caches before the route redirects.
  if (sensitiveReturnPaths.has(event.url.pathname)) {
    response.headers.set("Referrer-Policy", "no-referrer");
    response.headers.set("Cache-Control", "private, no-store");
  }
  if (!response.headers.has("Content-Security-Policy")) {
    response.headers.set("Content-Security-Policy", fallbackCsp);
  }
  return compress(event, response);
}
