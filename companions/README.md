# Companions

Companions are persistent advisory archetypes, not loop-runners.

They answer grounded queries for humans-at-edge through Twin or Concierge surfaces. They do not autonomously execute actions, close operating loops, spawn workers, mint credentials, deploy software, spend money, or write memory outside declared evidence paths.

Every companion declaration must keep:

- `population: companion`
- `runtime_status: PENDING-LIVE-RUN`
- `autonomy_ceiling: READ_ONLY_GROUNDED`
- `receipt_evidence_required: true`
- explicit `allowed_surfaces`
- explicit `forbidden_actions`

Companion runtime behavior stays pending until Twin has a grounded read path and observed receipt evidence.
