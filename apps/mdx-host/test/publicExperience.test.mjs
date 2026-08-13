import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { isPublicExperiencePath } from "../src/lib/publicExperience.js";

test("the public experience path contract includes pages and Svelte data reads", () => {
  for (const path of [
    "/landing",
    "/auth/sign-in",
    "/waitlist",
    "/redeem",
    "/welcome/pending",
    "/open-source",
    "/downloads",
    "/forge/connect/github",
    "/forge/connect/github/webhook",
    "/landing/__data.json"
  ]) {
    assert.equal(isPublicExperiencePath(path), true, path);
  }
  for (const path of ["/twin", "/forge", "/welcome/beta", "/api/kernel/version.json"]) {
    assert.equal(isPublicExperiencePath(path), false, path);
  }
});

test("the public root layout short-circuits private kernel reads", () => {
  const serverLayout = readFileSync(new URL("../src/routes/+layout.server.js", import.meta.url), "utf8");
  assert.match(serverLayout, /const publicExperience = isPublicExperiencePath\(url\.pathname\)/);
  assert.match(
    serverLayout,
    /publicExperience \? \[null, null, \{ reachable: false, error: "" \}\] : await Promise\.all/
  );
  assert.match(serverLayout, /return \{\s+publicExperience,/);
});

test("the anonymous pending room never claims that a stranger is signed in", () => {
  const pending = readFileSync(new URL("../src/routes/welcome/pending/+page.svelte", import.meta.url), "utf8");
  const welcomeLayout = readFileSync(new URL("../src/routes/welcome/+layout.svelte", import.meta.url), "utf8");
  assert.match(pending, /The beta is invite-only for now\./);
  assert.doesNotMatch(pending, /You're signed in - but not on the beta yet\./);
  assert.match(welcomeLayout, /\{#if !onPending\}/);
  assert.match(pending, /reply to your invite thread/);
  assert.match(pending, /Check access here/);
  assert.match(pending, /continues the same invite/);
  assert.doesNotMatch(pending, /the moment it's ready/);
});

test("waitlist success keeps its support reference without linking to private evidence", () => {
  const waitlist = readFileSync(new URL("../src/routes/waitlist/+page.svelte", import.meta.url), "utf8");
  assert.match(waitlist, /Your request reference is/);
  assert.match(waitlist, /before your account is active/);
  assert.doesNotMatch(waitlist, /href=\{`\/evidence\/\$\{encodeURIComponent\(form\.receiptId\)\}`\}/);
});

test("sign-in guides a new participant into one cross-device workspace", () => {
  const signIn = readFileSync(new URL("../src/routes/auth/sign-in/+page.server.js", import.meta.url), "utf8");
  assert.match(signIn, /provider for the address that received your invite/);
  assert.match(signIn, /connect Apple from You without creating another workspace/);
  assert.match(signIn, /private relay address must match your beta invite/);
});
