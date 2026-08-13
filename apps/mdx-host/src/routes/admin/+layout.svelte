<script>
  // The console: where the people running the system work. Its own shell
  // on purpose - members never see it, and operators get a rail built
  // for their job: health first, then trust controls, then setup, then
  // the per-app dials. The member app is one click back.
  import { page } from "$app/state";

  let { children } = $props();

  const groups = [
    {
      name: "Beta",
      items: [{ href: "/admin/beta", label: "Beta" }]
    },
    {
      name: "Health",
      items: [
        { href: "/admin/watchtower", label: "Watchtower" },
        { href: "/admin/operations", label: "Operations" },
        { href: "/admin/evidence", label: "Evidence" },
        { href: "/admin/models", label: "Models" },
        { href: "/admin/forge", label: "Forge" }
      ]
    },
    {
      name: "Trust & access",
      items: [
        { href: "/admin/access", label: "Access" },
        { href: "/admin/aegis", label: "Security" },
        { href: "/admin/guardrails", label: "Guardrails" },
        { href: "/admin/gate", label: "Gate" },
        { href: "/admin/charter", label: "Charter" }
      ]
    },
    {
      name: "Setup & platform",
      items: [
        { href: "/admin/start", label: "Getting started" },
        { href: "/admin/setup", label: "Setup" },
        { href: "/admin/platform", label: "Platform" },
        { href: "/admin/developer", label: "Developer" },
        { href: "/admin/registry", label: "Registry" }
      ]
    },
    {
      name: "App controls",
      items: [
        { href: "/admin/pages", label: "Pages" },
        { href: "/admin/twin", label: "Twin" },
        { href: "/admin/message", label: "Message" }
      ]
    }
  ];
</script>

<div class="console-shell">
  <aside class="console-rail" aria-label="Console areas">
    <a class="back-link" href="/">&larr; Back to the app</a>
    <p class="console-mark">Console</p>
    {#each groups as group}
      <p class="group-name">{group.name}</p>
      <nav aria-label={group.name}>
        {#each group.items as item}
          <a class:active={page.url.pathname === item.href} href={item.href}>{item.label}</a>
        {/each}
      </nav>
    {/each}
  </aside>
  <main class="console-stage">
    {@render children()}
  </main>
</div>

<style>
  .console-shell {
    display: grid;
    grid-template-columns: 210px minmax(0, 1fr);
    min-height: 100vh;
    min-height: 100dvh;
  }

  .console-rail {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 18px 14px;
    border-right: 1px solid var(--mdx-border-subtle);
    overflow-y: auto;
  }

  .back-link {
    font-size: 12.5px;
    color: var(--mdx-text-muted);
    text-decoration: none;
    padding: 4px 10px;
  }

  .back-link:hover {
    color: var(--mdx-text-primary);
  }

  .console-mark {
    margin: 10px 0 6px;
    padding: 0 10px;
    font-family: var(--mdx-font-display);
    font-size: 16px;
    font-weight: 700;
  }

  .group-name {
    margin: 14px 0 2px;
    padding: 0 10px;
    font-size: 10.5px;
    font-weight: 650;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--mdx-text-muted);
  }

  nav {
    display: grid;
    gap: 1px;
  }

  nav a {
    padding: 7px 10px;
    border-radius: var(--mdx-radius-md);
    font-size: 13.5px;
    color: var(--mdx-text-muted);
    text-decoration: none;
  }

  nav a.active,
  nav a:hover {
    color: var(--mdx-text-primary);
    background: var(--mdx-surface-raised);
  }

  .console-stage {
    min-width: 0;
    padding: 26px 30px 60px;
    overflow-y: auto;
  }

  @media (max-width: 720px) {
    .console-shell {
      grid-template-columns: 1fr;
    }

    .console-rail {
      padding: 14px;
      border-right: 0;
      border-bottom: 1px solid var(--mdx-border-subtle);
      overflow: visible;
    }

    nav {
      display: flex;
      flex-wrap: wrap;
      gap: 4px;
    }

    nav a {
      padding: 6px 9px;
    }

    .console-stage {
      padding: 22px 18px 48px;
      overflow: visible;
    }
  }
</style>
