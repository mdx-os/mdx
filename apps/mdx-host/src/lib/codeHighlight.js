// A small, dependency-free code tokenizer for diff lines. Not a full lexer -
// it walks one line and labels strings, comments, numbers, keywords, and types
// so the surface can color them with theme tokens. Per-line by design (diffs
// are line-oriented); a string or comment that opens and closes on the line is
// handled, cross-line state is not (acceptable for a review diff). The point is
// the premium, legible feel of color-coded code, not a compiler.

const KEYWORDS = new Set([
  // Rust
  "fn", "let", "mut", "pub", "const", "static", "struct", "enum", "impl", "trait",
  "use", "mod", "match", "as", "ref", "dyn", "where", "move", "unsafe", "crate",
  "self", "Self", "super", "async", "await",
  // shared control flow
  "if", "else", "for", "while", "loop", "return", "break", "continue", "in",
  // JS/TS
  "function", "var", "class", "extends", "implements", "interface", "new",
  "import", "export", "from", "default", "typeof", "instanceof", "void",
  "type", "of", "yield", "throw", "try", "catch", "finally", "switch", "case",
  // values
  "true", "false", "null", "undefined", "None", "Some", "Ok", "Err", "true", "false"
]);

const KW_VALUE = new Set(["true", "false", "null", "undefined", "None", "Some", "Ok", "Err"]);

function isIdentStart(c) {
  return /[A-Za-z_]/.test(c);
}
function isIdent(c) {
  return /[A-Za-z0-9_]/.test(c);
}

// Returns an array of { t, v } tokens for one line of code. `t` is one of
// comment, string, number, keyword, value, type, fn, punct, text.
export function tokenizeLine(line) {
  const tokens = [];
  let i = 0;
  const n = line.length;
  const push = (t, v) => {
    if (v) tokens.push({ t, v });
  };
  while (i < n) {
    const c = line[i];

    // Line comments: // ... or # ... (Rust/JS use //, shell/py use #).
    if ((c === "/" && line[i + 1] === "/") || c === "#") {
      push("comment", line.slice(i));
      break;
    }
    // Block comment opening on this line: /* ... (to */ or end of line).
    if (c === "/" && line[i + 1] === "*") {
      const end = line.indexOf("*/", i + 2);
      const stop = end === -1 ? n : end + 2;
      push("comment", line.slice(i, stop));
      i = stop;
      continue;
    }
    // Strings: ", ', ` - to the matching quote, honoring backslash escapes.
    if (c === '"' || c === "'" || c === "`") {
      let j = i + 1;
      while (j < n) {
        if (line[j] === "\\") {
          j += 2;
          continue;
        }
        if (line[j] === c) {
          j += 1;
          break;
        }
        j += 1;
      }
      push("string", line.slice(i, j));
      i = j;
      continue;
    }
    // Numbers.
    if (/[0-9]/.test(c)) {
      let j = i;
      while (j < n && /[0-9._xXa-fA-F]/.test(line[j])) j += 1;
      push("number", line.slice(i, j));
      i = j;
      continue;
    }
    // Identifiers, keywords, types, function calls.
    if (isIdentStart(c)) {
      let j = i;
      while (j < n && isIdent(line[j])) j += 1;
      const word = line.slice(i, j);
      if (KW_VALUE.has(word)) {
        push("value", word);
      } else if (KEYWORDS.has(word)) {
        push("keyword", word);
      } else if (/^[A-Z]/.test(word)) {
        push("type", word);
      } else if (line[j] === "(") {
        push("fn", word);
      } else {
        push("text", word);
      }
      i = j;
      continue;
    }
    // Everything else: punctuation/whitespace, coalesced.
    let j = i;
    while (j < n && !isIdentStart(line[j]) && !/[0-9"'`]/.test(line[j]) && line[j] !== "/" && line[j] !== "#") {
      j += 1;
    }
    if (j === i) j = i + 1;
    push("punct", line.slice(i, j));
    i = j;
  }
  return tokens;
}

// The @@ hunk header carries the starting line numbers: "@@ -oldStart,oldCount
// +newStart,newCount @@". We read the two starts and then count forward so
// every row gets its real old and new line numbers, the way a real review
// tool shows them.
const HUNK_HEADER = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/;

// Split a unified-diff patch into rendered lines: the +/-/context marker kept
// separate from the code, the code tokenized, and each row tagged with its old
// and new line numbers (null on the side where the row does not exist, e.g. an
// added line has no old-side number). Hunk and meta lines are not tokenized
// (they get their own color) and carry no line numbers.
export function diffLines(patch) {
  let oldNo = 0;
  let newNo = 0;
  return String(patch ?? "")
    .split("\n")
    .map((raw) => {
      if (raw.startsWith("@@")) {
        const m = raw.match(HUNK_HEADER);
        if (m) {
          oldNo = Number(m[1]);
          newNo = Number(m[2]);
        }
        return { kind: "hunk", marker: "", oldLine: null, newLine: null, tokens: [{ t: "hunk", v: raw }] };
      }
      if (raw.startsWith("diff ") || raw.startsWith("index ") || raw.startsWith("--- ") || raw.startsWith("+++ ") || raw.startsWith("new file") || raw.startsWith("deleted file")) {
        return { kind: "meta", marker: "", oldLine: null, newLine: null, tokens: [{ t: "meta", v: raw }] };
      }
      let kind = "ctx";
      let marker = "";
      let code = raw;
      let oldLine = null;
      let newLine = null;
      if (raw.startsWith("+")) {
        kind = "add";
        marker = "+";
        code = raw.slice(1);
        newLine = newNo;
        newNo += 1;
      } else if (raw.startsWith("-")) {
        kind = "del";
        marker = "-";
        code = raw.slice(1);
        oldLine = oldNo;
        oldNo += 1;
      } else {
        // Context line (leading space, or an unmarked trailing line).
        marker = raw.startsWith(" ") ? " " : "";
        code = raw.startsWith(" ") ? raw.slice(1) : raw;
        oldLine = oldNo;
        newLine = newNo;
        oldNo += 1;
        newNo += 1;
      }
      return { kind, marker, oldLine, newLine, tokens: tokenizeLine(code) };
    });
}
