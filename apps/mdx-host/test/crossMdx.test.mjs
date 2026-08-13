import assert from "node:assert/strict";
import test from "node:test";

import { crossMdxDigest } from "../src/lib/crossMdx.js";

test("Twin readiness uses human language without weakening the boundary", () => {
  const digest = crossMdxDigest({
    twinReadiness: { conversation_ready: false, provider_connected_local_allowed: false },
    modelConnection: { connected: false },
    twinRuntime: { blocked_authority: { tool_execution_allowed: false } },
    twinArtifacts: { artifact_count: 0 }
  });

  const twin = digest.signals.find((signal) => signal.key === "twin");
  assert.ok(twin);
  assert.match(twin.line, /conversation is still getting ready/);
  assert.match(twin.line, /no live model is connected/);
  assert.match(twin.line, /tools still require governed approval/);
  assert.doesNotMatch(twin.line, /proof is incomplete/);
  assert.equal(twin.attention, true);
});

test("Twin readiness reports observed live state", () => {
  const digest = crossMdxDigest({
    twinReadiness: { conversation_ready: true, provider_connected_local_allowed: true },
    modelConnection: { connected: true },
    twinRuntime: { blocked_authority: { tool_execution_allowed: true } },
    twinArtifacts: { artifact_count: 1 }
  });

  const twin = digest.signals.find((signal) => signal.key === "twin");
  assert.ok(twin);
  assert.match(twin.line, /conversation is ready/);
  assert.match(twin.line, /a live model is connected/);
  assert.match(twin.line, /1 artifact on record/);
  assert.doesNotMatch(twin.line, /governed approval/);
  assert.equal(twin.attention, false);
});

test("an accepted observation does not claim a usable model without the tenant credential", () => {
  const digest = crossMdxDigest({
    twinReadiness: { conversation_ready: true, provider_connected_local_allowed: true },
    modelConnection: { connected: false },
    twinRuntime: { blocked_authority: { tool_execution_allowed: false } },
    twinArtifacts: { artifact_count: 0 }
  });

  const twin = digest.signals.find((signal) => signal.key === "twin");
  assert.match(twin.line, /no live model is connected/);
  assert.equal(twin.attention, true);
});

test("ratified shared memory becomes grounded Twin context", () => {
  const digest = crossMdxDigest({
    pages: {
      document_count: 14,
      latest_document: {
        title: "Notes from general",
        source_receipt_ids: ["pages_publication_1", "message_thread_message_1"]
      }
    },
    ratifiedMemory: {
      ratifications: [
        {
          content: "Published page page_notes titled Notes from general",
          proposed_by: "human:author",
          ratified_by: "human:reviewer"
        }
      ]
    }
  });

  const memory = digest.signals.find((signal) => signal.key === "memory");
  assert.ok(memory);
  assert.match(memory.line, /1 shared memory cleared by another person/);
  assert.match(memory.line, /Published page page_notes/);
  assert.match(digest.worldContext, /Memory:/);
  assert.equal(memory.href, "/memory");
  const pages = digest.signals.find((signal) => signal.key === "pages");
  assert.match(pages.line, /14 pages in the library/);
  assert.match(pages.line, /latest "Notes from general" started in Message/);
  assert.match(digest.worldContext, /started in Message/);
});

test("the latest Product bet becomes grounded Twin context", () => {
  const digest = crossMdxDigest({
    productBets: {
      bet_count: 2,
      bets: [
        { bet: "Prove the first beta path." },
        { bet: "Prove every flagship app survives a Render restart." }
      ]
    }
  });

  const product = digest.signals.find((signal) => signal.key === "product");
  assert.ok(product);
  assert.match(product.line, /2 bets on the board/);
  assert.match(product.line, /Prove every flagship app survives a Render restart/);
  assert.equal(product.href, "/product-direction");
  assert.match(digest.worldContext, /Product:/);
});
