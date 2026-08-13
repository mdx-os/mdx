const RESEND_ENDPOINT = "https://api.resend.com/emails";

function requiredEnv(name, env = process.env) {
  return String(env[name] ?? "").trim();
}

export function inviteDeliveryReadiness(env = process.env) {
  const provider = requiredEnv("MDX_BETA_INVITE_EMAIL_PROVIDER", env).toLowerCase();
  const apiKey = requiredEnv("MDX_RESEND_API_KEY", env);
  const from = requiredEnv("MDX_BETA_INVITE_FROM", env);
  return {
    provider,
    ready: provider === "resend" && Boolean(apiKey) && Boolean(from),
    missing: [
      !provider && "MDX_BETA_INVITE_EMAIL_PROVIDER",
      !apiKey && "MDX_RESEND_API_KEY",
      !from && "MDX_BETA_INVITE_FROM"
    ].filter(Boolean)
  };
}

export function inviteMessage({ fullName, email, inviteId, origin }) {
  const firstName = String(fullName ?? "").trim().split(/\s+/)[0] || "there";
  const redeemUrl = new URL("/redeem", origin);
  redeemUrl.searchParams.set("invite", inviteId);
  return {
    to: String(email ?? "").trim().toLowerCase(),
    subject: "Your MDx beta invite",
    text: [
      `Hi ${firstName},`,
      "",
      "You're in. Redeem your MDx beta invite here:",
      redeemUrl.href,
      "",
      `(Your invite id, if you need it: ${inviteId})`,
      "",
      "On your first sign-in, choose Google or Apple for this invited email address. Use that same MDx workspace on web, Mac, and iPhone.",
      "",
      "If MDx says your workspace is still being activated, keep that page open. We will reply to this thread when it is ready; then choose Check access there, or open this redemption link again.",
      "",
      "After activation, MDx will guide you through the notarized Mac download and the TestFlight iPhone handoff. If Apple offers Hide My Email with a different address, sign in with the invited address first and connect Apple later from You.",
      "",
      "Reply to this email if anything feels unclear or gets stuck.",
      "",
      "- MD"
    ].join("\n")
  };
}

export async function deliverInvite(message, inviteId, fetchImpl = fetch, env = process.env) {
  const readiness = inviteDeliveryReadiness(env);
  if (!readiness.ready) {
    return { ok: false, provider: readiness.provider || "unconfigured", id: "", reason: "invite email provider is not configured" };
  }
  if (readiness.provider !== "resend") {
    return { ok: false, provider: readiness.provider, id: "", reason: "unsupported invite email provider" };
  }
  const response = await fetchImpl(RESEND_ENDPOINT, {
    method: "POST",
    headers: {
      authorization: `Bearer ${requiredEnv("MDX_RESEND_API_KEY", env)}`,
      "content-type": "application/json",
      "idempotency-key": `mdx-beta-invite/${inviteId}`
    },
    body: JSON.stringify({
      from: requiredEnv("MDX_BETA_INVITE_FROM", env),
      to: [message.to],
      subject: message.subject,
      text: message.text,
      reply_to: requiredEnv("MDX_BETA_INVITE_REPLY_TO", env) || undefined
    }),
    signal: AbortSignal.timeout(10000)
  });
  const packet = await response.json().catch(() => ({}));
  if (!response.ok || !packet?.id) {
    return {
      ok: false,
      provider: "resend",
      id: "",
      reason: String(packet?.message ?? `email provider returned ${response.status}`)
    };
  }
  return { ok: true, provider: "resend", id: String(packet.id), reason: "" };
}
