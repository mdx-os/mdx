// The record's surface is named what people call it; old links keep working.
import { redirect } from "@sveltejs/kit";

export function load() {
  throw redirect(308, "/proof");
}
