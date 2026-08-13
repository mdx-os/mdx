//! The V4A patch format for the GPT/Codex-family tool surface.
//!
//! OpenAI's GPT and Codex models are reinforcement-trained on one specific
//! edit wire format: an `apply_patch` envelope (`*** Begin Patch` ..
//! `*** End Patch`) holding Add/Update/Delete File sections with `@@`
//! anchored hunks. Serving those models our str-replace `edit_file` tool
//! makes them spend reasoning tokens fighting their training. This module
//! parses and applies that envelope so the turn client can serve each model
//! family the edit surface it was trained on; Claude and every other family
//! keep `edit_file`/`write_file` unchanged.
//!
//! Matching follows the reference applier's spirit: hunks apply at the FIRST
//! match at or after the cursor (the format is sequential, not unique-span),
//! with two fuzz levels - exact, then trailing-whitespace-insensitive, then
//! fully trimmed. Application is all-or-nothing per envelope: every operation
//! is computed against the workspace before any file is written, so a failing
//! hunk never leaves a half-applied patch behind.

/// One file-level operation parsed from the envelope. Variant names
/// mirror the envelope's own section headers (Add File / Delete File /
/// Update File) - the wire format is the naming authority here.
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PatchOp {
    AddFile {
        path: String,
        content: String,
    },
    DeleteFile {
        path: String,
    },
    UpdateFile {
        path: String,
        move_to: Option<String>,
        chunks: Vec<PatchChunk>,
    },
}

/// One `@@` hunk inside an Update File section. `old_lines` is the
/// context-plus-removed sequence that must match the file; `new_lines` is
/// the context-plus-added sequence that replaces it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PatchChunk {
    pub anchor: Option<String>,
    pub old_lines: Vec<String>,
    pub new_lines: Vec<String>,
}

impl PatchOp {
    /// Every workspace-relative path this operation touches, for scope
    /// enforcement before anything is applied.
    pub(crate) fn touched_paths(&self) -> Vec<&str> {
        match self {
            PatchOp::AddFile { path, .. } | PatchOp::DeleteFile { path } => vec![path.as_str()],
            PatchOp::UpdateFile { path, move_to, .. } => {
                let mut paths = vec![path.as_str()];
                if let Some(target) = move_to {
                    paths.push(target.as_str());
                }
                paths
            }
        }
    }
}

const BEGIN_MARKER: &str = "*** Begin Patch";
const END_MARKER: &str = "*** End Patch";
const ADD_PREFIX: &str = "*** Add File: ";
const UPDATE_PREFIX: &str = "*** Update File: ";
const DELETE_PREFIX: &str = "*** Delete File: ";
const MOVE_PREFIX: &str = "*** Move to: ";
const EOF_MARKER: &str = "*** End of File";

/// Parse a V4A envelope into file operations. Errors name the offending
/// line so the model can correct the patch on its next turn.
pub(crate) fn parse_patch(input: &str) -> Result<Vec<PatchOp>, String> {
    let lines: Vec<&str> = input.lines().collect();
    let begin = lines
        .iter()
        .position(|line| line.trim() == BEGIN_MARKER)
        .ok_or_else(|| format!("patch must start with \"{BEGIN_MARKER}\""))?;
    let end = lines
        .iter()
        .rposition(|line| line.trim() == END_MARKER)
        .ok_or_else(|| format!("patch must end with \"{END_MARKER}\""))?;
    if end <= begin {
        return Err(format!(
            "\"{END_MARKER}\" appears before \"{BEGIN_MARKER}\""
        ));
    }

    let mut ops: Vec<PatchOp> = Vec::new();
    let mut index = begin + 1;
    while index < end {
        let line = lines[index];
        if let Some(path) = line.strip_prefix(ADD_PREFIX) {
            let path = path.trim().to_string();
            if path.is_empty() {
                return Err("Add File needs a path".to_string());
            }
            let mut content_lines: Vec<String> = Vec::new();
            index += 1;
            while index < end && !lines[index].starts_with("*** ") {
                let raw = lines[index];
                if let Some(added) = raw.strip_prefix('+') {
                    content_lines.push(added.to_string());
                } else if raw.is_empty() {
                    content_lines.push(String::new());
                } else {
                    return Err(format!(
                        "Add File {path}: every line must start with \"+\", got: {raw}"
                    ));
                }
                index += 1;
            }
            let mut content = content_lines.join("\n");
            if !content.is_empty() {
                content.push('\n');
            }
            ops.push(PatchOp::AddFile { path, content });
        } else if let Some(path) = line.strip_prefix(DELETE_PREFIX) {
            let path = path.trim().to_string();
            if path.is_empty() {
                return Err("Delete File needs a path".to_string());
            }
            ops.push(PatchOp::DeleteFile { path });
            index += 1;
        } else if let Some(path) = line.strip_prefix(UPDATE_PREFIX) {
            let path = path.trim().to_string();
            if path.is_empty() {
                return Err("Update File needs a path".to_string());
            }
            index += 1;
            let mut move_to = None;
            if index < end
                && let Some(target) = lines[index].strip_prefix(MOVE_PREFIX)
            {
                move_to = Some(target.trim().to_string());
                index += 1;
            }
            let mut chunks: Vec<PatchChunk> = Vec::new();
            let mut current = PatchChunk {
                anchor: None,
                old_lines: Vec::new(),
                new_lines: Vec::new(),
            };
            let mut current_open = false;
            while index < end && !section_header(lines[index]) {
                let raw = lines[index];
                if raw.trim() == EOF_MARKER {
                    index += 1;
                    continue;
                }
                if let Some(anchor) = raw.strip_prefix("@@") {
                    if current_open {
                        chunks.push(current.clone());
                    }
                    let anchor = anchor.trim();
                    current = PatchChunk {
                        anchor: if anchor.is_empty() {
                            None
                        } else {
                            Some(anchor.to_string())
                        },
                        old_lines: Vec::new(),
                        new_lines: Vec::new(),
                    };
                    current_open = true;
                } else if let Some(added) = raw.strip_prefix('+') {
                    current.new_lines.push(added.to_string());
                    current_open = true;
                } else if let Some(removed) = raw.strip_prefix('-') {
                    current.old_lines.push(removed.to_string());
                    current_open = true;
                } else {
                    // Context: a leading space by the format, but models
                    // sometimes emit genuinely empty context lines.
                    let context = raw.strip_prefix(' ').unwrap_or(raw);
                    current.old_lines.push(context.to_string());
                    current.new_lines.push(context.to_string());
                    current_open = true;
                }
                index += 1;
            }
            if current_open {
                chunks.push(current);
            }
            if chunks.is_empty() {
                return Err(format!("Update File {path}: no hunks"));
            }
            ops.push(PatchOp::UpdateFile {
                path,
                move_to,
                chunks,
            });
        } else if line.trim().is_empty() {
            index += 1;
        } else {
            return Err(format!(
                "expected \"{ADD_PREFIX}\", \"{UPDATE_PREFIX}\", or \"{DELETE_PREFIX}\", got: {line}"
            ));
        }
    }
    if ops.is_empty() {
        return Err("patch contains no file operations".to_string());
    }
    Ok(ops)
}

fn section_header(line: &str) -> bool {
    line.starts_with(ADD_PREFIX)
        || line.starts_with(UPDATE_PREFIX)
        || line.starts_with(DELETE_PREFIX)
}

/// Apply an Update File section's hunks to the original content. Pure so
/// the loop can compute every result before writing anything.
pub(crate) fn apply_update(original: &str, chunks: &[PatchChunk]) -> Result<String, String> {
    let had_trailing_newline = original.ends_with('\n') || original.is_empty();
    let mut lines: Vec<String> = original.lines().map(|l| l.to_string()).collect();
    let mut cursor = 0usize;
    for (chunk_index, chunk) in chunks.iter().enumerate() {
        if let Some(anchor) = chunk.anchor.as_deref() {
            cursor = seek_anchor(&lines, cursor, anchor).ok_or_else(|| {
                format!(
                    "hunk {}: the @@ anchor `{anchor}` was not found in the file. Re-read the file and copy an anchor line that exists exactly as written.",
                    chunk_index + 1
                )
            })?;
        }
        if chunk.old_lines.is_empty() {
            // Pure addition: after the anchor when one was given, else at
            // the end of the file.
            let at = if chunk.anchor.is_some() {
                cursor
            } else {
                lines.len()
            };
            for (offset, line) in chunk.new_lines.iter().enumerate() {
                lines.insert(at + offset, line.clone());
            }
            cursor = at + chunk.new_lines.len();
            continue;
        }
        let start = find_span(&lines, cursor, &chunk.old_lines).ok_or_else(|| {
            format!(
                "hunk {}: the context lines did not match the file at or after line {} (first unmatched line: `{}`). The surrounding code has drifted from your patch. Re-read the file and rewrite this hunk against its current text.",
                chunk_index + 1,
                cursor + 1,
                chunk
                    .old_lines
                    .first()
                    .map(String::as_str)
                    .unwrap_or_default()
            )
        })?;
        lines.splice(
            start..start + chunk.old_lines.len(),
            chunk.new_lines.clone(),
        );
        cursor = start + chunk.new_lines.len();
    }
    let mut result = lines.join("\n");
    if had_trailing_newline && !result.is_empty() {
        result.push('\n');
    }
    Ok(result)
}

/// Position just past the first line at or after `from` matching the anchor
/// text, exact first, then trimmed.
fn seek_anchor(lines: &[String], from: usize, anchor: &str) -> Option<usize> {
    for (index, line) in lines.iter().enumerate().skip(from) {
        if line == anchor {
            return Some(index + 1);
        }
    }
    let trimmed = anchor.trim();
    for (index, line) in lines.iter().enumerate().skip(from) {
        if line.trim() == trimmed {
            return Some(index + 1);
        }
    }
    None
}

/// First index at or after `from` where `wanted` matches, at the loosest
/// fuzz level that finds one: exact, then trailing-whitespace-insensitive,
/// then fully trimmed.
fn find_span(lines: &[String], from: usize, wanted: &[String]) -> Option<usize> {
    if wanted.is_empty() || lines.len() < wanted.len() {
        return None;
    }
    let last_start = lines.len() - wanted.len();
    for fuzz in 0..3u8 {
        for start in from..=last_start {
            let matches = wanted.iter().enumerate().all(|(offset, want)| {
                let have = &lines[start + offset];
                match fuzz {
                    0 => have == want,
                    1 => have.trim_end() == want.trim_end(),
                    _ => have.trim() == want.trim(),
                }
            });
            if matches {
                return Some(start);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(body: &str) -> String {
        format!("*** Begin Patch\n{body}\n*** End Patch")
    }

    #[test]
    fn parses_add_update_delete_and_move_sections() {
        let patch = envelope(
            "*** Add File: docs/new.md\n+# Title\n+body\n\
             *** Update File: src/lib.rs\n*** Move to: src/lib2.rs\n@@ fn main\n old\n-gone\n+here\n\
             *** Delete File: src/dead.rs",
        );
        let ops = parse_patch(&patch).expect("parses");
        assert_eq!(ops.len(), 3);
        assert_eq!(
            ops[0],
            PatchOp::AddFile {
                path: "docs/new.md".to_string(),
                content: "# Title\nbody\n".to_string()
            }
        );
        match &ops[1] {
            PatchOp::UpdateFile {
                path,
                move_to,
                chunks,
            } => {
                assert_eq!(path, "src/lib.rs");
                assert_eq!(move_to.as_deref(), Some("src/lib2.rs"));
                assert_eq!(chunks.len(), 1);
                assert_eq!(chunks[0].anchor.as_deref(), Some("fn main"));
                assert_eq!(chunks[0].old_lines, vec!["old", "gone"]);
                assert_eq!(chunks[0].new_lines, vec!["old", "here"]);
            }
            other => panic!("expected update, got {other:?}"),
        }
        assert_eq!(
            ops[2],
            PatchOp::DeleteFile {
                path: "src/dead.rs".to_string()
            }
        );
    }

    #[test]
    fn refuses_envelope_without_markers_or_ops() {
        assert!(parse_patch("no markers").is_err());
        assert!(parse_patch("*** Begin Patch\n*** End Patch").is_err());
        assert!(parse_patch(&envelope("*** Frobnicate File: x")).is_err());
        assert!(parse_patch(&envelope("*** Add File: a.txt\nnot plus")).is_err());
    }

    #[test]
    fn applies_hunks_sequentially_with_fuzz() {
        let original = "fn one() {}\nfn two() {\n    a();  \n    b();\n}\nfn three() {}\n";
        let chunks = vec![
            PatchChunk {
                anchor: Some("fn two() {".to_string()),
                // Trailing whitespace differs from the file: fuzz level 1.
                old_lines: vec!["    a();".to_string(), "    b();".to_string()],
                new_lines: vec!["    a();".to_string(), "    c();".to_string()],
            },
            PatchChunk {
                anchor: None,
                old_lines: vec!["fn three() {}".to_string()],
                new_lines: vec!["fn three() { three_body(); }".to_string()],
            },
        ];
        let result = apply_update(original, &chunks).expect("applies");
        // The patch's context lines replace the file's - the trailing
        // whitespace on a();  normalizes to what the patch wrote.
        assert_eq!(
            result,
            "fn one() {}\nfn two() {\n    a();\n    c();\n}\nfn three() { three_body(); }\n"
        );
    }

    #[test]
    fn first_match_after_cursor_not_before() {
        // The same span exists twice; a chunk after an anchor must bind to
        // the occurrence AFTER the anchor, not the earlier one.
        let original = "x = 1\nmarker\nx = 1\n";
        let chunks = vec![PatchChunk {
            anchor: Some("marker".to_string()),
            old_lines: vec!["x = 1".to_string()],
            new_lines: vec!["x = 2".to_string()],
        }];
        let result = apply_update(original, &chunks).expect("applies");
        assert_eq!(result, "x = 1\nmarker\nx = 2\n");
    }

    #[test]
    fn pure_addition_appends_at_eof_or_after_anchor() {
        let original = "a\nb\n";
        let append = vec![PatchChunk {
            anchor: None,
            old_lines: Vec::new(),
            new_lines: vec!["c".to_string()],
        }];
        assert_eq!(
            apply_update(original, &append).expect("appends"),
            "a\nb\nc\n"
        );
        let after_anchor = vec![PatchChunk {
            anchor: Some("a".to_string()),
            old_lines: Vec::new(),
            new_lines: vec!["a2".to_string()],
        }];
        assert_eq!(
            apply_update(original, &after_anchor).expect("inserts"),
            "a\na2\nb\n"
        );
    }

    #[test]
    fn missing_context_names_the_hunk_and_line() {
        let original = "a\n";
        let chunks = vec![PatchChunk {
            anchor: None,
            old_lines: vec!["never there".to_string()],
            new_lines: vec!["x".to_string()],
        }];
        let error = apply_update(original, &chunks).expect_err("must fail");
        assert!(error.contains("hunk 1"), "{error}");
        assert!(error.contains("never there"), "{error}");
    }

    #[test]
    fn touched_paths_include_move_target() {
        let op = PatchOp::UpdateFile {
            path: "a.rs".to_string(),
            move_to: Some("b.rs".to_string()),
            chunks: Vec::new(),
        };
        assert_eq!(op.touched_paths(), vec!["a.rs", "b.rs"]);
    }

    #[test]
    fn tolerates_end_of_file_marker_and_blank_separators() {
        let patch = envelope(
            "*** Update File: a.txt\n@@\n-old\n+new\n*** End of File\n\n*** Delete File: b.txt",
        );
        let ops = parse_patch(&patch).expect("parses");
        assert_eq!(ops.len(), 2);
    }
}
