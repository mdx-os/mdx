<script>
  import { SOURCE_REPOSITORY_URL } from "./publicSite.js";
</script>

<footer class="public-footer">
  <a class="brand" href="/landing" aria-label="MDx home">MD<span>x</span></a>
  <span class="state">Source is open. Hosted beta is invite-only.</span>
  <nav aria-label="MDx public footer">
    <a href={SOURCE_REPOSITORY_URL}>Open source</a>
    <a href="/downloads">Download</a>
    <a href="/security">Security</a>
    <a href="/waitlist">Request an invite</a>
  </nav>
  <span class="by">Built by MD.</span>
</footer>

<style>
  .public-footer {
    display: grid;
    grid-template-columns: auto minmax(12rem, auto) 1fr auto;
    align-items: center;
    gap: 1.4rem;
    padding: 30px clamp(1rem, 4vw, 2.5rem) 44px;
    background: #0a0a0b;
    border-top: 1px solid #1a1a1d;
    color: #55555c;
    font-size: 0.8rem;
  }

  .brand {
    color: #f0f0f2;
    font-family: var(--mdx-font-display);
    font-size: 1.05rem;
    font-weight: 900;
    letter-spacing: -0.04em;
    text-decoration: none;
  }

  .brand span {
    color: #4285f4;
  }

  nav {
    display: flex;
    justify-content: center;
    gap: 1.25rem;
    flex-wrap: wrap;
  }

  nav a {
    color: #66666e;
    text-decoration: none;
  }

  nav a:hover {
    color: #d6d6dc;
  }

  @media (max-width: 760px) {
    .public-footer {
      grid-template-columns: 1fr auto;
    }

    nav {
      grid-column: 1 / -1;
      grid-row: 2;
      justify-content: flex-start;
    }

    .by {
      text-align: right;
    }
  }
</style>
