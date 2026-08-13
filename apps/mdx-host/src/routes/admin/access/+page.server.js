import { adminAccess } from "../../../lib/adminAccess.js";

export function load() {
  return {
    access: adminAccess(),
    boundary:
      "access is read-only here; roles, invites, sessions, and policy are shown from generated contracts only",
    safeNext:
      "wire the unified Gate only after every access mutation has session authority, policy, and a receipt"
  };
}
