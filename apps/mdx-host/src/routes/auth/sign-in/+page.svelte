<script>
  let { data } = $props();
  const next = $derived(encodeURIComponent(data.next));
</script>

<svelte:head><title>Sign in - MDx</title></svelte:head>

<main class="signin-shell">
  <a class="brand" href="/landing" aria-label="MDx home">MD<span>x</span></a>
  <section class="signin-card">
    <p class="eyebrow">Private beta</p>
    <h1>Sign in to MDx</h1>
    <p class="intro">{data.guidance.intro}</p>
    <div class="choices">
      <a class="choice google" href={`/auth/login?provider=google&next=${next}`}>Continue with Google</a>
      <a class="choice apple" href={`/auth/login?provider=apple&next=${next}`}>
        <span aria-hidden="true">&#63743;</span> Continue with Apple
      </a>
    </div>
    <p class="privacy">{data.guidance.identity}</p>
    <a class="back" href="/landing">Back to MDx</a>
  </section>
</main>

<style>
  :global(body) { margin: 0; background: #090b10; color: #f7f8fb; }
  .signin-shell { min-height: 100vh; display: grid; align-content: center; justify-items: center; gap: 24px; padding: 32px 20px; box-sizing: border-box; font-family: var(--mdx-font-body); }
  .brand { font-family: var(--mdx-font-display); font-size: 25px; font-weight: 900; letter-spacing: -0.04em; color: inherit; text-decoration: none; }
  .brand span { color: #8b7cff; }
  .signin-card { width: min(100%, 420px); box-sizing: border-box; padding: 34px; border: 1px solid rgba(255,255,255,.11); border-radius: 22px; background: rgba(255,255,255,.055); box-shadow: 0 26px 80px rgba(0,0,0,.38); }
  .eyebrow { margin: 0 0 11px; color: #a89cff; font-size: 12px; font-weight: 800; letter-spacing: .1em; text-transform: uppercase; }
  h1 { margin: 0; font-family: var(--mdx-font-display); font-size: 30px; letter-spacing: -.03em; }
  .intro { margin: 12px 0 24px; color: #b7bac5; font-size: 14.5px; line-height: 1.55; }
  .choices { display: grid; gap: 11px; }
  .choice { min-height: 48px; display: flex; align-items: center; justify-content: center; gap: 8px; padding: 0 18px; border-radius: 12px; font-weight: 750; text-decoration: none; box-sizing: border-box; }
  .google { background: #ffffff; color: #15171c; }
  .apple { background: #000000; color: #ffffff; border: 1px solid rgba(255,255,255,.2); }
  .apple span { font-family: -apple-system, BlinkMacSystemFont, sans-serif; font-size: 19px; }
  .choice:focus-visible, .back:focus-visible { outline: 3px solid #8b7cff; outline-offset: 3px; }
  .privacy { margin: 18px 0 0; color: #8e919c; font-size: 12px; line-height: 1.5; }
  .back { display: inline-block; margin-top: 22px; color: #b7bac5; font-size: 13px; text-underline-offset: 3px; }
  @media (max-width: 520px) { .signin-card { padding: 27px 22px; border-radius: 18px; } }
</style>
