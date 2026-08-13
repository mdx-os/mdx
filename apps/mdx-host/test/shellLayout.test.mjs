import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { sessionPresentation } from "../src/lib/session.js";

const layout = readFileSync(new URL("../src/routes/+layout.svelte", import.meta.url), "utf8");

test("the desktop shell keeps the status row content-sized", () => {
  assert.match(layout, /grid-template-rows:\s*auto minmax\(0, 1fr\)/);
  assert.match(layout, /\.host-shell\.console[\s\S]*?grid-template-rows:\s*minmax\(0, 1fr\)/);
});

test("navigation resets the shell-owned scroll container", () => {
  assert.match(layout, /afterNavigate\(resetHostStageScroll\)/);
  assert.match(layout, /history\.scrollRestoration = "manual"/);
  assert.match(layout, /hostStage\?\.scrollTo\(0, 0\);/);
  assert.match(layout, /window\.scrollTo\(0, 0\);/);
  assert.match(layout, /<main bind:this=\{hostStage\}/);
});

test("public website routes use native document scrolling", () => {
  assert.match(layout, /page\.url\.pathname === "\/open-source"/);
  assert.match(layout, /page\.url\.pathname === "\/downloads"/);
  assert.match(layout, /class:pagescroll=\{isLanding \|\| onPublicGate\}/);
  assert.match(layout, /body:has\(\.host-shell\.pagescroll\)/);
});

test("the shared shell presents every signed-in participant as themselves", () => {
  const presentation = sessionPresentation({
    authenticated: true,
    identity_source: "supabase_signed_jwt",
    role: "viewer",
    user_id: "53b164c9-should-never-render",
    email: "private@example.com"
  });

  assert.deepEqual(presentation, {
    label: "You",
    avatar: "Y",
    title: "Your profile and settings",
    trust: "Verified sign-in · viewer seat"
  });
  assert.doesNotMatch(layout, /title="MD - You"/);
  assert.doesNotMatch(layout, /working as \$\{data\.session\?\.user_id/);
});

test("the shell trust label distinguishes local proof without exposing identity data", () => {
  assert.equal(
    sessionPresentation({
      authenticated: true,
      identity_source: "local_demo_fixture",
      role: "owner"
    }).trust,
    "Local demo seat · owner seat"
  );
});

test("public funnel pages never start the private kernel heartbeat", () => {
  assert.match(layout, /if \(!browser \|\| onPublicExperience\)/);
  assert.match(layout, /data\.publicExperience === true/);
});
