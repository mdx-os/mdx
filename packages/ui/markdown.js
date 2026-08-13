// Streaming-safe markdown for companion answers. Safe by construction:
// the raw text is HTML-escaped FIRST, then markdown transforms run on
// the escaped text - there is no path for model output to inject markup.
// Block-splitting gives streaming its performance: completed blocks are
// content-stable strings, so Svelte's equality check skips re-rendering
// them and only the block still receiving tokens re-parses per frame.

function escapeHtml(text) {
  return String(text ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

// Split on blank lines, keeping fenced code together: a fence opened in
// one chunk and closed three paragraphs later is ONE block.
export function splitBlocks(text) {
  const lines = String(text ?? "").split("\n");
  const blocks = [];
  let current = [];
  let inFence = false;
  for (const line of lines) {
    if (line.trimStart().startsWith("```")) {
      inFence = !inFence;
    }
    if (!inFence && line.trim() === "") {
      if (current.length > 0) {
        blocks.push(current.join("\n"));
        current = [];
      }
      continue;
    }
    current.push(line);
  }
  if (current.length > 0) {
    blocks.push(current.join("\n"));
  }
  return blocks;
}

// Mid-stream markdown is legally broken: an unclosed fence or a dangling
// bold marker. Close them for display; the next token batch re-repairs.
export function repairBlock(block) {
  let repaired = block;
  const fences = (repaired.match(/```/g) ?? []).length;
  if (fences % 2 === 1) {
    repaired += "\n```";
  }
  const bolds = (repaired.match(/\*\*/g) ?? []).length;
  if (bolds % 2 === 1) {
    repaired += "**";
  }
  return repaired;
}

function inline(escaped) {
  return escaped
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/(^|\s)\*([^*\s][^*]*)\*/g, "$1<em>$2</em>")
    .replace(/\[([^\]]+)\]\((https?:\/\/[^)\s]+|\/[^)\s]*)\)/g, '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>');
}

export function renderBlock(block) {
  const raw = String(block ?? "");
  // Fenced code: escaped verbatim, no inline transforms inside.
  const fence = /^```([a-zA-Z0-9_-]*)\n?([\s\S]*?)\n?```\s*$/.exec(raw);
  if (fence) {
    return `<pre><code>${escapeHtml(fence[2])}</code></pre>`;
  }
  const escaped = escapeHtml(raw);
  const lines = escaped.split("\n");
  // Headings (block-level, first line wins the shape).
  const heading = /^(#{1,3})\s+(.*)$/.exec(lines[0]);
  if (heading && lines.length === 1) {
    const level = heading[1].length + 2; // h3..h5: never compete with page headings
    return `<h${level}>${inline(heading[2])}</h${level}>`;
  }
  // Lists: -, *, →, ->, or numbered.
  const isList = lines.every((line) => /^\s*([-*→]|-&gt;|\d+\.)\s+/.test(line));
  if (isList && lines.length > 0) {
    const items = lines
      .map((line) => line.replace(/^\s*([-*→]|-&gt;|\d+\.)\s+/, ""))
      .map((item) => `<li>${inline(item)}</li>`)
      .join("");
    return `<ul>${items}</ul>`;
  }
  // Blockquote.
  if (lines.every((line) => line.startsWith("&gt;"))) {
    const quoted = lines.map((line) => line.replace(/^&gt;\s?/, "")).join("<br>");
    return `<blockquote>${inline(quoted)}</blockquote>`;
  }
  return `<p>${inline(lines.join("<br>"))}</p>`;
}
