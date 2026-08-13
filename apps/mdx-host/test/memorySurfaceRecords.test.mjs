import assert from "node:assert/strict";
import test from "node:test";

import { memorySurfaceRecords } from "../src/lib/memorySurfaceRecords.js";

test("durable active records remain visible without pretending they were ratified", () => {
  const groups = memorySurfaceRecords({
    records: [
      {
        memory_id: "memory_1",
        memory_scope: "private_user_memory",
        content: "Remember the acceptance result",
        source_receipt_id: "source_1",
        gate_receipt_id: "gate_1"
      }
    ]
  });

  assert.equal(groups[0].surface, "Twin");
  assert.deepEqual(groups[0].waiting, []);
  assert.equal(groups[0].kept[0].recordBasis, "consolidated");
  assert.equal(groups[0].kept[0].receiptId, "gate_1");
  assert.equal(groups[0].kept[0].ratifiedBy, "");
});

test("pending and ratified projections win over the raw durable record", () => {
  const groups = memorySurfaceRecords({
    pending: [
      {
        memory_id: "memory_pending",
        memory_scope: "team_memory",
        content: "Wait for review",
        gate_receipt_id: "gate_pending"
      }
    ],
    ratifications: [
      {
        memory_id: "memory_kept",
        memory_scope: "team_memory",
        ratification_decision: "approve",
        ratification_receipt_id: "ratification_1",
        ratified_by: "reviewer"
      }
    ],
    records: [
      { memory_id: "memory_pending", memory_scope: "team_memory" },
      { memory_id: "memory_kept", memory_scope: "team_memory" }
    ]
  });

  assert.equal(groups[0].waiting.length, 1);
  assert.equal(groups[0].kept.length, 1);
  assert.equal(groups[0].kept[0].recordBasis, "ratified");
});
