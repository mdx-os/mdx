import { sveltekit } from "@sveltejs/kit/vite";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
import { defineConfig } from "vite";

const repoRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "../..");

export default defineConfig({
  plugins: [sveltekit()],
  server: {
    host: "127.0.0.1",
    port: 5182,
    fs: {
      allow: [repoRoot]
    }
  }
});
