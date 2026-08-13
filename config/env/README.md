# MDx Alpha Env Profiles

These files are presence-only operator worksheets. They name the variables needed for each activation profile without storing values.

Use `.env.example` as the complete superset. Use these profile files when you want a smaller checklist for a specific path.

## Profiles

- `local.env.example`: deterministic local proof.
- `provider-connected-local.env.example`: local proof plus governed LLM provider turn-on.
- `render-supabase.env.example`: personal hosted alpha profile.
- `aws-eks-rds.env.example`: enterprise AWS profile.
- `gcp-gke-cloudsql.env.example`: alternate GCP profile.
- `self-hosted-kubernetes.env.example`: future open-source Kubernetes profile.

## Rules

- Do not put real values in these files.
- Do not commit `.env.local`, profile copies, API keys, service-role keys, JWT secrets, or cloud credentials.
- Env presence is not authority. Provider calls, login, deployment, worker execution, production writes, and cutover all require MDx receipts.
