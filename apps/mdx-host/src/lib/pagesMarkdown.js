export function pagesMarkdownBlocks(markdown) {
  const blocks = [];
  let code = null;
  let paragraph = [];
  const flush = () => {
    if (paragraph.length) {
      blocks.push({ kind: "p", text: paragraph.join(" ") });
      paragraph = [];
    }
  };
  const lines = String(markdown).split("\n");
  const tableCells = (value) => value
    .trim()
    .replace(/^\||\|$/g, "")
    .split("|")
    .map((cell) => cell.trim());
  const tableDivider = (value) => {
    const cells = tableCells(value);
    return cells.length > 0 && cells.every((cell) => /^:?-{3,}:?$/.test(cell));
  };

  for (let index = 0; index < lines.length; index += 1) {
    const raw = lines[index];
    const line = raw.replace(/\s+$/, "");
    if (code !== null) {
      if (line.startsWith("```")) {
        blocks.push({ kind: "code", text: code.join("\n") });
        code = null;
      } else {
        code.push(raw);
      }
      continue;
    }
    if (
      line.trim().startsWith("|")
      && index + 1 < lines.length
      && tableDivider(lines[index + 1])
    ) {
      flush();
      const headers = tableCells(line);
      const rows = [];
      index += 2;
      while (index < lines.length && lines[index].trim().startsWith("|")) {
        const cells = tableCells(lines[index]);
        rows.push(headers.map((_, cellIndex) => cells[cellIndex] ?? ""));
        index += 1;
      }
      index -= 1;
      blocks.push({ kind: "table", headers, rows });
    } else if (line.startsWith("```")) {
      flush();
      code = [];
    } else if (/^#{1,4} /.test(line)) {
      flush();
      blocks.push({
        kind: "h",
        level: line.match(/^#+/)[0].length,
        text: line.replace(/^#+ /, "")
      });
    } else if (/^[-*] /.test(line)) {
      flush();
      const last = blocks[blocks.length - 1];
      const item = line.replace(/^[-*] /, "");
      if (last && last.kind === "ul") last.items.push(item);
      else blocks.push({ kind: "ul", items: [item] });
    } else if (/^\d+\. /.test(line)) {
      flush();
      const last = blocks[blocks.length - 1];
      const item = line.replace(/^\d+\. /, "");
      if (last && last.kind === "ol") last.items.push(item);
      else blocks.push({ kind: "ol", items: [item] });
    } else if (line.trim() === "") {
      flush();
    } else {
      paragraph.push(line.trim());
    }
  }
  flush();
  return blocks;
}
