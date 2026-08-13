import adapter from "@sveltejs/adapter-node";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

export default {
  preprocess: vitePreprocess(),
  kit: {
    adapter: adapter(),
    alias: {
      $generated: "../../generated"
    },
    // CSP is kit-managed so SvelteKit's own hydration script ships with a
    // hash instead of forcing unsafe-inline. A static script-src 'self'
    // header silently blocks hydration on every interactive route - found
    // the day the first real route needed onMount.
    csp: {
      mode: "hash",
      directives: {
        "default-src": ["self"],
        // The hash entry allows the app.html theme-resolver script
        // (recompute if that script changes by one character).
        "script-src": ["self", "sha256-xk5a75CTxKDVvOsse9dBPX/6P9Ru5+WaHfiIuMUNJ3g="],
        "style-src": ["self", "unsafe-inline"],
        "img-src": ["self", "data:"],
        "font-src": ["self", "data:"],
        "connect-src": ["self", "http://127.0.0.1:18787", "http://127.0.0.1:18950", "ws://127.0.0.1:*"],
        "frame-ancestors": ["none"],
        "base-uri": ["self"],
        "form-action": ["self"]
      }
    }
  }
};
