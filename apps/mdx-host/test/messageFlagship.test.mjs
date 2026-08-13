import assert from "node:assert/strict";
import test from "node:test";
import { buildMessageCatchUp, composeMessageWithAttachment } from "../src/routes/message/messageCatchUp.js";
import { foldReactions } from "../src/routes/message/messageReactions.js";
import { createMessageNotification, shouldNotifyMessage } from "../src/routes/message/messageNotifications.js";
import { mergePresenceRoster } from "../src/routes/message/messagePresence.js";
import { actorDisplayName } from "../src/lib/actorPresentation.js";

test("catch-up excludes thread plumbing and keeps the newest recorded conversation", () => {
  const messages = [
    { actor: "Mina", body: "First", sequence: 1, receiptId: "r1" },
    { actor: "Scout", body: "+:👍", sequence: 2, receiptId: "r2", reactTo: "r1" },
    { actor: "Scout", body: "A reply", sequence: 3, receiptId: "r3", replyTo: "r1" },
    { actor: "Scout", body: "Second", sequence: 4, receiptId: "r4" }
  ];
  const result = buildMessageCatchUp(messages);
  assert.deepEqual(result.recent.map((message) => message.receiptId), ["r1", "r4"]);
  assert.deepEqual(result.participants, ["Mina", "Scout"]);
});

test("object attachments accept product paths and safe web links", () => {
  assert.deepEqual(
    composeMessageWithAttachment("Review this", "/pages/roadmap", "Roadmap"),
    { body: "Review this\n\n[Roadmap](/pages/roadmap)", error: "" }
  );
  assert.equal(composeMessageWithAttachment("", "https://example.com/run", "Run").error, "");
});

test("object attachments reject executable and protocol-relative links", () => {
  assert.match(composeMessageWithAttachment("", "javascript:alert(1)", "bad").error, /http/);
  assert.match(composeMessageWithAttachment("", "//evil.example", "bad").error, /http/);
  assert.match(composeMessageWithAttachment("", "/etc/passwd", "bad").error, /MDx path/);
  assert.match(composeMessageWithAttachment("", "/pages/roadmap)[click](https://evil.example", "bad").error, /MDx path/);
  assert.match(composeMessageWithAttachment("", "/pages/road map", "bad").error, /MDx path/);
});

test("object attachments encode parentheses in otherwise valid web URLs", () => {
  const result = composeMessageWithAttachment("", "https://example.com/run(2)", "Run");
  assert.equal(result.body, "[Run](https://example.com/run%282%29)");
});

test("catch-up bounds output and clips previews", () => {
  const messages = Array.from({ length: 20 }, (_, index) => ({
    actor: `Person ${index % 3}`,
    body: "x".repeat(400),
    sequence: index + 1,
    receiptId: `r${index + 1}`
  }));
  const result = buildMessageCatchUp(messages);
  assert.equal(result.recent.length, 12);
  assert.equal(result.highlights.length, 4);
  assert.equal(result.highlights[0].body.length, 180);
  assert.equal(result.recent[0].receiptId, "r9");
});

test("reactions fold the latest event per actor instead of summing history", () => {
  const folded = foldReactions([
    { reactTo: "parent", actorId: "user:mina", actor: "Mina", body: "-:👍", sequence: 3 },
    { reactTo: "parent", actorId: "user:mina", actor: "Mina", body: "+:👍", sequence: 1 },
    { reactTo: "parent", actorId: "user:mina", actor: "Mina", body: "+:✅", sequence: 4 },
    { reactTo: "parent", actorId: "agent:scout", actor: "Scout", body: "+:👍", sequence: 2 }
  ], "mina");
  assert.deepEqual(folded.get("parent"), [
    { emoji: "✅", count: 1, mine: true },
    { emoji: "👍", count: 1, mine: false }
  ]);
  const distinctKinds = foldReactions([
    { reactTo: "parent", actorId: "agent:mina", actor: "Mina", body: "+:👀" }
  ], "mina");
  assert.equal(distinctKinds.get("parent")[0].mine, false);
});

test("notifications are live-only and constructor failures stay contained", () => {
  const base = {
    enabled: true,
    visibilityState: "hidden",
    actor: "Scout",
    selfActor: "Mina",
    muted: false
  };
  assert.equal(shouldNotifyMessage({ ...base, liveState: "syncing" }), false);
  assert.equal(shouldNotifyMessage({ ...base, liveState: "live" }), true);
  class RefusingNotification {
    constructor() { throw new Error("notifications unavailable"); }
  }
  assert.equal(createMessageNotification(RefusingNotification, "Title", {}), null);
});

test("presence merges only the signed-in human's kernel aliases", () => {
  const roster = [
    {
      actor_id: "local_user",
      actor_type: "system",
      display_name: "Local User",
      last_action: "marketplace act recorded",
      last_active_at: "2026-07-15T01:07:54Z",
      action_count: 18,
      active: true
    },
    {
      actor_id: "human:local_user",
      actor_type: "human",
      display_name: "Local User",
      last_action: "said something",
      last_active_at: "2026-07-15T01:06:33Z",
      action_count: 13,
      active: true
    },
    {
      actor_id: "agent:local_user",
      actor_type: "agent",
      display_name: "Local User",
      last_action: "agent posted",
      last_active_at: "2026-07-15T01:05:00Z",
      action_count: 2,
      active: false
    }
  ];

  const result = mergePresenceRoster(roster, "local_user");
  assert.equal(result.length, 2);
  assert.deepEqual(result[0], {
    actor_id: "human:local_user",
    actor_type: "human",
    display_name: "You",
    last_action: "marketplace act recorded",
    last_active_at: "2026-07-15T01:07:54Z",
    action_count: 31,
    active: true
  });
  assert.equal(result[1].actor_id, "agent:local_user");
  assert.equal(roster[0].actor_id, "local_user");
});

test("actor presentation never exposes opaque authentication identifiers", () => {
  const uuid = "00000000-0000-4000-8000-000000000001";
  assert.equal(actorDisplayName({ actorId: uuid, displayName: uuid, selfActorId: uuid }), "You");
  assert.equal(actorDisplayName({ actorId: `human:${uuid}`, displayName: uuid }), "Member");
  assert.equal(actorDisplayName({ actorId: `agent:${uuid}`, displayName: uuid, actorType: "agent" }), "Agent");
  assert.equal(actorDisplayName({ actorId: "human:mina", displayName: "Mina" }), "Mina");
});
