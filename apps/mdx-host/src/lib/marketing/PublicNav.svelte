<script>
  import { SOURCE_REPOSITORY_URL } from "./publicSite.js";

  let { active = "" } = $props();

  const links = [
    { id: "product", label: "Product", href: "/landing" },
    { id: "open-source", label: "Open source", href: SOURCE_REPOSITORY_URL },
    { id: "downloads", label: "Download", href: "/downloads" }
  ];
</script>

<header class="public-nav">
  <a class="brand" href="/landing" aria-label="MDx home">MD<span>x</span></a>
  <nav aria-label="MDx public site">
    {#each links as link}
      <a href={link.href} aria-current={active === link.id ? "page" : undefined}>{link.label}</a>
    {/each}
  </nav>
  <a class="signin" href="/auth/sign-in?next=%2Fwelcome%2Fbeta">Sign in</a>
</header>

<style>
  .public-nav {
    position: sticky;
    top: 0;
    z-index: 30;
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 1.5rem;
    min-height: 64px;
    padding: 0 clamp(1rem, 4vw, 2.5rem);
    background: rgba(8, 8, 10, 0.9);
    border-bottom: 1px solid rgba(255, 255, 255, 0.07);
    backdrop-filter: blur(18px);
    -webkit-backdrop-filter: blur(18px);
  }

  .brand {
    color: #f0f0f2;
    font-family: var(--mdx-font-display);
    font-size: 1.15rem;
    font-weight: 900;
    letter-spacing: -0.04em;
    text-decoration: none;
  }

  .brand span {
    color: #4285f4;
  }

  nav {
    display: flex;
    justify-content: flex-end;
    gap: clamp(1rem, 2vw, 1.7rem);
  }

  nav a,
  .signin {
    color: #90909a;
    font-size: 0.84rem;
    font-weight: 650;
    text-decoration: none;
    white-space: nowrap;
    transition: color 160ms ease;
  }

  nav a:hover,
  nav a[aria-current="page"],
  .signin:hover {
    color: #f0f0f2;
  }

  .signin {
    padding-left: 1.5rem;
    border-left: 1px solid rgba(255, 255, 255, 0.09);
  }

  @media (max-width: 760px) {
    .public-nav {
      grid-template-columns: auto 1fr auto;
      gap: 1rem;
      min-height: 58px;
    }

    nav {
      justify-content: flex-end;
      gap: 1rem;
    }

    .signin {
      padding-left: 0;
      border-left: 0;
    }
  }

  @media (max-width: 480px) {
    .public-nav { padding-inline: 0.9rem; }
    nav a { font-size: 0.76rem; }
    nav a:first-child { display: none; }
    .signin { font-size: 0.76rem; }
  }
</style>
