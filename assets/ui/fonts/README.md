# MDx UI Fonts

These assets are vendored for deterministic local UI proof. Production app CSS must load them through `generated/ui/mdx-theme.css`; app surfaces should use the generated `--mdx-font-*` tokens instead of hard-coded font stacks.

## Families

| Family | Use | Source | License posture |
| --- | --- | --- | --- |
| DM Sans | Body and interface text | `@fontsource-variable/dm-sans@5.2.8` | SIL OFL 1.1, see `OFL-1.1.txt` |
| Satoshi | Display and major product headings | Fontshare CDN asset snapshot | ITF Free Font License, approved for first public candidate with this notice retained |
| JetBrains Mono | Code, receipt ids, compact proof labels | `@fontsource-variable/jetbrains-mono@5.2.8` | SIL OFL 1.1, see `OFL-1.1.txt` |

## Rules

- Do not add Google Fonts, Fontshare, or other CDN CSS to app surfaces.
- Do not hand-edit `generated/ui/mdx-theme.css`; update `crates/mdx-codegen/src/ui.rs` and run `make generate`.
- Keep Satoshi bundled only as an MDx UI asset with this notice retained. Do not republish MDx as a standalone font mirror or font package without a separate license review.
