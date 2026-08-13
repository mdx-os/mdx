// Same-origin kernel proxy. The browser never talks to the kernel directly:
// no CORS surface, host security headers apply to every byte, and the
// kernel's base URL stays a server-side variable (MDX_LOCAL_API_URL), never
// baked into a client bundle.
//
// This is NOT an open relay: only routes declared in the generated catalog
// forward, with the declared method. Anything else answers 404 here.
import { routeCatalog } from "../../../../lib/platform.js";
import { timeoutForPath } from "../../../../lib/kernelProxyTimeout.js";
import { applyKernelIdentityHeaders } from "../../../../lib/session.server.js";

const kernelBase = process.env.MDX_LOCAL_API_URL ?? "http://127.0.0.1:18787";

// Most kernel calls are fast reads and must fail visibly if the kernel hangs.
// A few endpoints do inline model reasoning before they answer - the fleet
// planner, the work classifier, and mission shaping all call a reasoning model
// synchronously - and legitimately take far longer than a data read. Killing
// those at 15s produced a "kernel timeout" that the UI then mislabeled (a slow
// fleet plan looked like "try a smaller width"). Give the reasoning endpoints a
// generous budget; keep everything else snappy.
// A real SSE stream is long-lived and must skip the abort timeout. Detect it
// by the path segment, not a loose substring: the old path.includes("/stream")
// also matched /twin/stream-telemetry.json (a plain JSON read), which then
// escaped the timeout and could hang a waiting page forever. A stream is a
// segment that is exactly "stream" or ends "-stream" (session-stream); a
// ".json" read never is.
function isStreamingPath(path) {
  const bare = (path.split("?")[0] ?? "").toLowerCase();
  if (bare.endsWith(".json")) return false;
  const last = bare.split("/").pop() ?? "";
  return last === "stream" || last.endsWith("-stream");
}

function declaredPath(params, url, method) {
  const path = `/${params.path}${url.search ?? ""}`;
  const bare = `/${params.path}`;
  // Pattern routes (e.g. /pages/{document_id}/body) are declared with
  // placeholders; exact match first, then a placeholder-aware pass.
  if (routeCatalog.get(method, bare)) {
    return path;
  }
  const segments = bare.split("/");
  const match = routeCatalog.routes.find((route) => {
    if (route.method !== method) return false;
    const declared = route.local_path.split("/");
    if (declared.length !== segments.length) return false;
    return declared.every((part, index) => part.startsWith("{") || part === segments[index]);
  });
  return match ? path : null;
}

async function forward(method, { params, url, request, locals }) {
  const path = declaredPath(params, url, method);
  if (!path) {
    return new Response(JSON.stringify({ error: "route not declared in the generated catalog" }), {
      status: 404,
      headers: { "content-type": "application/json" }
    });
  }
  const headers = { Accept: "application/json" };
  if (request) {
    const contentType = request.headers.get("content-type");
    if (contentType) headers["Content-Type"] = contentType;
    applyKernelIdentityHeaders(headers, request, locals);
  }
  let response;
  try {
    // A slow kernel must fail visibly, not hang every waiting page. Streams
    // (SSE) are exempt: they are long-lived by design.
    const streaming = isStreamingPath(path);
    response = await fetch(`${kernelBase}${path}`, {
      method,
      headers,
      body: method === "POST" && request ? await request.text() : undefined,
      signal: streaming ? undefined : AbortSignal.timeout(timeoutForPath(path))
    });
  } catch (error) {
    const timedOut = error?.name === "TimeoutError" || error?.name === "AbortError";
    return new Response(
      JSON.stringify({ error: timedOut ? "kernel timeout" : "kernel unavailable" }),
      {
        status: timedOut ? 504 : 503,
        headers: { "content-type": "application/json" }
      }
    );
  }
  return new Response(response.body, {
    status: response.status,
    headers: { "content-type": response.headers.get("content-type") ?? "application/json" }
  });
}

export function GET(event) {
  return forward("GET", event);
}

export function POST(event) {
  return forward("POST", event);
}
