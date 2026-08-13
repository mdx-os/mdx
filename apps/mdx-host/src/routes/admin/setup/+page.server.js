// Setup reads the alpha-onboarding packet server-side through the kernel
// proxy: the page arrives complete, and when the kernel is not answering
// every field degrades to null so the surface invites instead of claiming.
// The packet is truth - this load never adds readiness the kernel did not
// report, and env VALUES never appear anywhere (paths and presence only,
// per the packet's own ui_handoff contract).
import { pulseFrom, tracksFrom } from "../../../lib/setupTracks.js";

async function readPacket(fetchImpl, path) {
  try {
    const response = await fetchImpl(`/api/kernel${path}`, {
      signal: AbortSignal.timeout(1500)
    });
    if (response.ok) {
      return await response.json();
    }
  } catch (error) {
    return null;
  }
  return null;
}

function text(value, fallback = "") {
  return typeof value === "string" && value.length > 0 ? value : fallback;
}

function stateLabel(kind) {
  switch (kind) {
    case "local":
      return "local working";
    case "proof":
      return "production-shaped proof";
    case "blocked":
      return "blocked";
    default:
      return "not attempted";
  }
}

function titleFromId(id) {
  const known = {
    local_dev: "Local dev",
    provider_connected_local: "Provider-connected local",
    personal_render_supabase: "Render and Supabase",
    enterprise_aws_eks_rds: "AWS EKS and RDS",
    gcp_gke_cloudsql: "GCP GKE and Cloud SQL",
    self_hosted_kubernetes: "Self-hosted Kubernetes"
  };
  if (known[id]) return known[id];
  return String(id ?? "Profile")
    .replace(/_/g, " ")
    .replace(/\b\w/g, (char) => char.toUpperCase());
}

function profileState(profile) {
  if (!profile) return stateLabel("unknown");
  if (profile.allowed_now === true) return stateLabel("local");
  if (profile.status === "PUBLIC_READINESS_REQUIRED") return stateLabel("blocked");
  return stateLabel("proof");
}

function readinessRows({ router, env, auth, deployment, release }) {
  const ownerReady = router?.owner_claimed === true;
  const localProfile = deployment?.profile_bootstraps?.find((profile) => profile.id === "local_dev");
  const publicGates = release?.required_gates ?? [];
  const licenseGate = publicGates.find((gate) => gate.id === "license");

  return [
    {
      area: "Deployment target",
      kind: deployment ? "proof" : "unknown",
      state: stateLabel(deployment ? "proof" : "unknown"),
      current: deployment
        ? `${deployment.profile_summary?.profile_count ?? 0} profiles exist: local, Render, Kubernetes, AWS, GCP, and self-hosted.`
        : "The deployment profile packet is not available from the kernel yet.",
      why: "Operators need to separate a runnable local install from a deployable production target.",
      next: deployment?.route_check ?? "make v2-deployment-profile-bootstrap-route-check",
      gui: "Read-only here. Live deploy opens in the cloud deploy slice."
    },
    {
      area: "Database mode",
      kind: localProfile ? "local" : "unknown",
      state: stateLabel(localProfile ? "local" : "unknown"),
      current: localProfile
        ? "The default path is snapshot/local first; production-shaped profiles require managed Postgres proof."
        : "No local database profile was found in the packet.",
      why: "The system should not confuse local evaluation data with a backed-up production database.",
      next: localProfile?.proof_commands?.[0] ?? "make v2-fresh-install-proof",
      gui: "Local status only. Database cutover is not a GUI action yet."
    },
    {
      area: "Secrets",
      kind: env ? "proof" : "unknown",
      state: stateLabel(env ? "proof" : "unknown"),
      current: env
        ? `${env.env_summary?.env_var_count ?? 0} env names are inventoried; secret values recorded: ${env.secret_values_recorded === true ? "yes" : "no"}.`
        : "The env contract packet is not available from the kernel yet.",
      why: "Production setup needs named secret slots without ever showing or storing the secret values.",
      next: env?.route_check ?? "make v2-alpha-env-contract-route-check",
      gui: "Presence can be shown. Values never appear here."
    },
    {
      area: "Auth provider",
      kind: auth?.real_login_allowed_now === true ? "proof" : "blocked",
      state: stateLabel(auth?.real_login_allowed_now === true ? "proof" : "blocked"),
      current: auth
        ? `Local stub ready: ${auth.local_auth_bootstrap_ready === true ? "yes" : "no"}; real login allowed: ${auth.real_login_allowed_now === true ? "yes" : "no"}.`
        : "The auth bootstrap packet is not available from the kernel yet.",
      why: "Real users need an external IdP, tenant claim mapping, session controls, and negative-token proof.",
      next: auth?.route_check ?? "make v2-alpha-auth-bootstrap-route-check",
      gui: "No login button from this packet alone."
    },
    {
      area: "Email and invites",
      kind: "blocked",
      state: stateLabel("blocked"),
      current: "Invite and role receipts may exist as substrate, but email delivery and session cookie writes are closed.",
      why: "An invite is only real when a person can receive it, accept it, get a session, and land in a tenant role.",
      next: "finish auth delivery, acceptance, session, and role-mapping authority first",
      gui: "No send-invite action."
    },
    {
      area: "Admin account model",
      kind: ownerReady ? "local" : "unknown",
      state: stateLabel(ownerReady ? "local" : "unknown"),
      current: ownerReady
        ? `This install is claimed by ${text(router?.owner_name, "an owner")}.`
        : "No owner claim is visible in the setup router packet.",
      why: "The local owner is a setup receipt, not a substitute for production user accounts.",
      next: ownerReady ? "choose a production auth profile before admitting real users" : "claim the install first",
      gui: ownerReady ? "Owner is visible. Real account creation is blocked." : "Claim flow lives in welcome."
    },
    {
      area: "Tenant and users",
      kind: "blocked",
      state: stateLabel("blocked"),
      current: auth
        ? `${auth.auth_summary?.invite_receipt_count ?? 0} invite receipts and ${auth.auth_summary?.role_assignment_receipt_count ?? 0} role receipts are visible as local evidence.`
        : "Tenant/user evidence is not available from the kernel yet.",
      why: "Tenant membership must come from governed auth policy, not from a local fixture.",
      next: "make auth-multiuser-readiness-check",
      gui: "Receipts only. No user admission yet."
    },
    {
      area: "Observability",
      kind: env ? "proof" : "unknown",
      state: stateLabel(env ? "proof" : "unknown"),
      current: env
        ? "OpenTelemetry endpoint presence is part of the cloud env contract."
        : "No observability env contract is visible yet.",
      why: "Production needs traces, logs, and incident visibility before operators put traffic on it.",
      next: "make production-admin-readiness",
      gui: "Checklist only."
    },
    {
      area: "Backups and migrations",
      kind: deployment ? "proof" : "unknown",
      state: stateLabel(deployment ? "proof" : "unknown"),
      current: deployment
        ? "Cloud profiles call out managed Postgres, migration, backup, and RLS proof requirements."
        : "No deployment profile packet is visible yet.",
      why: "A deployed stack is not production-ready until data durability and schema evolution are proven.",
      next: "make production-admin-readiness",
      gui: "Checklist only."
    },
    {
      area: "RLS and security",
      kind: "proof",
      state: stateLabel("proof"),
      current: "Production readiness aggregates RLS, access matrix, auth boundary, TLS, and deployment gates.",
      why: "The stack must fail closed before real users or cloud traffic are admitted.",
      next: "make production-admin-readiness",
      gui: "Checklist only."
    },
    {
      area: "OSS hygiene",
      kind: licenseGate?.status === "COMPLETE" ? "proof" : "blocked",
      state: stateLabel(licenseGate?.status === "COMPLETE" ? "proof" : "blocked"),
      current: release
        ? `License gate: ${licenseGate?.status ?? "unknown"}; public release ready: ${release.public_release_ready === true ? "yes" : "no"}.`
        : "The public release packet is not available from the kernel yet.",
      why: "Public release and production setup share the same honesty bar: no hidden personal paths, secrets, or publish claims.",
      next: release?.public_readiness_check ?? "make public-readiness-check",
      gui: "Readiness only. Publishing remains closed."
    }
  ];
}

function profileOptions(packet, selectedId) {
  const profiles = Array.isArray(packet?.profile_bootstraps) ? packet.profile_bootstraps : [];
  return profiles.map((profile) => ({
    id: profile.id,
    label: titleFromId(profile.id),
    status: profile.status ?? "UNKNOWN",
    state: profileState(profile),
    selected: profile.id === selectedId,
    href: `/admin/setup?profile=${encodeURIComponent(profile.id)}`
  }));
}

function packageArtifacts(packet, profileId) {
  const artifacts = Array.isArray(packet?.deployment_artifacts) ? packet.deployment_artifacts : [];
  return artifacts
    .filter((artifact) => {
      const targets = Array.isArray(artifact.target_profiles) ? artifact.target_profiles : [];
      return targets.includes(profileId);
    })
    .map((artifact) => ({
      id: artifact.id ?? "",
      status: artifact.status ?? "UNKNOWN",
      paths: Array.isArray(artifact.paths)
        ? artifact.paths
        : artifact.path
          ? [artifact.path]
          : [],
      proof: artifact.proof ?? packet?.route_check ?? "make v2-deployment-profile-bootstrap-route-check",
      deploymentAllowedNow: artifact.deployment_allowed_now === true
    }));
}

function packageAssumptions(profileId) {
  const assumptions = {
    enterprise_aws_eks_rds: {
      title: "Enterprise AWS base package",
      standard: [
        "EKS workload profile with service account identity",
        "RDS or Aurora PostgreSQL with backups, migration proof, RLS, and runtime index proof",
        "Secrets Manager namespace with database, auth, provider, relay, and runtime secret refs",
        "ALB or equivalent ingress with TLS, private subnet posture, network policy, and egress controls",
        "CloudWatch or OpenTelemetry collector with trace/log export",
        "Microsoft Entra OIDC/SAML first, with tenant and role claim mapping"
      ],
      customerSpecific: [
        "AWS account, region, cluster, VPC, subnet, domain, certificate, and KMS boundaries",
        "RDS versus Aurora choice, sizing, backup/PITR, maintenance windows, and migration owner",
        "IdP metadata, issuer, audience, JWKS, tenant claim, role claim, and group policy",
        "Approved secret naming, IAM role, workload identity, and break-glass operating model",
        "Compliance evidence, observability destination, incident contacts, and cutover approvers"
      ]
    },
    personal_render_supabase: {
      title: "Hosted alpha package",
      standard: [
        "Render web/private services plus managed Postgres",
        "Render env groups with sync:false secret placeholders",
        "Optional Supabase Auth lane with server-only service role boundary"
      ],
      customerSpecific: [
        "Render team, database, region, domain, and dashboard secret ownership",
        "Supabase project, OAuth providers, redirect URLs, and claim mapping"
      ]
    },
    gcp_gke_cloudsql: {
      title: "Enterprise GCP base package",
      standard: [
        "GKE workload profile with Workload Identity",
        "Cloud SQL PostgreSQL with migration and tenant policy proof",
        "Secret Manager prefix and OpenTelemetry export"
      ],
      customerSpecific: [
        "Project, region, cluster, network, domain, certificate, and secret prefix",
        "Google Workspace or federated IdP claim mapping"
      ]
    },
    self_hosted_kubernetes: {
      title: "Self-hosted Kubernetes package",
      standard: [
        "Cloud-agnostic Kubernetes base manifests",
        "Operator-selected Postgres, cache, secret store, auth, and ingress",
        "Public readiness, license, secret history, and package posture gates"
      ],
      customerSpecific: [
        "Cluster distribution, ingress controller, certificate manager, storage class, and backup system",
        "External secret workflow, IdP, tenant mapping, and operations ownership"
      ]
    }
  };

  return (
    assumptions[profileId] ?? {
      title: "Local package",
      standard: ["Local runtime proof and profile worksheet"],
      customerSpecific: ["Operator-owned env values stay outside MDx"]
    }
  );
}

function deploymentProfiles(packet) {
  const profiles = Array.isArray(packet?.profile_bootstraps) ? packet.profile_bootstraps : [];
  return profiles.map((profile) => {
    const allowed = profile.allowed_now === true;
    return {
      id: profile.id,
      target: profile.target ?? "",
      status: profile.status ?? "UNKNOWN",
      state: stateLabel(allowed ? "local" : "proof"),
      next: profile.proof_commands?.[0] ?? packet?.route_check ?? "make v2-deployment-profile-bootstrap-route-check",
      blockedUntil: Array.isArray(profile.blocked_until) ? profile.blocked_until : [],
      allowedNow: allowed
    };
  });
}

function profilePreflight({ deployment, env, auth, onboarding, release, selectedProfileId }) {
  const profiles = Array.isArray(deployment?.profile_bootstraps) ? deployment.profile_bootstraps : [];
  if (profiles.length === 0) return null;
  const fallbackProfile =
    profiles.find((profile) => profile.id === "personal_render_supabase") ?? profiles[0];
  const profile =
    profiles.find((candidate) => candidate.id === selectedProfileId) ?? fallbackProfile;
  const envByName = new Map((env?.env_contract ?? []).map((entry) => [entry.name, entry]));
  const requiredEnvNames = Array.from(new Set(profile.required_env_names ?? []));
  const envRows = requiredEnvNames.map((name) => {
    const entry = envByName.get(name);
    return {
      name,
      present: entry?.present === true,
      kind: entry?.secret === true ? "secret" : "config",
      category: entry?.category ?? "unknown",
      valueRecorded: entry?.value_recorded === true || entry?.secret_value_recorded === true
    };
  });
  const missingEnvNames = envRows.filter((row) => !row.present).map((row) => row.name);
  const worksheet =
    (onboarding?.onboarding_tracks ?? []).find((track) => track.id === profile.id)?.env_template ??
    (onboarding?.env_templates ?? []).find((template) => template.activation_profile === profile.id)?.path ??
    "";
  const publicBlockers = (release?.required_gates ?? [])
    .filter((gate) => gate.status !== "COMPLETE")
    .map((gate) => `${titleFromId(gate.id)}: ${gate.status}`);
  const explicitBlockers = Array.isArray(profile.blocked_until) ? profile.blocked_until : [];
  const blockers = [
    ...explicitBlockers,
    ...(profile.id === "self_hosted_kubernetes" ? publicBlockers : []),
    ...(auth?.real_login_allowed_now === true ? [] : ["real login authority"]),
    ...(deployment?.cloud_deployment_allowed_now === true ? [] : ["deployment authority"])
  ];
  const proofCommands =
    Array.isArray(profile.proof_commands) && profile.proof_commands.length > 0
      ? profile.proof_commands
      : [deployment?.route_check ?? "make v2-deployment-profile-bootstrap-route-check"];
  const nextActions = [
    worksheet
      ? `Open ${worksheet} and map every required name to this profile's secret store.`
      : "Refresh the deployment profile packet so the worksheet path is available.",
    missingEnvNames.length > 0
      ? `Bind ${missingEnvNames.length} missing env names in the target secret manager or shell.`
      : "Env names are present by name; keep values outside MDx and rerun the proof.",
    `Run ${proofCommands[0]} and read the returned packet before any cutover.`,
    "Keep invite, login, production write, and cloud deploy authority closed until their receipts exist."
  ];

  return {
    id: profile.id,
    label: titleFromId(profile.id),
    status: profile.status ?? "UNKNOWN",
    state: profileState(profile),
    target: profile.target ?? "",
    servicePlane: Array.isArray(profile.service_plane) ? profile.service_plane : [],
    secretStrategy: profile.secret_strategy ?? "",
    authStrategy: profile.auth_strategy ?? "",
    dataPlane: profile.data_plane ?? "",
    networkStrategy: profile.network_strategy ?? "",
    scaleStrategy: profile.scale_strategy ?? "",
    worksheet,
    requiredEnvNames,
    requiredEnvCount: requiredEnvNames.length,
    missingEnvNames,
    missingEnvCount: missingEnvNames.length,
    envRows,
    proofCommands,
    blockers,
    nextActions,
    packageArtifacts: packageArtifacts(deployment, profile.id),
    packageAssumptions: packageAssumptions(profile.id),
    authority: {
      allowedNow: profile.allowed_now === true,
      cloudDeploymentAllowed: deployment?.cloud_deployment_allowed_now === true,
      productionWriteAllowed: onboarding?.production_write_allowed === true,
      realLoginAllowed: auth?.real_login_allowed_now === true,
      secretValuesRecorded: env?.secret_values_recorded === true
    }
  };
}

function blockedAuthorities(auth, deployment, onboarding) {
  return [
    {
      label: "Real external login",
      allowed: auth?.real_login_allowed_now === true,
      reason: "Requires IdP validation, tenant claim mapping, session controls, and policy receipts."
    },
    {
      label: "Email invite delivery",
      allowed: false,
      reason: "No email delivery, acceptance, account/session creation, and role mapping path is open yet."
    },
    {
      label: "Cloud deploy",
      allowed: deployment?.cloud_deployment_allowed_now === true,
      reason: "Profiles and artifacts exist, but deployment authority remains receipt-gated."
    },
    {
      label: "Production writes",
      allowed: onboarding?.production_write_allowed === true,
      reason: "Setup packets are read-only and cannot grant production authority."
    }
  ];
}

export async function load({ fetch, url }) {
  const [packet, router, env, auth, deployment, release] = await Promise.all([
    readPacket(fetch, "/v2/alpha-onboarding.json"),
    readPacket(fetch, "/install/setup-router.json"),
    readPacket(fetch, "/v2/alpha-env-contract.json"),
    readPacket(fetch, "/v2/alpha-auth-bootstrap.json"),
    readPacket(fetch, "/v2/deployment-profile-bootstrap.json"),
    readPacket(fetch, "/v2/public-release-plan.json")
  ]);
  const selectedProfileId = url.searchParams.get("profile") ?? "";
  const selectedProfilePreflight = profilePreflight({
    deployment,
    env,
    auth,
    onboarding: packet,
    release,
    selectedProfileId
  });

  return {
    tracks: tracksFrom(packet),
    pulse: pulseFrom(packet),
    readinessRows: readinessRows({ router, env, auth, deployment, release }),
    profileOptions: profileOptions(deployment, selectedProfilePreflight?.id ?? ""),
    selectedProfilePreflight,
    deploymentProfiles: deploymentProfiles(deployment),
    blockedAuthorities: blockedAuthorities(auth, deployment, packet),
    publicGates: Array.isArray(release?.required_gates) ? release.required_gates : [],
    packetRoutes: [
      "/v2/alpha-onboarding.json",
      "/v2/alpha-env-contract.json",
      "/v2/alpha-auth-bootstrap.json",
      "/v2/deployment-profile-bootstrap.json",
      "/v2/public-release-plan.json"
    ],
    proof: packet
      ? {
          status: packet.status ?? "",
          route: packet.route ?? "/v2/alpha-onboarding.json",
          evidenceFile: packet.evidence_file ?? "",
          refreshCommand: packet.proof_check ?? "make v2-alpha-onboarding"
        }
      : null,
    oneCommandLocal: packet?.one_command_local ?? "make v2-fresh-install-proof",
    boundary:
      "a worksheet is not a key, a key is not permission - provider calls, real login, deployment, and production writes each open only with their own recorded approval",
    safeNext: "run the local proof, open the product, and turn things on one receipt at a time"
  };
}
