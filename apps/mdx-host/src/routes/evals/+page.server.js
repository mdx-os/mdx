// Quality is the user's word for this surface; old links keep working.
import { redirect } from "@sveltejs/kit";

export function load() {
  throw redirect(308, "/quality");
}
