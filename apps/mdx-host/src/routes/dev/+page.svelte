<script>
  import { onMount } from "svelte";

  let { data } = $props();

  const map = $derived(data.map);
  const posture = $derived(data.posture);

  const accessCards = $derived([
    {
      key: "API",
      title: "HTTP API",
      state: `${posture.routeCount} declared routes`,
      body:
        "Use MDx through the same local routes the host reads. Writes go through policy and save proof instead of disappearing into a black box.",
      sample: `curl http://127.0.0.1:18890/health\ncurl http://127.0.0.1:18890/activation/projection.json`
    },
    {
      key: "MCP",
      title: "MCP tools",
      state: "same truth, smaller context",
      body:
        "Expose read, search, and governed action tools to coding agents without making them scrape UI or invent state.",
      sample: `mdx mcp serve --scope local\nmdx mcp tools --surface forge`
    },
    {
      key: "CLI",
      title: "CLI",
      state: "operator-grade commands",
      body:
        "Run proof, inspect work, and hand safe tasks to agents from the terminal while keeping the same checks reviewers expect.",
      sample: `make local-smoke\nmake agent-context-spine-check\nmdx runs inspect latest`
    },
    {
      key: "Apps",
      title: "Native clients",
      state: "iOS, iPadOS, macOS next",
      body:
        "The Apple apps should be clients of the same API and MCP rails, not a second product with a second truth store.",
      sample: `MDxKit.connect()\nawait twin.ask(\"What needs me?\")\nawait forge.requestBuild(scope)`
    }
  ]);

  const buildSteps = [
    "Ask Twin to turn intent into a scoped plan",
    "Route the work to Forge with a boundary",
    "Run checks and collect proof as the work moves",
    "Hold ship decisions for a human until authority is explicit"
  ];

  let canvas;
  let ctx;
  let raf = 0;
  let lastTime = 0;
  let reducedMotion = false;
  let player;
  let obstacles = [];
  let receipts = [];
  let clouds = [];
  let particles = [];
  let nextObstacle = 0;
  let nextReceipt = 0;
  let game = $state({
    running: false,
    held: false,
    score: 0,
    best: 0,
    message: "Press space or tap start"
  });

  function resetWorld() {
    player = {
      x: 88,
      y: 222,
      size: 34,
      velocity: 0,
      grounded: true,
      tilt: 0,
      stride: 0
    };
    obstacles = [
      { x: 980, y: 217, w: 48, h: 39, label: "open write", tone: "danger" },
      { x: 1420, y: 202, w: 62, h: 54, label: "no policy", tone: "warn" }
    ];
    receipts = [
      { x: 520, y: 164, size: 24, collected: false, phase: 0 },
      { x: 860, y: 142, size: 24, collected: false, phase: 1.2 }
    ];
    particles = [];
    clouds = [
      { x: 140, y: 68, w: 96, speed: 18, tone: "cyan" },
      { x: 520, y: 46, w: 136, speed: 11, tone: "green" },
      { x: 810, y: 90, w: 86, speed: 14, tone: "amber" }
    ];
    nextObstacle = 680;
    nextReceipt = 520;
  }

  function startGame() {
    resetWorld();
    game.running = true;
    game.held = false;
    game.score = 0;
    game.message = "Collect receipts. Avoid unsafe authority.";
    lastTime = performance.now();
    cancelAnimationFrame(raf);
    raf = requestAnimationFrame(tick);
  }

  function holdRun(reason) {
    game.running = false;
    game.held = true;
    game.message = reason;
    if (game.score > game.best) {
      game.best = game.score;
      try {
        localStorage.setItem("mdx-dev-run-best", String(game.best));
      } catch (error) {
        // Best score persistence is optional.
      }
    }
    draw();
  }

  function jump() {
    if (!game.running) {
      startGame();
      return;
    }
    if (!player.grounded) return;
    player.velocity = -10.7;
    player.grounded = false;
    player.tilt = -0.2;
    pop(player.x + 22, player.y + 30, "#22d3ee", 4);
  }

  function tick(time) {
    const dt = Math.min((time - lastTime) / 16.67, 2);
    lastTime = time;
    update(dt);
    draw();
    if (game.running) {
      raf = requestAnimationFrame(tick);
    }
  }

  function update(dt) {
    const speed = 4.65 + Math.min(game.score / 1000, 2.6);
    game.score += Math.floor(dt * 1.3);
    player.velocity += 0.52 * dt;
    player.y += player.velocity * dt;
    player.stride += dt * (game.running ? 0.18 : 0.04);
    player.tilt = player.grounded ? 0 : Math.max(-0.2, Math.min(0.28, player.velocity / 34));
    if (player.y >= 222) {
      player.y = 222;
      player.velocity = 0;
      player.grounded = true;
      player.tilt = 0;
    }

    for (const cloud of clouds) {
      cloud.x -= cloud.speed * dt * 0.16;
      if (cloud.x + cloud.w < -20) cloud.x = 980 + Math.random() * 140;
    }

    for (const obstacle of obstacles) {
      obstacle.x -= speed * dt;
    }
    for (const receipt of receipts) {
      receipt.x -= speed * dt;
      receipt.phase += dt * 0.08;
    }
    for (const particle of particles) {
      particle.x += particle.vx * dt;
      particle.y += particle.vy * dt;
      particle.vy += 0.22 * dt;
      particle.life -= dt;
    }

    if (obstacles.length === 0 || obstacles[obstacles.length - 1].x < 760 - nextObstacle) {
      const variants = [
        { w: 48, h: 38, label: "open write", tone: "danger" },
        { w: 62, h: 54, label: "no policy", tone: "warn" },
        { w: 54, h: 44, label: "no proof", tone: "danger" },
        { w: 72, h: 28, label: "wide scope", tone: "warn" }
      ];
      const variant = variants[Math.floor(Math.random() * variants.length)];
      obstacles.push({ x: 1040 + Math.random() * 220, y: 256 - variant.h, ...variant });
      nextObstacle = 0 + Math.random() * 160;
    }
    if (receipts.length === 0 || receipts[receipts.length - 1].x < 820 - nextReceipt) {
      receipts.push({
        x: 1060 + Math.random() * 180,
        y: 126 + Math.random() * 62,
        size: 24,
        collected: false,
        phase: Math.random() * 3
      });
      nextReceipt = 0 + Math.random() * 190;
    }

    obstacles = obstacles.filter((obstacle) => obstacle.x + obstacle.w > -30);
    receipts = receipts.filter((receipt) => receipt.x + receipt.size > -30 && !receipt.collected);
    particles = particles.filter((particle) => particle.life > 0);

    const playerBox = { x: player.x + 5, y: player.y + 4, w: player.size - 8, h: player.size - 6 };
    for (const obstacle of obstacles) {
      if (intersects(playerBox, obstacle)) {
        holdRun("Run held. Authority needs a receipt.");
        return;
      }
    }
    for (const receipt of receipts) {
      if (intersects(playerBox, { x: receipt.x, y: receipt.y, w: receipt.size, h: receipt.size })) {
        receipt.collected = true;
        game.score += 75;
        game.message = "Receipt saved. Faster now.";
        pop(receipt.x + receipt.size / 2, receipt.y + receipt.size / 2, "#34d399", 10);
      }
    }
  }

  function pop(x, y, color, count) {
    for (let i = 0; i < count; i += 1) {
      particles.push({
        x,
        y,
        vx: (Math.random() - 0.5) * 4.6,
        vy: -Math.random() * 3.8 - 0.4,
        life: 22 + Math.random() * 16,
        color
      });
    }
  }

  function intersects(a, b) {
    return a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y;
  }

  function pixelText(text, x, y, size = 13, color = "#f5f5f7", align = "left") {
    ctx.save();
    ctx.fillStyle = color;
    ctx.font = `${size}px "JetBrains Mono", ui-monospace, monospace`;
    ctx.textAlign = align;
    ctx.textBaseline = "top";
    ctx.fillText(text, x, y);
    ctx.restore();
  }

  function drawCloud(cloud) {
    const fill =
      cloud.tone === "green"
        ? "rgba(52, 211, 153, 0.32)"
        : cloud.tone === "amber"
          ? "rgba(251, 191, 36, 0.24)"
          : "rgba(34, 211, 238, 0.25)";
    ctx.fillStyle = fill;
    const y = cloud.y;
    ctx.fillRect(cloud.x, y + 22, cloud.w, 12);
    ctx.fillRect(cloud.x + 18, y + 12, cloud.w * 0.5, 12);
    ctx.fillRect(cloud.x + 42, y + 4, cloud.w * 0.34, 12);
    ctx.fillStyle = "rgba(245,245,247,.16)";
    ctx.fillRect(cloud.x + cloud.w - 24, y + 26, 18, 4);
  }

  function drawPlayer() {
    ctx.save();
    ctx.translate(player.x + player.size / 2, player.y + player.size / 2);
    ctx.rotate(player.tilt);
    ctx.translate(-player.size / 2, -player.size / 2);
    const foot = Math.sin(player.stride) > 0 ? 2 : -2;
    ctx.fillStyle = "rgba(52,211,153,.28)";
    ctx.fillRect(-4, 30, 40, 4);
    ctx.fillStyle = "#34d399";
    ctx.fillRect(3, 6, 28, 25);
    ctx.fillStyle = "#22d3ee";
    ctx.fillRect(7, 1, 20, 7);
    ctx.fillStyle = "#0a0a0b";
    ctx.fillRect(9, 13, 5, 5);
    ctx.fillRect(21, 13, 5, 5);
    ctx.fillRect(11, 23, 14, 3);
    ctx.fillStyle = "#f5f5f7";
    ctx.fillRect(0, 10, 4, 18);
    ctx.fillRect(30, 10, 4, 18);
    ctx.fillStyle = "#fbbf24";
    ctx.fillRect(7, 31, 8, 3 + foot);
    ctx.fillRect(20, 31, 8, 3 - foot);
    ctx.restore();
  }

  function drawReceipt(receipt) {
    const bob = Math.sin(receipt.phase) * 4;
    ctx.fillStyle = "#f5f5f7";
    ctx.fillRect(receipt.x, receipt.y + bob, receipt.size, receipt.size);
    ctx.fillStyle = "#0a0a0b";
    ctx.fillRect(receipt.x + 5, receipt.y + 6 + bob, 12, 2);
    ctx.fillRect(receipt.x + 5, receipt.y + 12 + bob, 9, 2);
    ctx.fillStyle = "#34d399";
    ctx.fillRect(receipt.x + 14, receipt.y + 15 + bob, 4, 4);
  }

  function drawObstacle(obstacle) {
    ctx.fillStyle = "rgba(0,0,0,.28)";
    ctx.fillRect(obstacle.x + 5, obstacle.y + 5, obstacle.w, obstacle.h);
    ctx.fillStyle = obstacle.tone === "warn" ? "#fbbf24" : "#fb7185";
    ctx.fillRect(obstacle.x, obstacle.y, obstacle.w, obstacle.h);
    ctx.fillStyle = "#0a0a0b";
    ctx.fillRect(obstacle.x + 7, obstacle.y + 8, Math.max(10, obstacle.w - 14), 4);
    ctx.fillRect(obstacle.x + 7, obstacle.y + 18, Math.max(8, obstacle.w - 20), 4);
    ctx.fillStyle = "rgba(245,245,247,.42)";
    ctx.fillRect(obstacle.x + obstacle.w - 10, obstacle.y + 4, 4, obstacle.h - 8);
    pixelText(obstacle.label, obstacle.x + obstacle.w / 2, obstacle.y - 18, 10, "rgba(245,245,247,.72)", "center");
  }

  function drawGround() {
    ctx.fillStyle = "#f5f5f7";
    ctx.fillRect(0, 256, 960, 4);
    ctx.fillStyle = "rgba(34, 211, 238, 0.26)";
    for (let x = -10 - (game.score % 82); x < 1040; x += 82) {
      ctx.fillRect(x, 282, 34, 4);
      ctx.fillRect(x + 48, 304, 24, 4);
    }
    ctx.fillStyle = "rgba(52,211,153,.24)";
    ctx.fillRect(0, 260, 960, 7);
    ctx.fillStyle = "rgba(0,0,0,.22)";
    ctx.fillRect(0, 267, 960, 53);
  }

  function draw() {
    if (!ctx) return;
    ctx.clearRect(0, 0, 960, 320);
    const gradient = ctx.createLinearGradient(0, 0, 960, 320);
    gradient.addColorStop(0, "#1000b8");
    gradient.addColorStop(0.48, "#0927a8");
    gradient.addColorStop(1, "#05261f");
    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, 960, 320);

    ctx.fillStyle = "rgba(245,245,247,.08)";
    for (let x = 0; x < 960; x += 24) {
      ctx.fillRect(x, 0, 1, 320);
    }
    for (let y = 0; y < 320; y += 24) {
      ctx.fillRect(0, y, 960, 1);
    }

    for (const cloud of clouds) drawCloud(cloud);
    pixelText("policy", 92, 48, 14, "rgba(245,245,247,.24)");
    pixelText("proof", 398, 88, 14, "rgba(245,245,247,.24)");
    pixelText("human", 660, 66, 14, "rgba(245,245,247,.24)");
    drawGround();
    for (const receipt of receipts) drawReceipt(receipt);
    for (const obstacle of obstacles) drawObstacle(obstacle);
    for (const particle of particles) {
      ctx.fillStyle = particle.color;
      ctx.fillRect(particle.x, particle.y, 4, 4);
    }
    drawPlayer();

    ctx.fillStyle = "rgba(0,0,0,.28)";
    ctx.fillRect(698, 20, 220, 54);
    pixelText("SCORE", 720, 30, 13, "rgba(245,245,247,.58)");
    pixelText(String(game.score).padStart(5, "0"), 720, 52, 16, "#f5f5f7");
    pixelText("BEST", 842, 30, 13, "rgba(245,245,247,.58)");
    pixelText(String(game.best).padStart(5, "0"), 842, 52, 16, "#f5f5f7");

    if (!game.running) {
      ctx.fillStyle = "rgba(0,0,0,.38)";
      ctx.fillRect(304, 104, 352, 92);
      pixelText("> mdx run", 480, 124, 34, "#f5f5f7", "center");
      pixelText(game.message, 480, 172, 16, "rgba(245,245,247,.82)", "center");
    }
  }

  function devKeys(event) {
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    const target = event.target;
    if (target?.tagName === "INPUT" || target?.tagName === "TEXTAREA" || target?.isContentEditable) return;
    const jump = { a: "api", m: "mcp", c: "cli", n: "native", g: "game" }[event.key.toLowerCase()];
    if (jump) document.getElementById(jump)?.scrollIntoView({ behavior: "smooth", block: "start" });
  }

  onMount(() => {
    ctx = canvas.getContext("2d");
    ctx.imageSmoothingEnabled = false;
    reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    try {
      game.best = Number(localStorage.getItem("mdx-dev-run-best") ?? 0);
    } catch (error) {
      game.best = 0;
    }
    resetWorld();
    draw();

    const onKeyDown = (event) => {
      const tag = event.target?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      if (event.code === "Space" || event.key === " ") {
        event.preventDefault();
        jump();
      }
    };
    window.addEventListener("keydown", onKeyDown);

    if (!reducedMotion) {
      const demo = setTimeout(() => {
        if (!game.running && !game.held) draw();
      }, 250);
      return () => {
        clearTimeout(demo);
        window.removeEventListener("keydown", onKeyDown);
        cancelAnimationFrame(raf);
      };
    }

    return () => {
      window.removeEventListener("keydown", onKeyDown);
      cancelAnimationFrame(raf);
    };
  });
</script>

<svelte:window onkeydown={devKeys} />

<svelte:head>
  <title>MDx Developer Platform</title>
  <meta
    name="description"
    content="Build against MDx through API, MCP, CLI, and future native app clients with governed receipts and human authority."
  />
</svelte:head>

<main class="dev-page" data-route-state="ready">
  <header class="dev-nav">
    <a class="wordmark" href="/landing" aria-label="MDx home"><span>MD</span><b>x</b></a>
    <nav aria-label="Developer sections">
      <a href="#api">[A] API</a>
      <a href="#mcp">[M] MCP</a>
      <a href="#cli">[C] CLI</a>
      <a href="#native">[N] Native</a>
      <a href="#game">[G] Game</a>
    </nav>
    <a class="nav-button" href="/welcome">Start</a>
  </header>

  <section class="hero">
    <div class="hero-copy">
      <p class="eyebrow">/dev/mdx</p>
      <h1>Developers</h1>
      <p class="hero-sub">
        Not another agent dashboard. Build through API, MCP, CLI, and native clients
        that all hit the same MDx rails: policy first, proof always, human authority
        when it matters.
      </p>
      <div class="install-line" aria-label="Local start command">
        <span>$</span>
        <code>pnpm --dir apps/mdx-host dev</code>
        <button type="button" aria-label="Copy command">⌘C</button>
      </div>
      <div class="hero-actions">
        <a class="primary" href="#api">Start building</a>
        <a class="secondary" href="#game">Play MDx Run</a>
      </div>
    </div>

    <div class="hero-panel" aria-label="MDx developer preview">
      <div class="window-bar">
        <span></span><span></span><span></span>
        <strong>company.ops.mdx</strong>
      </div>
      <div class="pixel-workbench" aria-hidden="true">
        <div class="pixel-rail">
          <i></i><i></i><i></i><i></i>
        </div>
        <div class="pixel-board">
          <span class="pixel-title">Forge run</span>
          <div class="pixel-row wide"></div>
          <div class="pixel-row"></div>
          <div class="pixel-cells">
            <i></i><i></i><i></i><i></i>
          </div>
        </div>
        <div class="pixel-agent">
          <span></span>
          <b></b>
        </div>
        <div class="pixel-receipt one"></div>
        <div class="pixel-receipt two"></div>
      </div>
      <div class="hero-stats">
        <div>
          <span class="term-label">API</span>
          <strong>{posture.routeCount}</strong>
          <small>declared routes</small>
        </div>
        <div>
          <span class="term-label">Writes</span>
          <strong>{posture.governedWriteCount}</strong>
          <small>save proof</small>
        </div>
        <div>
          <span class="term-label">Queue</span>
          <strong>{posture.workItemCount}</strong>
          <small>scoped slices</small>
        </div>
      </div>
    </div>
  </section>

  <!-- The proof band: what governed actually means, in the wire format.
       Live texture, not marketing - this refusal and this receipt are the
       real shapes the kernel speaks. -->
  <section class="proof-band" aria-label="Governance you can curl">
    <div class="section-copy">
      <p class="eyebrow">The difference</p>
      <h2>Every action is governed. Every outcome is proof.</h2>
      <p>Ask MDx to act without authority and it refuses - politely, in JSON, with a receipt. That is not an error page. That is the product.</p>
    </div>
    <div class="proof-panels">
      <div class="proof-term">
        <div class="window-bar"><span></span><span></span><span></span><strong>refusal, live</strong></div>
        <pre><code>$ curl -X POST :18890/marketplace/installs.json -d '&#123;&#125;'
&#123;
  "status": "REFUSED",
  "reason": "say who is acting - every act lands
             with its actor, never a default",
  "recorded": false
&#125;</code></pre>
      </div>
      <div class="proof-toasts" aria-hidden="true">
        <div class="dev-toast">
          <span class="dt-glyph">▣</span>
          <div><strong>Forge &gt; run:done</strong><span>5 files changed, 2 checks passed, branch ready.</span></div>
        </div>
        <div class="dev-toast">
          <span class="dt-glyph">✓</span>
          <div><strong>Kernel &gt; receipt:signed</strong><span>forge_run_receipt_077473 · on the ledger.</span></div>
        </div>
        <div class="dev-toast">
          <span class="dt-glyph">✕</span>
          <div><strong>Policy &gt; write:refused</strong><span>outside scope - stopped, receipted, explained.</span></div>
        </div>
      </div>
    </div>
  </section>

  <!-- The blueprint: the whole system in one diagram, mono and honest. -->
  <section class="blueprint" aria-label="How MDx fits together">
    <div class="bp-grid">
      <div class="bp-col">
        <p class="bp-head">Outside</p>
        <div class="bp-box">Your repos<span>GitHub, Bitbucket, local</span></div>
        <div class="bp-box">Your agents<span>Claude, Codex, yours</span></div>
      </div>
      <div class="bp-arrow" aria-hidden="true">→</div>
      <div class="bp-col">
        <p class="bp-head">Doors</p>
        <div class="bp-box solid">API / MCP / CLI<span>the same governed routes</span></div>
      </div>
      <div class="bp-arrow" aria-hidden="true">→</div>
      <div class="bp-col">
        <p class="bp-head">Kernel</p>
        <div class="bp-box solid">Policy · Ledger · Receipts<span>fail-closed, signed, durable</span></div>
      </div>
      <div class="bp-arrow" aria-hidden="true">→</div>
      <div class="bp-col">
        <p class="bp-head">Surfaces</p>
        <div class="bp-box">Twin thinks</div>
        <div class="bp-box">Forge builds</div>
        <div class="bp-box">Message coordinates</div>
        <div class="bp-box">Memory remembers</div>
      </div>
    </div>
    <p class="bp-hint">Press <kbd>A</kbd> API · <kbd>M</kbd> MCP · <kbd>C</kbd> CLI · <kbd>N</kbd> Native · <kbd>G</kbd> Game</p>
  </section>

  <section class="access-strip" aria-label="MDx access paths">
    {#each accessCards as card}
      <a href={`#${card.key.toLowerCase()}`} class="access-chip">
        <span>{card.key}</span>
        <strong>{card.title}</strong>
      </a>
    {/each}
  </section>

  <section class="build-flow">
    <div class="section-copy">
      <p class="eyebrow">The shape</p>
      <h2>One governed path, many doors.</h2>
      <p>
        The browser should not be the only way into MDx. The product surface, terminal,
        coding agent, and future Apple apps should all call into the same routes and return
        the same proof.
      </p>
    </div>
    <ol class="steps">
      {#each buildSteps as step, index}
        <li>
          <span>{index + 1}</span>
          <p>{step}</p>
        </li>
      {/each}
    </ol>
  </section>

  <section class="access-grid">
    {#each accessCards as card}
      <article id={card.key.toLowerCase()} class="access-card">
        <div>
          <span class="tag">{card.key}</span>
          <h2>{card.title}</h2>
          <p>{card.body}</p>
        </div>
        <span class="state">{card.state}</span>
        <pre><code>{card.sample}</code></pre>
      </article>
    {/each}
  </section>

  <section class="native-band" id="native">
    <div>
      <p class="eyebrow">iOS / iPadOS / macOS</p>
      <h2>Native apps should feel personal without becoming a separate system.</h2>
      <p>
        The Apple clients can be beautiful and fast because the heavy judgment stays in MDx:
        session admission, policy, route catalogs, worker boundaries, and proof. Native UI
        becomes the best cockpit for the same governed company.
      </p>
    </div>
    <div class="device-stack" aria-hidden="true">
      <span class="mac">Forge</span>
      <span class="ipad">Twin</span>
      <span class="phone">Proof</span>
    </div>
  </section>

  <section class="docs-band">
    <div>
      <p class="eyebrow">Build map</p>
      <h2>Start from the repo's own contracts.</h2>
    </div>
    <div class="doc-row">
      {#each map.developerDocs as doc}
        <article>
          <strong>{doc.title}</strong>
          <code>{doc.path}</code>
          <p>{doc.line}</p>
        </article>
      {/each}
    </div>
  </section>

  <section class="game-section" id="game">
    <div class="section-copy">
      <p class="eyebrow">The run</p>
      <h2>Collect proof. Avoid unsafe authority.</h2>
      <p>
        A tiny MDx side-scroller: jump for receipts, dodge open writes, and keep the run
        moving until a human can make the call.
      </p>
    </div>
    <div class="game-shell">
      <div class="game-top">
        <div>
          <strong>mdx run</strong>
          <span>{game.message}</span>
        </div>
        <button type="button" onclick={startGame}>{game.running ? "Restart" : "Start run"}</button>
      </div>
      <canvas
        bind:this={canvas}
        width="960"
        height="320"
        tabindex="0"
        aria-label="MDx run game. Press space to jump, collect receipts, and avoid unsafe authority blocks."
        onclick={jump}
      ></canvas>
      <p class="game-help">
        Space or tap to jump. Score {game.score}. Best {game.best}.
      </p>
    </div>
  </section>

  <footer class="dev-footer">
    <span>MDx</span>
    <a href="/landing">Product</a>
    <a href="/security">Security</a>
    <a href="/welcome">Start</a>
  </footer>
</main>

<style>
  .dev-page {
    min-height: 100vh;
    overflow-x: hidden;
    background:
      linear-gradient(rgba(255, 255, 255, 0.035) 1px, transparent 1px),
      linear-gradient(90deg, rgba(255, 255, 255, 0.035) 1px, transparent 1px),
      radial-gradient(circle at 16% 20%, rgba(52, 211, 153, 0.34), transparent 25%),
      radial-gradient(circle at 78% 12%, rgba(34, 211, 238, 0.24), transparent 26%),
      linear-gradient(135deg, #1200b8 0%, #0919a7 38%, #07271f 78%, #09080b 100%);
    background-size: 42px 42px, 42px 42px, auto, auto, auto;
    color: #f5f5f7;
    font-family: var(--mdx-font-body);
  }

  .dev-nav {
    position: sticky;
    top: 0;
    z-index: 10;
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    gap: 24px;
    min-height: 64px;
    padding: 12px clamp(18px, 4vw, 52px);
    border-bottom: 1px solid rgba(255, 255, 255, 0.16);
    background: rgba(9, 8, 11, 0.72);
    backdrop-filter: blur(18px);
  }

  .wordmark {
    display: inline-flex;
    align-items: center;
    gap: 1px;
    color: #f5f5f7;
    font-family: var(--mdx-font-display);
    font-size: 20px;
    font-weight: 900;
    text-decoration: none;
  }

  .wordmark span {
    display: grid;
    width: 34px;
    height: 34px;
    place-items: center;
    border: 2px solid currentColor;
    box-shadow: 4px 4px 0 #34d399;
    line-height: 1;
  }

  .wordmark b {
    color: #34d399;
    font-weight: 900;
  }

  .dev-nav nav {
    display: flex;
    justify-content: center;
    gap: 18px;
    min-width: 0;
    overflow-x: auto;
  }

  .dev-nav a {
    color: rgba(245, 245, 247, 0.78);
    font-size: 13px;
    text-decoration: none;
    white-space: nowrap;
  }

  .dev-nav a:hover {
    color: #f5f5f7;
  }

  .nav-button,
  .hero-actions a,
  .game-top button {
    border: 2px solid rgba(255, 255, 255, 0.9);
    border-radius: 2px;
    background: var(--mdx-surface-sunken);
    color: #0a0a0b;
    font-weight: 750;
    text-decoration: none;
    box-shadow: 5px 5px 0 rgba(52, 211, 153, 0.8);
    transition:
      transform 150ms ease,
      box-shadow 150ms ease;
  }

  .nav-button:hover,
  .hero-actions a:hover,
  .game-top button:hover {
    transform: translate(2px, 2px);
    box-shadow: 3px 3px 0 rgba(52, 211, 153, 0.8);
  }

  .dev-nav .nav-button {
    color: #0a0a0b;
  }

  .nav-button {
    display: inline-flex;
    justify-self: end;
    align-items: center;
    padding: 9px 14px;
  }

  .hero {
    display: grid;
    grid-template-columns: minmax(0, 0.92fr) minmax(360px, 1.08fr);
    gap: clamp(28px, 6vw, 76px);
    align-items: center;
    max-width: 1240px;
    min-height: calc(100vh - 64px);
    margin: 0 auto;
    padding: clamp(48px, 7vw, 92px) clamp(18px, 4vw, 52px) 72px;
  }

  .hero-copy {
    display: grid;
    gap: 22px;
    align-content: center;
  }

  .eyebrow,
  .tag,
  .term-label,
  .state {
    margin: 0;
    color: #34d399;
    font-family: var(--mdx-font-mono);
    font-size: 11px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  h1,
  h2,
  p {
    margin: 0;
  }

  h1 {
    max-width: 620px;
    font-family: var(--mdx-font-display);
    font-size: clamp(58px, 8.4vw, 104px);
    font-weight: 900;
    line-height: 0.86;
    text-shadow: 8px 8px 0 rgba(0, 0, 0, 0.34);
  }

  .hero-sub {
    max-width: 680px;
    color: rgba(245, 245, 247, 0.82);
    font-size: clamp(17px, 2vw, 22px);
    line-height: 1.45;
  }

  .install-line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    max-width: 540px;
    padding: 12px 12px;
    border: 2px solid rgba(255, 255, 255, 0.42);
    border-radius: 2px;
    background: rgba(245, 245, 247, 0.92);
    box-shadow: 7px 7px 0 rgba(0, 0, 0, 0.34);
  }

  .install-line code,
  pre code,
  .docs-band code {
    font-family: var(--mdx-font-mono);
  }

  .install-line code {
    min-width: 0;
    color: #0a0a0b;
    font-size: 13px;
    overflow-wrap: anywhere;
  }

  .install-line span {
    color: #1200b8;
    font-family: var(--mdx-font-mono);
    font-size: 13px;
  }

  .install-line button {
    margin-left: auto;
    border: 1px solid rgba(10, 10, 11, 0.18);
    border-radius: 2px;
    background: #0a0a0b;
    color: #34d399;
    font-family: var(--mdx-font-mono);
    font-size: 13px;
    padding: 5px 8px;
  }

  .hero-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }

  .hero-actions a {
    display: inline-flex;
    min-height: 42px;
    align-items: center;
    padding: 0 18px;
  }

  .hero-actions .secondary {
    background: #09080b;
    color: #f5f5f7;
    box-shadow: 5px 5px 0 rgba(251, 191, 36, 0.75);
  }

  .hero-panel {
    position: relative;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    gap: 14px;
    min-height: 520px;
    padding: 16px;
    border: 2px solid rgba(245, 245, 247, 0.72);
    border-radius: 2px;
    background:
      linear-gradient(rgba(255, 255, 255, 0.06) 1px, transparent 1px),
      linear-gradient(90deg, rgba(255, 255, 255, 0.06) 1px, transparent 1px),
      rgba(9, 8, 11, 0.68);
    background-size: 18px 18px, 18px 18px, auto;
    box-shadow: 14px 14px 0 rgba(0, 0, 0, 0.34);
    overflow: hidden;
  }

  .window-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    color: rgba(245, 245, 247, 0.68);
    font-family: var(--mdx-font-mono);
    font-size: 12px;
  }

  .window-bar span {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.24);
  }

  .window-bar strong {
    margin-left: 8px;
    font-weight: 500;
  }

  .pixel-workbench {
    position: relative;
    min-height: 350px;
    border: 1px solid rgba(245, 245, 247, 0.18);
    background:
      radial-gradient(circle at 68% 18%, rgba(34, 211, 238, 0.18), transparent 18%),
      linear-gradient(180deg, rgba(18, 0, 184, 0.32), rgba(5, 38, 31, 0.46));
    overflow: hidden;
  }

  .pixel-workbench::before {
    content: "";
    position: absolute;
    inset: auto 0 44px;
    height: 5px;
    background: var(--mdx-surface-sunken);
    box-shadow: 0 10px 0 rgba(52, 211, 153, 0.36);
  }

  .pixel-rail,
  .pixel-board,
  .pixel-agent,
  .pixel-receipt {
    position: absolute;
  }

  .pixel-rail {
    left: 28px;
    top: 34px;
    display: grid;
    gap: 12px;
  }

  .pixel-rail i {
    display: block;
    width: 42px;
    height: 42px;
    border: 2px solid rgba(245, 245, 247, 0.72);
    background: rgba(10, 10, 11, 0.5);
    box-shadow: 4px 4px 0 rgba(34, 211, 238, 0.36);
  }

  .pixel-board {
    left: 106px;
    right: 42px;
    top: 42px;
    min-height: 184px;
    padding: 22px;
    border: 2px solid rgba(245, 245, 247, 0.65);
    background: rgba(245, 245, 247, 0.92);
    color: #0a0a0b;
    box-shadow: 8px 8px 0 rgba(0, 0, 0, 0.3);
  }

  .pixel-title {
    font-family: var(--mdx-font-mono);
    font-size: 13px;
    color: #1200b8;
  }

  .pixel-row {
    width: 54%;
    height: 12px;
    margin-top: 18px;
    background: #0a0a0b;
  }

  .pixel-row.wide {
    width: 82%;
    background: #34d399;
  }

  .pixel-cells {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 8px;
    margin-top: 24px;
  }

  .pixel-cells i {
    height: 42px;
    background: rgba(18, 0, 184, 0.12);
    border: 1px solid rgba(18, 0, 184, 0.28);
  }

  .pixel-agent {
    left: 92px;
    bottom: 58px;
    width: 54px;
    height: 48px;
    background: #34d399;
    box-shadow: 6px 6px 0 rgba(0, 0, 0, 0.3);
  }

  .pixel-agent span {
    position: absolute;
    left: 9px;
    top: 14px;
    width: 8px;
    height: 8px;
    background: #0a0a0b;
    box-shadow: 27px 0 0 #0a0a0b;
  }

  .pixel-agent b {
    position: absolute;
    left: 16px;
    bottom: 12px;
    width: 22px;
    height: 4px;
    background: #0a0a0b;
  }

  .pixel-receipt {
    width: 30px;
    height: 34px;
    background: var(--mdx-surface-sunken);
    box-shadow: 5px 5px 0 rgba(251, 191, 36, 0.55);
  }

  .pixel-receipt::before,
  .pixel-receipt::after {
    content: "";
    position: absolute;
    left: 7px;
    height: 3px;
    background: #0a0a0b;
  }

  .pixel-receipt::before {
    top: 10px;
    width: 16px;
  }

  .pixel-receipt::after {
    top: 18px;
    width: 11px;
  }

  .pixel-receipt.one {
    right: 146px;
    bottom: 78px;
  }

  .pixel-receipt.two {
    right: 54px;
    bottom: 128px;
  }

  .hero-stats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
  }

  .hero-stats div,
  .access-card,
  .docs-band article {
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 2px;
    background: rgba(255, 255, 255, 0.055);
  }

  .hero-stats div {
    display: grid;
    gap: 8px;
    padding: 16px;
  }

  .hero-stats strong {
    font-family: var(--mdx-font-display);
    font-size: 36px;
    line-height: 1;
  }

  .hero-stats small {
    color: rgba(245, 245, 247, 0.58);
    font-size: 12px;
    line-height: 1.35;
  }

  .access-strip,
  .build-flow,
  .access-grid,
  .native-band,
  .docs-band,
  .game-section,
  .dev-footer {
    max-width: 1180px;
    margin: 0 auto;
    padding-left: clamp(18px, 4vw, 52px);
    padding-right: clamp(18px, 4vw, 52px);
  }

  .access-strip {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 10px;
    padding-bottom: 92px;
  }

  .access-chip {
    display: grid;
    gap: 6px;
    padding: 18px;
    border: 2px solid rgba(255, 255, 255, 0.18);
    border-radius: 2px;
    background: rgba(9, 8, 11, 0.42);
    color: #f5f5f7;
    text-decoration: none;
    box-shadow: 5px 5px 0 rgba(0, 0, 0, 0.28);
    transition:
      transform 150ms ease,
      background 150ms ease,
      box-shadow 150ms ease;
  }

  .access-chip:hover {
    background: rgba(245, 245, 247, 0.12);
    transform: translate(2px, 2px);
    box-shadow: 3px 3px 0 rgba(0, 0, 0, 0.28);
  }

  .access-chip span {
    color: #fbbf24;
    font-family: var(--mdx-font-mono);
    font-size: 11px;
  }

  .access-chip strong {
    font-size: 16px;
  }

  .build-flow,
  .native-band,
  .game-section {
    display: grid;
    grid-template-columns: minmax(0, 0.85fr) minmax(0, 1.15fr);
    gap: clamp(24px, 5vw, 64px);
    align-items: start;
    padding-bottom: 96px;
  }

  .section-copy {
    display: grid;
    gap: 14px;
  }

  .section-copy h2,
  .native-band h2,
  .docs-band h2 {
    font-family: var(--mdx-font-display);
    font-size: clamp(30px, 4.4vw, 56px);
    line-height: 1.02;
  }

  .section-copy p,
  .native-band p,
  .docs-band p,
  .access-card p {
    color: rgba(245, 245, 247, 0.68);
    font-size: 16px;
    line-height: 1.55;
  }

  .steps {
    display: grid;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .steps li {
    display: grid;
    grid-template-columns: 42px minmax(0, 1fr);
    gap: 16px;
    align-items: center;
    padding: 18px;
    border: 2px solid rgba(255, 255, 255, 0.12);
    border-radius: 2px;
    background: rgba(9, 8, 11, 0.42);
    box-shadow: 5px 5px 0 rgba(0, 0, 0, 0.22);
  }

  .steps span {
    display: grid;
    width: 42px;
    height: 42px;
    place-items: center;
    border-radius: 8px;
    background: rgba(52, 211, 153, 0.14);
    color: #34d399;
    font-family: var(--mdx-font-mono);
  }

  .steps p {
    color: #f5f5f7;
    font-size: 16px;
  }

  .access-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 16px;
    padding-bottom: 96px;
  }

  .access-card {
    display: grid;
    grid-template-rows: 1fr auto auto;
    gap: 18px;
    min-height: 336px;
    padding: 22px;
    box-shadow: 7px 7px 0 rgba(0, 0, 0, 0.24);
  }

  .access-card h2 {
    margin-top: 10px;
    font-family: var(--mdx-font-display);
    font-size: 30px;
  }

  .state {
    color: #22d3ee;
  }

  pre {
    margin: 0;
    min-width: 0;
    overflow: auto;
    padding: 16px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 2px;
    background: rgba(0, 0, 0, 0.34);
  }

  pre code {
    color: rgba(245, 245, 247, 0.86);
    font-size: 12px;
    line-height: 1.6;
    white-space: pre;
  }

  .native-band {
    align-items: center;
  }

  .native-band > div:first-child {
    display: grid;
    gap: 14px;
  }

  .device-stack {
    position: relative;
    min-height: 360px;
  }

  .device-stack span {
    position: absolute;
    display: grid;
    place-items: end start;
    border: 1px solid rgba(255, 255, 255, 0.14);
    border-radius: 2px;
    background:
      linear-gradient(rgba(255, 255, 255, 0.06) 1px, transparent 1px),
      linear-gradient(90deg, rgba(255, 255, 255, 0.06) 1px, transparent 1px),
      linear-gradient(150deg, rgba(255, 255, 255, 0.1), rgba(255, 255, 255, 0.04));
    background-size: 18px 18px, 18px 18px, auto;
    color: rgba(245, 245, 247, 0.82);
    font-family: var(--mdx-font-mono);
    font-size: 12px;
    padding: 18px;
    box-shadow: 10px 10px 0 rgba(0, 0, 0, 0.32);
  }

  .mac {
    inset: 22px 22px 86px 56px;
  }

  .ipad {
    inset: 92px 168px 24px 0;
    border-radius: 2px;
  }

  .phone {
    inset: 124px 0 12px auto;
    width: 132px;
    border-radius: 2px;
  }

  .docs-band {
    display: grid;
    gap: 24px;
    padding-bottom: 96px;
  }

  .doc-row {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 12px;
  }

  .docs-band article {
    display: grid;
    gap: 10px;
    padding: 18px;
  }

  .docs-band strong {
    font-family: var(--mdx-font-display);
    font-size: 18px;
  }

  .docs-band code {
    color: #fbbf24;
    font-size: 11px;
    overflow-wrap: anywhere;
  }

  .game-section {
    max-width: 1240px;
    grid-template-columns: minmax(260px, 0.35fr) minmax(0, 1fr);
    padding-top: 36px;
    padding-bottom: 90px;
  }

  .game-shell {
    overflow: hidden;
    border: 2px solid rgba(245, 245, 247, 0.86);
    border-radius: 2px;
    background: rgba(10, 10, 11, 0.78);
    box-shadow: 12px 12px 0 rgba(0, 0, 0, 0.34);
  }

  .game-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 14px 16px;
    border-bottom: 2px solid rgba(245, 245, 247, 0.18);
  }

  .game-top div {
    display: grid;
    gap: 3px;
  }

  .game-top strong {
    font-family: var(--mdx-font-display);
    font-size: 17px;
  }

  .game-top span,
  .game-help {
    color: rgba(245, 245, 247, 0.62);
    font-size: 13px;
  }

  .game-top button {
    min-height: 36px;
    padding: 0 14px;
    cursor: pointer;
  }

  canvas {
    display: block;
    width: 100%;
    aspect-ratio: 3 / 1;
    background: #0a0a0b;
    cursor: pointer;
    image-rendering: pixelated;
  }

  .game-help {
    padding: 12px 16px 14px;
  }

  .dev-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding-bottom: 42px;
    color: rgba(245, 245, 247, 0.56);
    font-size: 13px;
  }

  .dev-footer span {
    color: #f5f5f7;
    font-family: var(--mdx-font-display);
    font-weight: 900;
  }

  .dev-footer a {
    color: rgba(245, 245, 247, 0.7);
    text-decoration: none;
  }

  @media (max-width: 900px) {
    .dev-nav {
      grid-template-columns: auto auto;
    }

    .dev-nav nav {
      grid-column: 1 / -1;
      justify-content: flex-start;
      order: 3;
    }

    .hero,
    .build-flow,
    .native-band,
    .game-section {
      grid-template-columns: 1fr;
      min-height: auto;
    }

    .access-strip,
    .access-grid,
    .doc-row,
    .hero-stats {
      grid-template-columns: 1fr 1fr;
    }

    .hero-panel {
      min-height: 360px;
    }
  }

  @media (max-width: 620px) {
    .access-strip,
    .access-grid,
    .doc-row,
    .hero-stats {
      grid-template-columns: 1fr;
    }

    .dev-nav {
      gap: 12px;
    }

    h1 {
      font-size: 48px;
    }

    .hero {
      padding-top: 38px;
    }

    .hero-stats strong {
      font-size: 30px;
    }

    .device-stack {
      min-height: 300px;
    }

    .mac {
      inset: 6px 0 98px 0;
    }

    .ipad {
      inset: 96px 92px 20px 0;
    }

    .phone {
      inset: 126px 0 12px auto;
      width: 110px;
    }

    .game-top {
      align-items: flex-start;
      flex-direction: column;
    }
  }
  .proof-band { display: grid; grid-template-columns: minmax(0, 5fr) minmax(0, 7fr); gap: 32px; align-items: center; padding: 56px 0; border-top: 1px dashed var(--mdx-border-default); }
  .proof-panels { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 18px; align-items: start; }
  .proof-term { border: 1px solid var(--mdx-border-subtle); border-radius: 12px; overflow: hidden; background: var(--mdx-surface-base); box-shadow: var(--mdx-shadow-card); }
  .proof-term pre { margin: 0; padding: 16px 18px; font-family: var(--mdx-font-mono); font-size: 12px; line-height: 1.6; overflow-x: auto; color: var(--mdx-text-primary); }
  .proof-toasts { display: grid; gap: 12px; align-content: center; }
  .dev-toast { display: flex; gap: 10px; align-items: flex-start; padding: 10px 14px; border: 1px solid var(--mdx-border-subtle); border-radius: 10px; background: var(--mdx-surface-raised); box-shadow: var(--mdx-shadow-card); max-width: 300px; }
  .dev-toast strong { display: block; font-family: var(--mdx-font-mono); font-size: 11.5px; }
  .dev-toast span:not(.dt-glyph) { font-size: 11.5px; color: var(--mdx-text-secondary); }
  .dt-glyph { font-size: 13px; color: var(--mdx-accent-primary); }
  .blueprint { padding: 40px 0 24px; border-top: 1px dashed var(--mdx-border-default); }
  .bp-grid { display: flex; gap: 14px; align-items: stretch; flex-wrap: wrap; }
  .bp-col { display: grid; gap: 8px; align-content: start; flex: 1; min-width: 140px; }
  .bp-head { margin: 0; font-family: var(--mdx-font-mono); font-size: 11px; text-transform: uppercase; letter-spacing: 0.08em; color: var(--mdx-text-muted); }
  .bp-box { border: 1px dashed var(--mdx-border-default); border-radius: 8px; padding: 10px 12px; font-size: 12.5px; font-weight: 600; color: var(--mdx-text-primary); }
  .bp-box span { display: block; font-weight: 400; font-size: 11px; color: var(--mdx-text-muted); margin-top: 2px; }
  .bp-box.solid { border-style: solid; background: var(--mdx-surface-raised); box-shadow: var(--mdx-shadow-card); }
  .bp-arrow { align-self: center; color: var(--mdx-text-muted); font-size: 16px; }
  .bp-hint { margin: 18px 0 0; font-size: 12px; color: var(--mdx-text-muted); }
  .bp-hint kbd { font-family: var(--mdx-font-mono); font-size: 11px; border: 1px solid var(--mdx-border-subtle); border-radius: 4px; padding: 1px 5px; background: var(--mdx-surface-raised); }
  @media (max-width: 900px) { .proof-band { grid-template-columns: 1fr; } .proof-panels { grid-template-columns: 1fr; } }
</style>
