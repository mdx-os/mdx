// The Message admin tab's contract truth: the offline-queue, replay, and
// agent-noise declarations the runtime is checked against - rendered as
// facts, never hand-maintained. Server-load only.
import offlineQueue from "../../../../../../generated/message/message-offline-queue-contract.json";
import replay from "../../../../../../generated/message/message-replay-contract.json";
import agentNoise from "../../../../../../generated/message/message-agent-noise-policy.json";

export function load() {
  return {
    rails: {
      offlineQueue: {
        line: offlineQueue.human_line,
        storage: offlineQueue.queue.database,
        backoff: offlineQueue.flush.backoff_ms.map((ms) => `${ms / 1000}s`).join(" → ")
      },
      replay: {
        line: replay.human_line,
        states: replay.catch_up.states
      },
      agentNoise: {
        line: agentNoise.human_line,
        foldThreshold: agentNoise.folding.threshold_consecutive_non_human
      }
    },
    relayFacts: [
      { label: "Relay address", value: "MDX_MESSAGE_RELAY_URL on the kernel (valkey), port 9000 for clients" },
      { label: "Replay window", value: "MDX_MESSAGE_RELAY_HOT_CACHE_PER_TOPIC, default 200 per topic" },
      { label: "Rate limits", value: "per connection: MDX_MESSAGE_RELAY_MAX_MSG_PER_SEC and byte caps, set on the relay process" }
    ],
    connectors: [
      { label: "Slack", line: "not built yet - lights up when its contract lands" },
      { label: "Teams", line: "not built yet - lights up when its contract lands" },
      { label: "GitHub", line: "not built yet - lights up when its contract lands" },
      { label: "Email", line: "not built yet - lights up when its contract lands" }
    ]
  };
}
