<script>
  // THE repo connect. Three surfaces grew their own copies of this form
  // (forge door, runs page, welcome setup) and they drifted - this is the
  // consolidation, built from the best halves of each: the door's clone
  // naming, folder dialog, and git-error split; welcome's paste-and-detect.
  // One field, one click; the kind toggles (and auto-detects on paste).
  import { deriveRepoFields, repoKindOf, parseConnectError } from "./forgeRepo.js";

  let {
    actor = "local_user",
    compact = false,
    hosted = false,
    onConnected = () => {}
  } = $props();

  let root = $state("");
  let kind = $state("remote");
  let connecting = $state(false);
  let folderPicking = $state(false);
  let flash = $state(null);

  const pendingName = $derived(root.trim() ? deriveRepoFields(root, kind).label : "");

  function onInput() {
    // Content decides: a URL is a remote, anything else is a folder path.
    // (repoKindOf is deterministic, so the mode can never disagree with what
    // was actually typed.)
    if (root.trim()) kind = repoKindOf(root.trim());
  }

  function switchKind() {
    kind = kind === "remote" ? "local" : "remote";
    root = "";
    flash = null;
  }

  async function chooseFolder() {
    if (folderPicking) return;
    folderPicking = true;
    flash = null;
    try {
      const response = await fetch("/api/kernel/forge/local-folder-dialog.json", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ open_dialog: true })
      });
      const data = await response.json().catch(() => null);
      if (data?.status === "OK" && data.path) {
        root = data.path;
        kind = "local";
        if (data.is_git_repo === false) {
          flash = { ok: false, line: "That folder has no .git - connect anyway if it is the right one.", raw: "" };
        }
      } else if (data?.status === "DIALOG_UNAVAILABLE") {
        flash = { ok: false, line: "The folder picker is not available here - paste the folder path instead.", raw: "" };
      }
    } catch (error) {
      flash = { ok: false, line: "The folder picker did not answer - paste the path instead.", raw: "" };
    }
    folderPicking = false;
  }

  async function connect() {
    const value = root.trim();
    if (connecting || !value) return;
    connecting = true;
    flash = null;
    try {
      const derived = deriveRepoFields(value, kind);
      const response = await fetch("/api/kernel/forge/repos.json", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          repo_id: derived.id,
          label: derived.label,
          root: value,
          kind,
          actor_id: actor
        })
      });
      const packet = await response.json().catch(() => ({}));
      if (response.ok && packet?.status === "CONNECTED") {
        root = "";
        flash = null;
        onConnected(derived.id, derived.label);
      } else {
        flash = { ok: false, ...parseConnectError(packet?.reason) };
      }
    } catch (error) {
      flash = { ok: false, line: "Nothing connected - that did not land.", raw: "" };
    }
    connecting = false;
  }
</script>

<div class="rc" data-compact={compact}>
  {#if hosted}
    <strong>Connect GitHub</strong>
    <p class="rc-note">You choose the repositories MDx can see. No personal access token.</p>
    <a class="rc-go" href="/forge/connect/github?action=start">Choose repositories in GitHub</a>
  {/if}
  <div class="rc-field">
    <input
      type="text"
      class="rc-input"
      bind:value={root}
      oninput={onInput}
      placeholder={kind === "remote" ? (hosted ? "Paste a public GitHub or Bitbucket URL" : "Paste a GitHub or Bitbucket URL") : "Path to a folder on this machine"}
      aria-label={kind === "remote" ? "Repo URL" : "Repo folder path"}
      spellcheck="false"
      onkeydown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          connect();
        }
      }}
    />
    {#if kind === "local"}
      <button type="button" class="rc-browse" disabled={folderPicking} onclick={chooseFolder}>
        {folderPicking ? "Opening..." : "Choose folder"}
      </button>
    {/if}
    <button type="button" class="rc-go" disabled={connecting || !root.trim()} onclick={connect}>
      {connecting ? (kind === "remote" ? "Cloning..." : "Connecting...") : "Connect"}
    </button>
  </div>
  {#if !hosted}
    <button type="button" class="rc-alt" onclick={switchKind}>
      {kind === "remote" ? "Or point Forge at a folder on this machine" : "Or paste a GitHub or Bitbucket URL instead"}
    </button>
  {/if}
  {#if connecting && pendingName}
    <p class="rc-note"><span class="rc-spin" aria-hidden="true"></span>{kind === "remote" ? "Cloning" : "Connecting"} {pendingName}...</p>
  {:else if !compact}
    <p class="rc-note">
      {kind === "remote"
        ? hosted
          ? "Public repositories connect directly. Use the GitHub connection above for private code."
          : "We clone it with this machine's git credentials and keep it under .mdx-local. Forge names it from the URL."
        : "Forge works in an isolated copy under .mdx-local - your folder is never touched."}
    </p>
  {/if}
  {#if flash && !flash.ok}
    <div class="rc-err" role="alert">
      <p>{flash.line}</p>
      {#if flash.raw}
        <details class="rc-raw"><summary>Details</summary><code>{flash.raw}</code></details>
      {/if}
    </div>
  {/if}
</div>

<style>
  .rc { display: grid; gap: 7px; }
  .rc-field { display: flex; gap: 8px; }
  .rc-input { flex: 1; min-width: 0; font: inherit; font-size: 13px; padding: 9px 11px; border-radius: 8px; border: 1px solid var(--mdx-border-subtle); background: var(--mdx-surface-raised); color: var(--mdx-text-primary); }
  .rc-browse, .rc-go { font: inherit; font-size: 12.5px; padding: 8px 13px; border-radius: 8px; border: 1px solid var(--mdx-border-subtle); background: var(--mdx-surface-raised); color: var(--mdx-text-primary); cursor: pointer; white-space: nowrap; }
  .rc-go { border: none; background: var(--mdx-accent-primary); color: var(--mdx-on-accent); font-weight: 650; }
  .rc-go:disabled, .rc-browse:disabled { opacity: 0.55; cursor: default; }
  .rc-alt { justify-self: start; border: none; background: none; font: inherit; font-size: 12px; color: var(--mdx-text-secondary); cursor: pointer; padding: 0; text-decoration: underline; text-underline-offset: 2px; }
  .rc-note { display: flex; align-items: center; gap: 7px; margin: 0; font-size: 12px; color: var(--mdx-text-muted); line-height: 1.5; }
  .rc-spin { width: 10px; height: 10px; border-radius: 50%; border: 2px solid var(--mdx-border-subtle); border-top-color: var(--mdx-accent-primary); animation: rc-spin 0.8s linear infinite; }
  @keyframes rc-spin { to { transform: rotate(360deg); } }
  .rc-err { display: grid; gap: 4px; font-size: 12.5px; color: var(--mdx-accent-error); }
  .rc-err p { margin: 0; }
  .rc-raw summary { cursor: pointer; font-size: 11.5px; color: var(--mdx-text-muted); }
  .rc-raw code { display: block; margin-top: 4px; padding: 8px; border-radius: 6px; font-size: 11px; background: var(--mdx-surface-raised); border: 1px solid var(--mdx-border-subtle); color: var(--mdx-text-secondary); white-space: pre-wrap; word-break: break-word; }
  @media (prefers-reduced-motion: reduce) { .rc-spin { animation: none; } }
  @media (max-width: 620px) {
    .rc-field { align-items: stretch; flex-direction: column; }
  }
</style>
