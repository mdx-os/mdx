import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

test("the beta app handoff explains Mac install, iPhone TestFlight, and account continuity", () => {
  const handoff = readFileSync(new URL("../src/routes/download/macos/+page.svelte", import.meta.url), "utf8");
  const handoffServer = readFileSync(new URL("../src/routes/download/macos/+page.server.js", import.meta.url), "utf8");
  assert.match(handoffServer, /Move MDx into your Applications folder/);
  assert.match(handoff, /MDx for iPhone/);
  assert.match(handoffServer, /iPhone beta arrives through TestFlight/);
  assert.match(handoffServer, /same invited Google or Apple account everywhere/);
  assert.match(handoff, /Continue to your first-session guide/);
});

test("the public site names the app handoff instead of implementation status", () => {
  const nav = readFileSync(new URL("../src/lib/marketing/PublicNav.svelte", import.meta.url), "utf8");
  assert.match(nav, /label: "Download", href: "\/downloads"/);
  assert.doesNotMatch(nav, />Canary download<\/a>/);
});
