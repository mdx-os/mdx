export const OPERATOR_ROLES = ["owner", "operator", "admin"];

export function normalizeRole(role) {
  return String(role ?? "viewer").trim().toLowerCase() || "viewer";
}

export function isOperatorRole(role) {
  return OPERATOR_ROLES.includes(normalizeRole(role));
}

export function isOperatorSession(session) {
  return Boolean(session?.authenticated) && isOperatorRole(session?.role);
}

export function sessionPresentation(session) {
  const role = normalizeRole(session?.role);
  const authenticated = session?.authenticated === true;
  const source = String(session?.identity_source ?? "none");

  let trust = "No verified sign-in";
  if (authenticated && source === "supabase_signed_jwt") {
    trust = "Verified sign-in";
  } else if (authenticated && source === "trusted_identity_headers") {
    trust = "Verified host session";
  } else if (authenticated && source === "local_demo_fixture") {
    trust = "Local demo seat";
  } else if (authenticated) {
    trust = "Authenticated session";
  }

  return {
    label: "You",
    avatar: "Y",
    title: "Your profile and settings",
    trust: `${trust} · ${role} seat`
  };
}
