<script>
  // One token system: the generated theme is the single source of truth. The
  // legacy apps/mdx shell theme (kept on disk for that app) used to be imported
  // here too and redefined the SAME tokens with different values - the accent
  // even changed hue between light and dark - so it is no longer imported.
  import "../../../../generated/ui/mdx-theme.css";
  import { browser } from "$app/environment";
  import { afterNavigate, onNavigate } from "$app/navigation";
  import { page } from "$app/state";
  import { onMount, tick } from "svelte";
  import FeedbackButton from "@mdx/ui/components/FeedbackButton.svelte";
  import CommandPalette from "../lib/CommandPalette.svelte";
  import KernelBanner from "../lib/KernelBanner.svelte";
  import { kernelReachability, startKernelHeartbeat, seedReachability } from "../lib/kernelReachability.js";
  import { WEB_APP_VERSION } from "../lib/runNotifications.js";
  import {
    errorEvent,
    interactionTimingEvent,
    routeTimingEvent,
    sessionEndEvent,
    sessionStartEvent,
    surfaceVisitEvent,
    telemetry
  } from "../lib/telemetry.js";
  import { isOperatorSession, sessionPresentation } from "../lib/session.js";
  import { traceMode, toggleTrace, hydrateTrace } from "../lib/traceMode.js";
  import { applyTheme } from "../lib/youProfile.js";

  // The quiet feedback affordance rides the shell, so every product surface
  // (Home/Twin, Message, Pages, Forge, Marketplace) carries it without each
  // page wiring its own. Operator and proof surfaces are out of scope here.
  function feedbackSurfaceFor(pathname) {
    if (pathname === "/") return "home";
    const segment = pathname.split("/")[1];
    // twin was missing from this list, so the home surface itself had no
    // report affordance; the kernel accepts home and native_macos now too.
    return ["twin", "forge", "message", "pages", "marketplace"].includes(segment) ? segment : null;
  }

  // Routes cross-fade instead of swapping: a brief view transition makes
  // the shell feel like one place. Skipped entirely for reduced motion.
  onNavigate((navigation) => {
    if (!document.startViewTransition) {
      return;
    }
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      return;
    }
    return new Promise((resolve) => {
      document.startViewTransition(async () => {
        resolve();
        await navigation.complete;
      });
    });
  });

  let { data, children } = $props();
  let telemetryCount = $state(0);
  let deliveredCount = $state(0);
  let hostStage = $state(null);

  // SvelteKit restores window scroll, but the product shell scrolls its own
  // stage. Reset after the destination DOM commits and once more on the next
  // frame so browser history restoration cannot put the internal scroller
  // back where the previous route left it.
  function resetHostStageScroll() {
    if (!browser) return;
    void tick().then(() =>
      requestAnimationFrame(() => {
        hostStage?.scrollTo(0, 0);
        window.scrollTo(0, 0);
      })
    );
  }
  afterNavigate(resetHostStageScroll);
  onMount(() => {
    history.scrollRestoration = "manual";
    resetHostStageScroll();
  });
  // Is the local kernel answering? One shared, honest signal drives the status
  // dot AND the shell-level banner, so every surface agrees on the truth
  // instead of each page inventing its own dot. Seeded from the server's
  // liveness probe, then kept fresh by a visibility-gated heartbeat.
  const online = $derived($kernelReachability.online);
  // The rail collapses to icons only, the way v1 did; the choice persists.
  let railCollapsed = $state(false);
  // Effective theme for the quick toggle; the full system/light/dark choice
  // still lives on You. Reflects what is actually painted right now.
  let isDark = $state(true);

  // The product rail: the premier apps lead with no heavy header (they are
  // the obvious thing), the company surfaces sit in a quiet secondary group,
  // and the trust ledger is one calm item at the foot. Studio folds into
  // Forge. Pages is now named for what it holds (documents), and Memory is a
  // real rail item: the one place to read what MDx has learned across the
  // surfaces. Learn stays the deeper edit-and-ratify lane, reached from
  // Memory and from Forge's candidate lessons. You is reached from the
  // profile chip below, so it is not duplicated here.
  // Contract drift, enforced (was display-only): a fingerprint change mid-
  // session means the kernel was redeployed underneath this tab; a
  // min_app_version above ours means this web build is too old.
  let versionDrift = $state("");
  // Carry the last real version read: a load whose kernelVersion swallowed to
  // null (a slow or momentarily-down kernel) must not blank out the fingerprint
  // and quietly suppress the "MDx was updated underneath this tab" safety
  // banner. We compare against the last-known-good read instead of nothing.
  let lastGoodVersion = null;
  $effect(() => {
    const kernel = data.kernelVersion ?? lastGoodVersion;
    if (!kernel?.fingerprint) return;
    lastGoodVersion = kernel;
    try {
      const KEY = "mdx.contractFingerprint";
      const seen = localStorage.getItem(KEY);
      if (seen && seen !== kernel.fingerprint) {
        versionDrift = "MDx was updated underneath this tab - refresh to pick up the new contract.";
      }
      localStorage.setItem(KEY, kernel.fingerprint);
    } catch (error) {
      // storage unavailable; drift detection is best-effort
    }
    const min = (kernel.minAppVersion ?? "").split(".").map(Number);
    const mine = WEB_APP_VERSION.split(".").map(Number);
    for (let i = 0; i < Math.max(min.length, mine.length); i += 1) {
      const a = mine[i] ?? 0;
      const b = min[i] ?? 0;
      if (a !== b) {
        if (a < b) versionDrift = `This MDx kernel needs app ${kernel.minAppVersion} or newer - update the web build.`;
        break;
      }
    }
  });

  const navGroups = $derived([
    {
      label: "",
      items: [
        { href: "/twin", label: "Twin", icon: "twin", count: data.needsYouCount },
        { href: "/message", label: "Message", icon: "message", count: data.messageWaitingCount },
        { href: "/forge", label: "Forge", icon: "forge", count: data.forgeNeedsYouCount },
        { href: "/pages", label: "Pages", icon: "pages" },
        { href: "/memory", label: "Memory", icon: "learn" },
        { href: "/marketplace", label: "Marketplace", icon: "marketplace" }
      ]
    },
    {
      label: "Company",
      items: [
        { href: "/strategy", label: "Strategy", icon: "strategy" },
        { href: "/product-direction", label: "Product", icon: "product" },
        { href: "/talent", label: "Talent", icon: "talent" }
      ]
    },
    {
      label: "",
      items: [{ href: "/proof", label: "Proof", icon: "proof" },
        { href: "/help", label: "Help", icon: "help" }]
    }
  ]);

  // The operator Console is a real top-level door, but only for the seats
  // that run the system. Members never see it. The session seat carries the
  // role; today the local owner sees it, a plain member would not.
  const isOperator = $derived(isOperatorSession(data.session));
  const sessionIdentity = $derived(sessionPresentation(data.session));

  function toggleRail() {
    railCollapsed = !railCollapsed;
    try {
      localStorage.setItem("mdx-rail-collapsed", railCollapsed ? "1" : "0");
    } catch (error) {
      // best effort
    }
  }

  function toggleTheme() {
    const next = isDark ? "light" : "dark";
    applyTheme(next);
    isDark = !isDark;
  }

  const feedbackSurface = $derived(feedbackSurfaceFor(page.url.pathname));
  const feedbackAvoidsComposer = $derived(
    page.url.pathname === "/twin" || page.url.pathname === "/message"
  );

  // The console is its own surface: operator pages bring their own shell,
  // so the member rail steps aside entirely on /admin routes.
  const onConsole = $derived(page.url.pathname.startsWith("/admin"));
  // Public-facing pages (the marketing landings and the security trust page)
  // render full-bleed: no product rail, no stage padding.
  const isLanding = $derived(
    page.url.pathname === "/landing" ||
      page.url.pathname === "/forge-product" ||
      page.url.pathname === "/open-source" ||
      page.url.pathname === "/downloads" ||
      page.url.pathname === "/security" ||
      page.url.pathname === "/dev" ||
      page.url.pathname.startsWith("/download/")
  );
  // Onboarding is its own focused world: the activation journey renders full-
  // bleed with a minimal welcome chrome (welcome/+layout), no product rail.
  const onWelcome = $derived(page.url.pathname.startsWith("/welcome"));
  // The public front gate renders bare: a stranger requesting or redeeming
  // an invite must not see the app rail, live badges, or the Console link.
  const onPublicGate = $derived(
    page.url.pathname.startsWith("/waitlist") ||
      page.url.pathname.startsWith("/redeem") ||
      page.url.pathname === "/auth/sign-in"
  );
  const onPublicExperience = $derived(data.publicExperience === true);

  const platformSignals = $derived([
    sessionIdentity.trust,
    `${data.routeSummary.routeCount} declared routes`,
    `${data.routeSummary.governedWriteCount} governed writes`,
    `${data.telemetry.budgets.clientJsCssKb}KB client budget`,
    data.sessionBoundary.actorAdmissionRequired ? "actor admission required" : "actor admission missing"
  ]);

  // Apply the saved trace-mode choice once on the client (server renders off),
  // and read the saved rail + theme state for the controls in the foot.
  $effect(() => {
    hydrateTrace();
    try {
      railCollapsed = localStorage.getItem("mdx-rail-collapsed") === "1";
    } catch (error) {
      railCollapsed = false;
    }
    isDark = document.documentElement.dataset.theme !== "light";
  });

  // Seed the shared reachability store from the server's liveness probe so the
  // first paint already knows the truth (no flash of "online" over a dead
  // kernel), then run a visibility-gated heartbeat that keeps it honest and
  // tells a slow-but-alive kernel apart from one that is genuinely down.
  $effect(() => {
    seedReachability({ reachable: data.reachable, error: data.reachableError });
  });
  $effect(() => {
    if (!browser || onPublicExperience) {
      return;
    }
    return startKernelHeartbeat({ intervalMs: 15000 });
  });

  $effect(() => {
    if (!browser || onPublicExperience) {
      return;
    }

    const route = page.url.pathname;
    const startedAt = performance.now();

    requestAnimationFrame(() => {
      telemetry.record(
        routeTimingEvent({
          route,
          startedAt,
          endedAt: performance.now()
        })
      );
      // Source-B surface visit: which product surface this route belongs to.
      // Safe fields only; identity is server-derived at the sink.
      const surface = feedbackSurfaceFor(route);
      if (surface) {
        telemetry.record(surfaceVisitEvent({ surface, route }));
      }
      telemetryCount = telemetry.size();
    });
  });

  $effect(() => {
    if (!browser || onPublicExperience) {
      return;
    }

    const onError = (event) => {
      telemetry.record(
        errorEvent({
          route: page.url.pathname,
          message: event.message
        })
      );
      telemetryCount = telemetry.size();
    };

    window.addEventListener("error", onError);

    return () => {
      window.removeEventListener("error", onError);
    };
  });

  $effect(() => {
    if (!browser || onPublicExperience) {
      return;
    }
    const recordInteraction = (event) => {
      const startedAt = typeof event.timeStamp === "number" ? event.timeStamp : performance.now();
      requestAnimationFrame(() => {
        telemetry.record(
          interactionTimingEvent({
            route: page.url.pathname,
            durationMs: performance.now() - startedAt,
            interactionKind: event.type
          })
        );
        telemetryCount = telemetry.size();
      });
    };
    window.addEventListener("pointerup", recordInteraction, { passive: true });
    window.addEventListener("keydown", recordInteraction, { passive: true });
    return () => {
      window.removeEventListener("pointerup", recordInteraction);
      window.removeEventListener("keydown", recordInteraction);
    };
  });

  // Buffered events flush to the host's own sink - same origin, no third
  // parties. A best-effort beacon catches the tab closing mid-buffer.
  $effect(() => {
    if (!browser || onPublicExperience) {
      return;
    }

    // Source-B session bounds: one start now, one end on pagehide with the
    // session duration. Safe fields only.
    const sessionStartedAt = performance.now();
    telemetry.record(sessionStartEvent({ route: page.url.pathname }));
    telemetryCount = telemetry.size();

    const interval = setInterval(async () => {
      // Nothing to send, or the tab is hidden - skip the flush tick.
      if (document.visibilityState !== "visible" || telemetry.size() === 0) {
        return;
      }
      await telemetry.flush();
      deliveredCount = telemetry.deliveredCount();
    }, 10000);
    const onPageHide = () => {
      telemetry.record(
        sessionEndEvent({ route: page.url.pathname, durationMs: performance.now() - sessionStartedAt })
      );
      telemetry.beacon();
    };
    window.addEventListener("pagehide", onPageHide);

    return () => {
      clearInterval(interval);
      window.removeEventListener("pagehide", onPageHide);
    };
  });
</script>

<svelte:head>
  <title>MDx</title>
  <meta
    name="description"
    content="MDx frontend host spike with route, session, security, and evidence rails."
  />
</svelte:head>

{#snippet navIcon(id)}
  <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    {#if id === "twin"}<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>
    {:else if id === "message"}<rect x="3" y="5" width="18" height="14" rx="2"/><path d="m3 7 9 6 9-6"/>
    {:else if id === "forge"}<path d="m12 2 9 5-9 5-9-5 9-5Z"/><path d="m3 12 9 5 9-5"/><path d="m3 17 9 5 9-5"/>
    {:else if id === "pages"}<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/>
    {:else if id === "studio"}<path d="M12 20h9"/><path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z"/>
    {:else if id === "learn"}<path d="M4 12a4 4 0 0 1 4-4h1a3 3 0 0 1 6 0h1a4 4 0 0 1 0 8h-1a3 3 0 0 1-6 0H8a4 4 0 0 1-4-4Z"/><path d="M9 8v8M15 8v8M8 12h8"/>
    {:else if id === "strategy"}<path d="M12 2l2.9 6.3 6.6.6-5 4.4 1.5 6.5L12 16.8 6 20.2l1.5-6.5-5-4.4 6.6-.6z"/>
    {:else if id === "product"}<line x1="4" y1="21" x2="4" y2="14"/><line x1="4" y1="10" x2="4" y2="3"/><line x1="12" y1="21" x2="12" y2="12"/><line x1="12" y1="8" x2="12" y2="3"/><line x1="20" y1="21" x2="20" y2="16"/><line x1="20" y1="12" x2="20" y2="3"/><line x1="2" y1="14" x2="6" y2="14"/><line x1="10" y1="8" x2="14" y2="8"/><line x1="18" y1="16" x2="22" y2="16"/>
    {:else if id === "talent"}<path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/>
    {:else if id === "marketplace"}<path d="M3 9l1-5h16l1 5"/><path d="M5 9v10h14V9"/><path d="M9 19v-5h6v5"/>
    {:else if id === "proof"}<circle cx="12" cy="12" r="9"/><path d="m8 12 3 3 5-6"/>
    {:else if id === "you"}<circle cx="12" cy="8" r="4"/><path d="M4 21v-1a6 6 0 0 1 6-6h4a6 6 0 0 1 6 6v1"/>
    {:else if id === "help"}<circle cx="12" cy="12" r="9"/><path d="M9.5 9a2.5 2.5 0 0 1 5 .3c0 1.7-2.5 2.2-2.5 3.7"/><line x1="12" y1="17" x2="12" y2="17.01"/>
    {/if}
  </svg>
{/snippet}
<div class="host-shell" class:console={onConsole || isLanding || onWelcome || onPublicGate} class:pagescroll={isLanding || onPublicGate}>
  <a class="skip-link" href="#main">Skip to content</a>
  <CommandPalette />
  {#if versionDrift}
    <div class="version-banner" role="status">
      {versionDrift} <button type="button" class="vb-refresh" onclick={() => location.reload()}>Refresh</button>
    </div>
  {/if}
  {#if !onConsole && !isLanding && !onWelcome && !onPublicGate}
    <div class="host-kernel-banner">
      <KernelBanner />
    </div>
  {/if}
  {#if !onConsole && !isLanding && !onWelcome && !onPublicGate}
  <aside class="host-rail" class:collapsed={railCollapsed} aria-label="MDx host routes">
    <div class="rail-top">
      <a class="brand rail-label" href="/" aria-label="MDx home">
        <strong>MD<span>x</span></strong>
      </a>
      <button
        type="button"
        class="rail-collapse"
        onclick={toggleRail}
        aria-label={railCollapsed ? "Expand the menu" : "Collapse the menu"}
        title={railCollapsed ? "Expand" : "Collapse"}
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d={railCollapsed ? "m9 18 6-6-6-6" : "m15 18-6-6 6-6"} />
        </svg>
      </button>
    </div>
    <nav>
      {#each navGroups as group}
        {#if group.label}<span class="nav-section rail-label">{group.label}</span>{:else}<span class="nav-gap" aria-hidden="true"></span>{/if}
        {#each group.items as item}
          <a
            class:active={page.url.pathname === item.href}
            href={item.href}
            title={railCollapsed ? item.label : null}
          >
            {@render navIcon(item.icon)}
            <span class="rail-label">{item.label}</span>
            {#if item.count}<span class="nav-count">{item.count}</span>{/if}
          </a>
        {/each}
      {/each}
    </nav>
    {#if isOperator}
      <a
        class="rail-tool"
        class:active={page.url.pathname.startsWith("/admin")}
        href="/admin"
        title={railCollapsed ? "Console" : "Run the system"}
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 6h16M4 12h16M4 18h16"/><circle cx="9" cy="6" r="1.6" fill="currentColor" stroke="none"/><circle cx="15" cy="12" r="1.6" fill="currentColor" stroke="none"/><circle cx="8" cy="18" r="1.6" fill="currentColor" stroke="none"/></svg>
        <span class="rail-label">Console</span>
      </a>
    {/if}
    <button
      type="button"
      class="rail-tool"
      onclick={toggleTheme}
      aria-label={isDark ? "Switch to light mode" : "Switch to dark mode"}
      title={isDark ? "Light mode" : "Dark mode"}
    >
      {#if isDark}
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/></svg>
      {:else}
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8Z"/></svg>
      {/if}
      <span class="rail-label">{isDark ? "Light mode" : "Dark mode"}</span>
    </button>
    <button
      type="button"
      class="trust-shield"
      class:on={$traceMode}
      onclick={toggleTrace}
      aria-pressed={$traceMode}
      title={$traceMode ? "Hide the proof" : "Why you can trust this"}
    >
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M12 2 4 5v6c0 5 3.4 8.6 8 11 4.6-2.4 8-6 8-11V5l-8-3Z" />
        {#if $traceMode}<path d="m9 12 2 2 4-4" />{/if}
      </svg>
      <span class="rail-label">{$traceMode ? "Showing the proof" : "Trust"}</span>
    </button>
    <div class="rail-foot">
      <a class="rail-profile" href="/you" aria-label="You - profile and settings" title={sessionIdentity.title}>
        <span class="rail-avatar" aria-hidden="true">
          {sessionIdentity.avatar}
          <span class="status-dot" class:online aria-hidden="true"></span>
        </span>
        <span class="rail-name rail-label">{sessionIdentity.label}</span>
      </a>
      {#if $traceMode}
        <div class="rail-proof">
          <span>{data.telemetry.release}</span>
          {#each platformSignals as signal}
            <span>{signal}</span>
          {/each}
          <span>{telemetryCount} telemetry events · {deliveredCount} saved by this host</span>
        </div>
      {/if}
    </div>
  </aside>
  {/if}

  <main bind:this={hostStage} id="main" class="host-stage" class:bare={isLanding || onWelcome || onPublicGate}>
    {@render children()}
  </main>

  {#if feedbackSurface && !onWelcome}
    <div class="host-feedback" class:avoid-composer={feedbackAvoidsComposer}>
      <FeedbackButton surface={feedbackSurface} session={data.session} appVersion={data.telemetry.release} />
    </div>
  {/if}
</div>

<style>
  :global(html),
  :global(body) {
    margin: 0;
  }

  :global(body) {
    overflow: hidden;
    background: var(--mdx-surface-base);
  }

  /* Border-box everywhere: without this, every min-height/width with
     padding computes content-box and quietly inflates (the door cards
     rendered 166px from a 128px minimum). The proof shell patched this
     per-component three times; the host fixes it once. */
  :global(*),
  :global(*::before),
  :global(*::after) {
    box-sizing: border-box;
  }

  /* The shell owns the height chain: the stage scrolls internally and
     routes can fill it with height: 100% instead of viewport math that
     drifts whenever the rail wraps (the mobile 92px overflow lesson). */
  .host-shell {
    display: grid;
    grid-template-columns: 184px minmax(0, 1fr);
    grid-template-rows: auto minmax(0, 1fr);
    height: 100vh;
    height: 100dvh;
    color: var(--mdx-text-primary);
    background: var(--mdx-surface-base);
    font-family: var(--mdx-font-body);
  }

  .host-shell.console {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr);
  }

  /* Collapsed rail: the column narrows to an icon strip. */
  .host-shell:has(.host-rail.collapsed) {
    grid-template-columns: 60px minmax(0, 1fr);
  }

  :global(::view-transition-old(root)),
  :global(::view-transition-new(root)) {
    animation-duration: 180ms;
    animation-timing-function: ease-out;
  }

  /* Keyboard focus ring is owned by the theme sheets (one branded ring,
     both themes). Note for proofs: the generated sheet transitions
     outline-color over 120ms, so measure after it settles. */

  .host-rail {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto auto auto;
    gap: 10px;
    border-right: 1px solid var(--mdx-border-subtle);
    padding: 12px 8px;
    background: var(--mdx-surface-sunken);
    transition: none;
  }

  .rail-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 2px 4px 2px 6px;
  }
  .rail-collapse {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--mdx-text-tertiary);
    cursor: pointer;
    flex: none;
  }
  .rail-collapse svg {
    width: 16px;
    height: 16px;
  }
  .rail-collapse:hover {
    color: var(--mdx-text-primary);
    background: var(--mdx-surface-raised);
  }

  .nav-section {
    padding: 8px 10px 2px;
    color: var(--mdx-text-tertiary);
    font-size: 10.5px;
    font-weight: 650;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .nav-icon {
    width: 17px;
    height: 17px;
    flex: none;
  }

  /* Shared shape for the rail's bottom controls (theme, trust). */
  .rail-tool {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 10px;
    border: none;
    border-radius: var(--mdx-radius-md);
    background: transparent;
    color: var(--mdx-text-muted);
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
  }
  .rail-tool svg {
    width: 17px;
    height: 17px;
    flex: none;
  }
  .rail-tool:hover {
    color: var(--mdx-text-primary);
    background: var(--mdx-surface-raised);
  }

  /* Collapsed: icons only. Labels fold away and the rail narrows. */
  .host-rail.collapsed .rail-label {
    display: none;
  }
  .host-rail.collapsed nav a,
  .host-rail.collapsed .rail-tool,
  .host-rail.collapsed .trust-shield,
  .host-rail.collapsed .rail-profile {
    justify-content: center;
    padding-left: 0;
    padding-right: 0;
  }
  .host-rail.collapsed .rail-top {
    justify-content: center;
  }
  .host-rail.collapsed .nav-section {
    height: 8px;
    padding: 0;
    overflow: hidden;
  }

  .brand {
    color: var(--mdx-text-primary);
    text-decoration: none;
    padding: 4px 8px;
  }

  .brand strong {
    font-family: var(--mdx-font-display);
    font-size: 18px;
  }

  .brand span {
    color: var(--mdx-accent-primary);
  }

  /* The system-wide trust shield: quiet by default, lights up when the proof
     is showing. Press it on any page to reveal why the contents can be trusted. */
  .trust-shield {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: transparent;
    color: var(--mdx-text-muted);
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
    transition: color var(--mdx-motion-fast) var(--mdx-ease), border-color var(--mdx-motion-fast) var(--mdx-ease), background var(--mdx-motion-fast) var(--mdx-ease);
  }
  .trust-shield svg {
    width: 15px;
    height: 15px;
    flex: none;
  }
  .trust-shield:hover {
    color: var(--mdx-text-primary);
    border-color: var(--mdx-border-default);
  }
  .trust-shield.on {
    color: var(--mdx-accent-primary);
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 35%, transparent);
    background: color-mix(in srgb, var(--mdx-accent-primary) 8%, transparent);
  }

  nav {
    display: grid;
    gap: 6px;
    align-content: start;
    min-height: 0;
    overflow-y: auto;
  }

  .nav-gap {
    height: 10px;
  }

  nav a {
    display: flex;
    align-items: center;
    gap: 11px;
    border-radius: var(--mdx-radius-md);
    padding: 9px 10px;
    color: var(--mdx-text-muted);
    text-decoration: none;
    font-size: 13.5px;
  }

  .nav-count {
    margin-left: auto;
    font-size: 11px;
    font-weight: 650;
    line-height: 1;
    padding: 3px 7px;
    border-radius: 999px;
    background: var(--mdx-accent-primary);
    color: var(--mdx-text-on-accent, white);
  }
  .host-rail.collapsed .nav-count {
    position: absolute;
    margin: 0;
    transform: translate(10px, -10px);
    padding: 1px 5px;
    font-size: 9px;
  }
  .host-rail.collapsed nav a {
    position: relative;
  }

  nav a.active,
  nav a:hover {
    color: var(--mdx-text-primary);
    background: var(--mdx-surface-raised);
  }

  /* The foot: who you are and whether the system is answering. The profile
     chip opens You (profile and settings); the dot is the live heartbeat. */
  .rail-foot {
    display: grid;
    gap: 8px;
  }
  .rail-profile {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 7px 8px;
    border-radius: var(--mdx-radius-md);
    text-decoration: none;
    color: var(--mdx-text-primary);
  }
  .rail-profile:hover {
    background: var(--mdx-surface-raised);
  }
  .rail-avatar {
    position: relative;
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    flex: none;
    border-radius: 50%;
    background: var(--mdx-accent-primary);
    color: var(--mdx-text-on-accent, white);
    font-size: 11px;
    font-weight: 650;
    letter-spacing: 0.02em;
  }
  .rail-name {
    font-size: 13px;
    font-weight: 600;
    line-height: 1.2;
  }
  /* Presence as a dot on the avatar's corner - one identity, status in the
     corner, instead of the name repeated beside an "Online" word. */
  .status-dot {
    position: absolute;
    right: -1px;
    bottom: -1px;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--mdx-warning-amber, #d9883a);
    border: 2px solid var(--mdx-surface-sunken);
  }
  .status-dot.online {
    background: var(--mdx-success-green, #2faa6a);
  }

  .rail-proof {
    display: grid;
    gap: 6px;
    border-top: 1px solid var(--mdx-border-subtle);
    padding: 10px 8px 0;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .host-stage {
    min-width: 0;
    min-height: 0;
    padding: 24px;
    overflow: auto;
  }
  .host-stage.bare {
    padding: 0;
  }

  :global(body:has(.host-shell.pagescroll)) {
    overflow: auto;
  }

  .host-shell.pagescroll {
    height: auto;
    min-height: 100dvh;
  }

  .host-shell.pagescroll .host-stage {
    overflow: visible;
  }

  /* The shared kernel banner is a full-width, content-sized row above the
     product. The second row owns the remaining height. Without explicit rows,
     CSS Grid stretches an empty auto row on short routes and creates a large,
     route-dependent gap above the entire app. */
  .host-kernel-banner {
    grid-column: 1 / -1;
  }

  /* Quiet, out of the way: the affordance sits in the corner of the stage and
     never competes with the first viewport. */
  .host-feedback {
    position: fixed;
    right: 18px;
    bottom: 16px;
    z-index: 30;
  }
  .host-feedback.avoid-composer {
    bottom: 124px;
  }

  @media (max-width: 860px) {
    .host-feedback {
      right: 12px;
      bottom: 12px;
    }
    .host-feedback.avoid-composer {
      bottom: 126px;
    }
  }

  @media (max-width: 860px) {
    .host-shell {
      grid-template-columns: 1fr;
      /* Product routes always contribute the quiet kernel banner, the rail,
         and the stage. Give each child an explicit row so the rail never
         collapses to zero height and paints over the route underneath it. */
      grid-template-rows: auto auto minmax(0, 1fr);
    }

    .host-shell.console {
      grid-template-rows: minmax(0, 1fr);
    }

    .host-shell:has(.host-rail.collapsed) {
      grid-template-columns: 1fr;
    }

    .host-rail {
      grid-template-columns: auto minmax(0, 1fr) auto auto auto auto;
      grid-template-rows: auto;
      align-items: center;
      gap: 6px;
      padding: 8px 10px;
      border-right: 0;
      border-bottom: 1px solid var(--mdx-border-subtle);
    }

    .rail-top {
      padding: 0;
    }

    .rail-collapse,
    nav .nav-section,
    nav .nav-gap {
      display: none;
    }

    nav {
      display: flex;
      overflow-x: auto;
      gap: 2px;
      min-width: 0;
      scrollbar-width: none;
    }

    nav::-webkit-scrollbar {
      display: none;
    }

    nav a {
      flex: 0 0 auto;
      min-height: 34px;
      padding: 7px 9px;
      white-space: nowrap;
    }

    .host-rail.collapsed .rail-top .rail-label,
    .host-rail.collapsed nav a .rail-label {
      display: inline;
    }

    .host-rail.collapsed nav a {
      justify-content: flex-start;
      padding-left: 9px;
      padding-right: 9px;
    }

    .host-rail > .rail-tool,
    .host-rail > .trust-shield,
    .host-rail .rail-profile {
      width: 34px;
      height: 34px;
      justify-content: center;
      padding: 0;
    }

    .host-rail > .rail-tool .rail-label,
    .host-rail > .trust-shield .rail-label,
    .host-rail .rail-name {
      display: none;
    }

    .rail-foot {
      min-width: 34px;
    }

    .rail-proof {
      grid-column: 1 / -1;
      display: flex;
      flex-wrap: wrap;
    }
  }

  @media (max-width: 520px) {
    .host-rail {
      display: block;
      padding: 6px 8px;
      overflow: hidden;
    }
    .rail-top,
    .nav-section,
    .nav-gap,
    .rail-tool,
    .trust-shield,
    .rail-foot {
      display: none;
    }
    nav {
      gap: 4px;
      padding-bottom: 2px;
    }
    .host-stage { padding: 14px 12px; }
    .host-feedback { display: none; }
  }

  .skip-link { position: absolute; left: -9999px; top: 10px; z-index: 200; padding: 8px 14px; border-radius: 8px; background: var(--mdx-surface-base); border: 1px solid var(--mdx-border-subtle); color: var(--mdx-text-primary); font-size: 13px; }
  .skip-link:focus-visible { left: 10px; }
  /* Referenced-but-undefined tokens painted hardcoded fallbacks (three reds
     on one surface); alias them to the real palette in one place. The faint
     tier failed contrast at 1.73:1 - it now reads as muted, which passes AA. */
  :global(:root) {
    --mdx-accent-danger: var(--mdx-accent-error);
    --mdx-on-accent: #ffffff;
    --mdx-text-faint: var(--mdx-text-muted);
  }
  /* One interaction-motion rule: hovers ease instead of snapping (173 hover
     rules, 60 transitions before this). The theme's reduced-motion kill
     switch still zeroes everything for users who ask. */
  :global(button, a, [role="button"], summary, input, select, textarea) {
    transition: background-color 120ms ease, border-color 120ms ease, color 120ms ease, opacity 120ms ease, box-shadow 120ms ease;
  }
  .version-banner { position: fixed; top: 10px; left: 50%; transform: translateX(-50%); z-index: 120; display: flex; align-items: center; gap: 10px; padding: 8px 14px; border-radius: 999px; font-size: 12.5px; color: var(--mdx-text-primary); background: var(--mdx-surface-base); border: 1px solid color-mix(in srgb, var(--mdx-accent-warning, #b45309) 45%, transparent); box-shadow: 0 6px 24px color-mix(in srgb, black 25%, transparent); }
  .vb-refresh { font: inherit; font-size: 12px; border: none; border-radius: 999px; padding: 3px 10px; background: var(--mdx-accent-primary); color: var(--mdx-on-accent); cursor: pointer; }
</style>
