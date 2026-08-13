import sourceMap from "../../../../generated/source-map/mdx-source-map.json";
import routes from "../../../../generated/routes/mdx-local-routes.json";
import marketplace from "../../../../generated/marketplace/capability-registry.json";
import aegisCard from "../../../../generated/capability-cards/aegis_scanner_agent.json";
import charterCard from "../../../../generated/capability-cards/charter_attestation_agent.json";
import evalsCard from "../../../../generated/capability-cards/evals_runner_agent.json";
import forgeCard from "../../../../generated/capability-cards/forge_orchestrator_agent.json";
import productCard from "../../../../generated/capability-cards/product_shaping_agent.json";
import talentCard from "../../../../generated/capability-cards/talent_autonomy_agent.json";

const capabilityCards = [
  aegisCard,
  charterCard,
  evalsCard,
  forgeCard,
  productCard,
  talentCard
];

const components = [
  {
    id: "proof-spine",
    layer: "foundation",
    name: "Proof Spine",
    line: "Receipts, audit slices, local evidence, and status packets.",
    surfaces: ["receipts", "audit", "local_evidence", "status", "health"],
    sourceHints: ["activation runway", "local evidence", "status"],
    flow: "local route -> receipt or packet -> evidence surface -> operator decision"
  },
  {
    id: "access-boundary",
    layer: "foundation",
    name: "Access Boundary",
    line: "Session controls, actor admission, and tenant-policy preflights.",
    surfaces: ["auth_session"],
    sourceHints: ["auth session boundary"],
    flow: "session -> actor admission -> policy preflight -> governed route"
  },
  {
    id: "capability-catalog",
    layer: "foundation",
    name: "Capability Catalog",
    line: "Marketplace entries, capability cards, risk states, and blocked actions.",
    surfaces: ["marketplace"],
    sourceHints: ["marketplace", "capability"],
    capabilityAgents: [
      "aegis_scanner_agent",
      "charter_attestation_agent",
      "evals_runner_agent",
      "forge_orchestrator_agent",
      "product_shaping_agent",
      "talent_autonomy_agent"
    ],
    flow: "capability source -> scan state -> access level -> governed install or refusal"
  },
  {
    id: "forge-harness",
    layer: "orchestration",
    name: "Forge And Harness",
    line: "Build intent, run evidence, harness gates, Talent, and local drivers.",
    surfaces: ["forge", "harness", "loop_runner", "talent", "memory_store"],
    sourceHints: ["harness", "forge", "talent", "worker"],
    capabilityAgents: ["forge_orchestrator_agent", "talent_autonomy_agent"],
    flow: "work item -> harness admission -> Forge run -> Talent boundary -> evidence"
  },
  {
    id: "product-strategy",
    layer: "orchestration",
    name: "Product And Strategy",
    line: "Direction, ratification, feedback autonomy, and company goals.",
    surfaces: ["product", "strategy", "beta_feedback"],
    sourceHints: ["product", "strategy", "feedback"],
    capabilityAgents: ["product_shaping_agent"],
    flow: "signal -> assessment -> shaped work -> ratification or handoff"
  },
  {
    id: "v1-replacement",
    layer: "orchestration",
    name: "v1 Replacement",
    line: "Read shadows, write mirrors, migration gates, and cutover evidence.",
    surfaces: ["v1", "v1_migration"],
    sourceHints: ["v1"],
    flow: "v1 route map -> shadow proof -> approval request -> cutover evidence"
  },
  {
    id: "twin-observatory",
    layer: "interface",
    name: "Twin And Observatory",
    line: "Companion state, sessions, posture, answers, and current signal.",
    surfaces: ["twin", "twin_session", "concierge", "observatory"],
    sourceHints: ["twin", "observatory", "read surfaces"],
    capabilityAgents: ["aegis_scanner_agent", "evals_runner_agent"],
    flow: "question or signal -> read surface -> grounded answer -> evidence disclosure"
  },
  {
    id: "pages-message",
    layer: "interface",
    name: "Pages And Message",
    line: "Company memory, message threads, publication rails, and feedback closeout.",
    surfaces: ["pages", "message", "message_thread", "changelog"],
    sourceHints: ["pages", "message", "world model"],
    flow: "event -> thread or page projection -> review rail -> published memory"
  },
  {
    id: "operations",
    layer: "interface",
    name: "Operations",
    line: "Runtime telemetry, watch surfaces, and budget-aware local proof.",
    surfaces: ["runtime"],
    sourceHints: ["runtime", "verification", "quality"],
    flow: "runtime signal -> check packet -> watch surface -> next safe action"
  }
];

const layers = [
  {
    id: "foundation",
    name: "Foundation",
    line: "Authority, proof, and capability inventory.",
    color: "blue"
  },
  {
    id: "orchestration",
    name: "Orchestration",
    line: "The loops that move intent into governed work.",
    color: "gold"
  },
  {
    id: "interface",
    name: "Interface",
    line: "The rooms where humans inspect, decide, and follow evidence.",
    color: "green"
  }
];

function compactRoute(route) {
  return {
    method: route.method,
    path: route.local_path,
    operation: route.operation_id,
    receiptBacked: route.receipt_backed === true,
    schema: route.response_schema_path
  };
}

function routesForSurfaces(surfaces) {
  const set = new Set(surfaces);
  return (routes.routes ?? [])
    .filter((route) => set.has(route.surface))
    .map(compactRoute)
    .sort((a, b) => `${a.path}:${a.method}`.localeCompare(`${b.path}:${b.method}`));
}

function sourceEntriesForHints(hints) {
  const lowered = hints.map((hint) => hint.toLowerCase());
  return (sourceMap.entries ?? [])
    .filter((entry) => {
      const text = `${entry.domain} ${entry.source} ${entry.generated}`.toLowerCase();
      return lowered.some((hint) => text.includes(hint));
    })
    .map((entry) => ({
      domain: entry.domain,
      source: entry.source,
      generated: entry.generated,
      check: entry.check
    }))
    .slice(0, 6);
}

function capabilitySummary(agentIds = []) {
  const allowed = new Set(agentIds);
  return capabilityCards
    .filter((card) => allowed.has(card.agent))
    .map((card) => ({
      agent: card.agent,
      ceiling: card.autonomy_ceiling,
      allowed: card.allowed_actions.length,
      forbidden: card.forbidden_actions.length
    }));
}

function componentView(component) {
  const componentRoutes = routesForSurfaces(component.surfaces);
  const writes = componentRoutes.filter((route) => route.method === "POST");
  const receipts = writes.filter((route) => route.receiptBacked);
  return {
    ...component,
    routeCount: componentRoutes.length,
    writeCount: writes.length,
    receiptBackedWrites: receipts.length,
    routes: componentRoutes.slice(0, 10),
    routeOverflow: Math.max(componentRoutes.length - 10, 0),
    sources: sourceEntriesForHints(component.sourceHints ?? []),
    capabilities: capabilitySummary(component.capabilityAgents ?? [])
  };
}

export function adminRegistry() {
  const componentViews = components.map(componentView);
  const layerViews = layers.map((layer) => ({
    ...layer,
    components: componentViews.filter((component) => component.layer === layer.id)
  }));
  const allRoutes = routes.routes ?? [];
  const writeRoutes = allRoutes.filter((route) => route.method === "POST");
  const governedWrites = writeRoutes.filter((route) => route.receipt_backed === true);
  const marketplaceItems = marketplace.capabilities ?? [];

  return {
    routeCount: allRoutes.length,
    writeCount: writeRoutes.length,
    governedWriteCount: governedWrites.length,
    componentCount: componentViews.length,
    sourceCount: sourceMap.entries?.length ?? 0,
    marketplace: {
      count: marketplaceItems.length,
      approved: marketplaceItems.filter((item) => item.status === "approved").length,
      review: marketplaceItems.filter((item) => item.status === "needs_review").length,
      blocked: marketplaceItems.filter((item) => item.status === "blocked").length
    },
    layers: layerViews,
    sources: [
      { label: "Routes", value: "generated/routes/mdx-local-routes.json" },
      { label: "Source map", value: "generated/source-map/mdx-source-map.json" },
      { label: "Marketplace", value: "generated/marketplace/capability-registry.json" },
      { label: "Cards", value: "generated/capability-cards/*.json" }
    ]
  };
}

export function adminRegistryHtml(registry) {
  return registry.layers.map(layerHtml).join("");
}

function layerHtml(layer) {
  const countLabel = layer.components.length === 1 ? "component" : "components";
  return `<section class="registry-layer ${escapeAttr(layer.color)}">
    <header class="registry-layer-head">
      <div>
        <h2>${escapeHtml(layer.name)}</h2>
        <p>${escapeHtml(layer.line)}</p>
      </div>
      <span>${layer.components.length} ${countLabel}</span>
    </header>
    <div class="registry-components">${layer.components.map(componentHtml).join("")}</div>
  </section>`;
}

function componentHtml(component) {
  return `<article class="registry-component">
    <div class="registry-component-main">
      <div>
        <h3>${escapeHtml(component.name)}</h3>
        <p>${escapeHtml(component.line)}</p>
      </div>
      <dl>
        <div><dt>routes</dt><dd>${component.routeCount}</dd></div>
        <div><dt>writes</dt><dd>${component.writeCount}</dd></div>
        <div><dt>receipts</dt><dd>${component.receiptBackedWrites}</dd></div>
      </dl>
    </div>
    <p class="registry-flow">${escapeHtml(component.flow)}</p>
    <div class="registry-chips" aria-label="${escapeAttr(component.name)} surfaces">
      ${component.surfaces.map((surface) => `<code>${escapeHtml(surface)}</code>`).join("")}
    </div>
    <details>
      <summary>routes and checks</summary>
      ${routesHtml(component)}
      ${sourcesHtml(component)}
      ${capabilitiesHtml(component)}
    </details>
  </article>`;
}

function routesHtml(component) {
  if (component.routes.length === 0) {
    return `<p class="registry-muted">No declared route is attached to this component yet.</p>`;
  }
  const rows = component.routes
    .map((route) => {
      const receipt = route.receiptBacked ? `<span class="registry-receipt">receipt</span>` : "";
      return `<li>
        <span class="registry-method ${escapeAttr(route.method.toLowerCase())}">${escapeHtml(route.method)}</span>
        <code>${escapeHtml(route.path)}</code>
        ${receipt}
      </li>`;
    })
    .join("");
  const overflow =
    component.routeOverflow > 0
      ? `<p class="registry-muted">plus ${component.routeOverflow} more in the generated route catalog</p>`
      : "";
  return `<ul class="registry-routes">${rows}</ul>${overflow}`;
}

function sourcesHtml(component) {
  if (component.sources.length === 0) return "";
  return `<ul class="registry-sources">${component.sources
    .map(
      (source) =>
        `<li><span>${escapeHtml(source.domain)}</span><code>${escapeHtml(source.check)}</code></li>`
    )
    .join("")}</ul>`;
}

function capabilitiesHtml(component) {
  if (component.capabilities.length === 0) return "";
  return `<ul class="registry-capabilities">${component.capabilities
    .map(
      (capability) =>
        `<li>
          <code>${escapeHtml(capability.agent)}</code>
          <span>${escapeHtml(capability.ceiling)}</span>
          <span>${capability.allowed} allowed</span>
          <span>${capability.forbidden} forbidden</span>
        </li>`
    )
    .join("")}</ul>`;
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function escapeAttr(value) {
  return escapeHtml(value).replaceAll("`", "&#96;");
}
