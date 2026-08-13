import { redirect } from "@sveltejs/kit";
import { SOURCE_REPOSITORY_URL } from "../../lib/marketing/publicSite.js";

export const load = () => redirect(307, SOURCE_REPOSITORY_URL);
