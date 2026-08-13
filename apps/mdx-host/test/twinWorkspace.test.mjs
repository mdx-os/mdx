import test from "node:test";
import assert from "node:assert/strict";
import {
  applyConversationPreference,
  activeDraftReceiptId,
  attachmentDisposition,
  copiedConversationContext,
  conversationMarkdown,
  normalizeWorkspace,
  researchPreflightState,
  sortedVisibleSessions,
  workspaceContext
} from "../src/lib/twinWorkspace.js";

test("workspace context is bounded and explicit", () => {
  const workspace = normalizeWorkspace({ name: " Launch ", instructions: "Use signed facts", files: ["brief.md", ""] });
  assert.equal(workspaceContext(workspace), "Active project: Launch\nProject instructions: Use signed facts\nScoped references: brief.md");
});

test("conversation preferences pin, archive, and rename without mutating receipts", () => {
  const original = Object.freeze({ untouched: Object.freeze({ title: "Original" }) });
  let prefs = applyConversationPreference(original, "a", { title: "Alpha", pinned: true });
  prefs = applyConversationPreference(prefs, "b", { archived: true });
  const visible = sortedVisibleSessions([{ id: "b", title: "B", lastOrder: 2 }, { id: "a", title: "A", lastOrder: 1 }], prefs);
  assert.deepEqual(visible.map(({ id, displayTitle }) => ({ id, displayTitle })), [{ id: "a", displayTitle: "Alpha" }]);
  assert.deepEqual(original, { untouched: { title: "Original" } });
  assert.equal(sortedVisibleSessions([{ id: "a", title: "Alpha", lastOrder: 1 }], prefs, "missing").length, 0);
  assert.deepEqual(
    sortedVisibleSessions([{ id: "b", title: "B", lastOrder: 2 }], prefs, "", true).map(({ id, archived }) => ({ id, archived })),
    [{ id: "b", archived: true }]
  );
});

test("new conversations sort ahead of older receipt order", () => {
  const visible = sortedVisibleSessions([
    { id: "old", title: "Old", lastOrder: 42 },
    { id: "new", title: "New", lastOrder: Number.POSITIVE_INFINITY }
  ], {});
  assert.deepEqual(visible.map(({ id }) => id), ["new", "old"]);
});

test("research claims a recorded block only when a control receipt exists", () => {
  assert.equal(researchPreflightState(true, { connector_execution_allowed: false }), "unavailable");
  assert.equal(researchPreflightState(false, { control_receipt_id: "control-1" }), "unavailable");
  assert.equal(
    researchPreflightState(true, { control_receipt_id: "control-1", connector_execution_allowed: false }),
    "blocked"
  );
  assert.equal(
    researchPreflightState(true, { control_receipt_id: "control-2", connector_execution_allowed: true }),
    "ready"
  );
});

test("research never reuses a copied session's draft receipt", () => {
  const copiedTurns = [{ restored: true, draftReceiptId: "draft-from-old-session" }];
  const projection = {
    drafts: [
      { session_id: "old", draft_receipt_id: "draft-from-old-session" },
      { session_id: "current", draft_receipt_id: "draft-from-current-session" }
    ]
  };
  assert.equal(activeDraftReceiptId(copiedTurns, projection, "current"), "draft-from-current-session");
  assert.equal(activeDraftReceiptId(copiedTurns, projection, "new-copy"), "");
  assert.equal(
    activeDraftReceiptId([{ restored: false, draftReceiptId: "draft-just-saved" }], projection, "current"),
    "draft-just-saved"
  );
});

test("copied conversation context includes both sides and stays bounded", () => {
  const context = copiedConversationContext("Source", [
    { kind: "you", body: "Question" },
    { kind: "companion", companionName: "Advisor", body: "Answer" }
  ], 80);
  assert.match(context, /# Source[\s\S]*## You[\s\S]*Question[\s\S]*## Advisor[\s\S]*Answer/);
  assert.ok(context.length <= 80);
});

test("attachment policy refuses unsafe readers clearly", () => {
  assert.equal(attachmentDisposition({ name: "scan.png", type: "image/png" }), "blocked_image");
  assert.equal(attachmentDisposition({ name: "plan.docx" }), "blocked_office");
  assert.equal(attachmentDisposition({ name: "notes.md", type: "text/markdown" }), "text");
  assert.equal(attachmentDisposition({ name: "brief.pdf", type: "application/pdf" }), "pdf");
  assert.equal(attachmentDisposition({ name: "archive.zip", type: "application/zip" }), "unsupported");
});

test("conversation export remains portable markdown", () => {
  assert.match(conversationMarkdown("Decision", [{ kind: "you", body: "Ship?" }, { kind: "companion", companionName: "Twin", body: "Yes." }]), /# Decision[\s\S]*## You[\s\S]*## Twin/);
});
