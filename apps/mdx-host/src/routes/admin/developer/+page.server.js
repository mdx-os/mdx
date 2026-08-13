// Developer reads only generated contracts - the repo describing itself.
// No kernel required: this page is complete on a fresh clone before the
// server ever starts, which is exactly the moment an engineer needs it.
import { developerMap } from "../../../lib/developerMap.js";

export function load() {
  return {
    map: developerMap(),
    boundary:
      "this page reads the repo's own generated contracts and never acts; the example write below only works against your local kernel, under its receipts",
    safeNext: "clone, run make local-smoke, open this page, and pick a work item from the queue"
  };
}
