// Forge runtime helpers. Reads ride the same-origin kernel proxy.

// The runtime truth Forge reads instead of approximating it locally. Each is a
// read-only kernel projection: the operator decision packet (is a change at the
// ship door, does it need a human, what is the safe next move), the runtime
// metrics and standing targets, the durable queue state, and the standing
// authorization registry. Where a route is empty or unavailable, the page keeps
// its honest local copy and never fabricates a number.
export const runtimeProjections = {
  forge: "/forge/runtime-projection.json",
  operatorPacket: "/runtime/operator-packet.json",
  metrics: "/runtime/metrics.json",
  queue: "/runtime/queue-projection.json",
  envelopes: "/autonomy/envelopes/projection.json"
};
