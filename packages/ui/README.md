# @mdx/ui

Internal UI package for MDx operator primitives.

This package is the production package boundary for the shared components currently proven under `apps/shared`. The first platform slice copies those primitives here without changing existing app imports, so Claude's surface work and the current `apps/mdx` proof shell keep running while the cloud host spike adopts package imports.

Future work should consume from this package and retire app-local shared imports only when one integrator owns the migration.
