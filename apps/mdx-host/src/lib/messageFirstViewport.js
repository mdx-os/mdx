// Message first-viewport facts from generated contracts: what the recorded
// conversation rail guarantees before any runtime answer arrives.
import threadContract from "../../../../generated/world-model/message-thread-message-contract.json";
import authBoundary from "../../../../generated/auth/mdx-auth-session-boundary.json";

export const messageFirstViewport = {
  source: "generated/world-model/message-thread-message-contract.json",
  currentSignal:
    "One conversation for people and agents - everything said here is kept, the moment it lands.",
  judgment:
    "The thread is the team's shared memory, not a chat that evaporates.",
  safeNext: "Send a message - it reaches active local MDx participants and is kept.",
  boundary: [
    "delivery stays on this machine until production realtime turns on with evidence",
    "service-role fanout blocked",
    "presence mutation blocked",
    authBoundary.actor_admission.production_write_allowed ? "production writes allowed" : "production writes blocked"
  ],
  evidence: [threadContract.name ?? "message thread contract", threadContract.required_receipt_kind ?? "message receipt kind"]
};
