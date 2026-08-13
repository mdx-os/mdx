export function publicRootEnabled() {
  return String(process.env.MDX_PUBLIC_ROOT_MODE ?? "").trim().toLowerCase() === "landing";
}

export function publicRootRedirect() {
  return publicRootEnabled() ? "/landing" : null;
}
