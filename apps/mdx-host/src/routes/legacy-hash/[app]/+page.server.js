import { redirect } from "@sveltejs/kit";

const known = new Set(["home", "twin", "forge"]);

export function load({ params }) {
  const app = known.has(params.app) ? params.app : "home";
  throw redirect(308, `/${app}`);
}
