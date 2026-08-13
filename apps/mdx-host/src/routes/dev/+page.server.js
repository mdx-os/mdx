import { developerMap } from "../../lib/developerMap.js";

export function load() {
  const map = developerMap();
  return {
    map,
    posture: {
      routeCount: map.routeCount,
      governedWriteCount: map.governedWriteCount,
      workItemCount: map.workItems.length,
      crateCount: map.crates.length
    }
  };
}
