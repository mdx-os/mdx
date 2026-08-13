// The switches this page offers are declared in the generated capability
// registry - the page renders the declaration, it never hand-maintains the
// list. Server-load only: generated JSON never ships in client bundles.
import registry from "../../../../../../generated/twin/twin-capability-registry.json";
import { companions } from "../../../lib/twinCompanions.js";

export function load() {
  const switchable = registry.capabilities.filter(
    (capability) => capability.admin_editable && capability.config_key
  );
  const ofKind = (kind) =>
    registry.capabilities
      .filter((capability) => capability.kind === kind)
      .map((capability) => ({
        id: capability.id,
        status: capability.status,
        label: capability.ui_label,
        line: capability.ui_line,
        description: capability.human_description
      }));
  return {
    groups: registry.switch_groups.map((group) => ({
      title: group.title,
      cue: group.cue,
      items: switchable
        .filter((capability) => capability.switch_group === group.id)
        .map((capability) => ({
          key: capability.config_key,
          label: capability.ui_label,
          line: capability.ui_line
        }))
    })),
    companions: companions.map((companion) => ({
      id: companion.id,
      name: companion.name,
      stance: companion.stance
    })),
    routing: ofKind("routing_policy"),
    memory: ofKind("memory_policy"),
    connectors: ofKind("connector").filter((entry) => entry.status === "not_built"),
    registryVersion: registry.registry_version
  };
}
