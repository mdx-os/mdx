import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { isPublicExperiencePath } from "../src/lib/publicExperience.js";

const read = (relativePath) =>
  readFileSync(new URL(`../src/${relativePath}`, import.meta.url), "utf8");

test("the public site has one restrained navigation contract", () => {
  const nav = read("lib/marketing/PublicNav.svelte");
  const footer = read("lib/marketing/PublicFooter.svelte");

  assert.match(nav, /label: "Product", href: "\/landing"/);
  assert.match(nav, /label: "Open source", href: SOURCE_REPOSITORY_URL/);
  assert.match(nav, /label: "Download", href: "\/downloads"/);
  assert.doesNotMatch(nav, /label: "Forge"/);
  assert.doesNotMatch(nav, /label: "Security"/);
  assert.match(nav, /aria-current=\{active === link\.id \? "page" : undefined\}/);
  assert.match(footer, /Source is open\. Hosted beta is invite-only\./);
  assert.match(footer, /Request an invite/);
});

test("open source is a first-class launch action with one canonical target", () => {
  const publicSite = read("lib/marketing/publicSite.js");
  const landing = read("routes/landing/+page.svelte");
  const sourceRedirect = read("routes/open-source/+page.server.js");
  const forgeRedirect = read("routes/forge-product/+page.server.js");

  assert.match(publicSite, /https:\/\/github\.com\/mdx-os\/mdx/);
  assert.match(landing, /Take the whole thing\./);
  assert.match(landing, /The source is public/);
  assert.match(landing, /Apache 2\.0/);
  assert.match(sourceRedirect, /SOURCE_REPOSITORY_URL/);
  assert.match(forgeRedirect, /\/landing#product/);
});

test("the download hub names the real update path without a second marketing story", () => {
  const downloads = read("routes/downloads/+page.svelte");

  assert.match(downloads, /Hosted updates arrive automatically/);
  assert.match(downloads, /Prompts when a new build is ready/);
  assert.match(downloads, /Updates through TestFlight/);
  assert.match(downloads, /Mac builds are signed and notarized before release/);
  assert.doesNotMatch(downloads, /One MDx\. Wherever you work/);
});

test("public claims stay modest until usage supplies outside proof", () => {
  const landing = read("routes/landing/+page.svelte");
  const security = read("routes/security/+page.svelte");

  for (const claim of [/100,?000 users/i, /production ready/i, /posture score/i, /operating system for AI/i]) {
    assert.doesNotMatch(landing, claim);
    assert.doesNotMatch(security, claim);
  }
  assert.match(landing, /MDx is early\. Expect rough edges/);
  assert.match(security, /The claims have checks behind them/);
});

test("unfinished developer docs stay behind sign-in while launch pages remain public", () => {
  assert.equal(isPublicExperiencePath("/open-source"), true);
  assert.equal(isPublicExperiencePath("/downloads"), true);
  assert.equal(isPublicExperiencePath("/dev"), false);
});
