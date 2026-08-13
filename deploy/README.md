# MDx Deployment Artifacts

These artifacts keep provider setup outside the MDx security core. The personal
beta Render plus Supabase adapter is deployable to staging; the other cloud
profiles remain reference scaffolds until their provider-specific work lands.

## Profiles

- `deploy/docker-compose`: local-secure reference. A production-shaped stack on one machine that boots through the deployment-profile gate.
- `render.yaml`: canonical personal-beta staging Blueprint with a public product
  host and private kernel. It creates no Render Postgres database.
- `deploy/render/production-render.yaml`: production-equivalent Blueprint held
  behind staging, canary, and founder approval.
- `deploy/supabase`: direct-Postgres provider baseline. Application tables stay
  off the Supabase Data API unless a reviewed migration explicitly grants them.
- `deploy/kubernetes/base`: cloud-agnostic Kubernetes base for AWS EKS, GCP GKE, and self-hosted profiles.
- `deploy/terraform/aws-eks-rds`: AWS profile skeleton for EKS/RDS/Secrets Manager binding.
- `deploy/terraform/gcp-gke-cloudsql`: GCP profile skeleton for GKE/Cloud SQL/Secret Manager binding.

## Same Security Core

Provider choice is configuration and adapters, not a new auth or tenancy path.
Every adapter points at the same fail-closed deployment-profile gate
(`crates/mdx-core/src/deployment_profile.rs`): the cloud adapters set
`MDX_DEPLOYMENT_MODE=production` and the Docker Compose reference sets
`local-secure`. A deployed node boots through that gate and refuses an insecure
start. No adapter can bypass the auth, RLS, delegation, or receipt requirements;
adapters supply infrastructure only.

## Boundaries

- No secret values are committed.
- The Docker Compose `.env` is gitignored; only `.env.example` is tracked.
- Kubernetes `mdx-runtime-secrets` is intentionally empty and must be supplied by an external secret store.
- Render provider and auth values use `sync: false` and must be supplied in the
  Render dashboard or an approved secret workflow.
- Render reaches Supabase through the TLS-required IPv4-compatible Supavisor
  session pooler. The direct IPv6 database address is not used.
- The product host receives only the private kernel host and port. It never
  receives a Supabase service-role key and explicitly refuses trusted identity
  headers.
- Production Render resources and `mdx-os.com` remain unapplied and human-gated.
- Terraform profiles expose selected profile metadata only. They do not create infrastructure yet.
- Cloud deployment stays blocked until MDx records deployment, auth, secret-store, database, relay, and operator approval receipts.

## Proof

Run:

```sh
make deployment-adapter-check
make v2-deployment-artifacts-check
make v2-deployment-profile-bootstrap-route-check
```
