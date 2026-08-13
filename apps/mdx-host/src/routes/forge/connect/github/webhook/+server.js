const MAX_WEBHOOK_BYTES = 1024 * 1024;

function json(status, error) {
  return new Response(JSON.stringify({ error }), {
    status,
    headers: { "content-type": "application/json", "cache-control": "no-store" }
  });
}

export async function POST({ request, fetch }) {
  const contentLength = Number(request.headers.get("content-length") ?? 0);
  if (contentLength > MAX_WEBHOOK_BYTES) return json(413, "github webhook payload too large");

  const signature = request.headers.get("x-hub-signature-256") ?? "";
  const delivery = request.headers.get("x-github-delivery") ?? "";
  const event = request.headers.get("x-github-event") ?? "";
  if (!signature.startsWith("sha256=") || !delivery || event !== "installation") {
    return json(401, "github webhook headers invalid");
  }

  const body = new Uint8Array(await request.arrayBuffer());
  if (body.byteLength > MAX_WEBHOOK_BYTES) return json(413, "github webhook payload too large");

  const kernelBase = process.env.MDX_LOCAL_API_URL ?? "http://127.0.0.1:18787";
  try {
    const response = await fetch(`${kernelBase}/mobile/cloud/github-webhooks.json`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "x-hub-signature-256": signature,
        "x-github-delivery": delivery,
        "x-github-event": event
      },
      body,
      signal: AbortSignal.timeout(15_000)
    });
    return new Response(response.body, {
      status: response.status,
      headers: {
        "content-type": response.headers.get("content-type") ?? "application/json",
        "cache-control": "no-store"
      }
    });
  } catch {
    return json(503, "github webhook unavailable");
  }
}
