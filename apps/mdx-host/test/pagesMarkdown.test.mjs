import assert from "node:assert/strict";
import test from "node:test";
import { pagesMarkdownBlocks } from "../src/lib/pagesMarkdown.js";

test("published page tables remain one semantic block with aligned cells", () => {
  const blocks = pagesMarkdownBlocks(`# Current truth

| Substrate | Status | Meaning |
| --- | :---: | ---: |
| Postgres | LIVE | Durable storage |
| AgentCore | READY |

Next decision.`);

  assert.deepEqual(blocks, [
    { kind: "h", level: 1, text: "Current truth" },
    {
      kind: "table",
      headers: ["Substrate", "Status", "Meaning"],
      rows: [
        ["Postgres", "LIVE", "Durable storage"],
        ["AgentCore", "READY", ""]
      ]
    },
    { kind: "p", text: "Next decision." }
  ]);
});

test("pipe-like prose and fenced code are not promoted into page tables", () => {
  const blocks = pagesMarkdownBlocks(`A | B
not a divider

\`\`\`
| code | only |
| --- | --- |
\`\`\``);

  assert.deepEqual(blocks, [
    { kind: "p", text: "A | B not a divider" },
    { kind: "code", text: "| code | only |\n| --- | --- |" }
  ]);
});
