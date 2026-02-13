# TUI-921 quick peek popups

Task: `forge-6ad`  
Status: delivered

## Scope

- Inline peek popup model for IDs in-place, no navigation switch.
- Supported entity previews:
  - loop ID
  - task ID
  - fmail thread ID
  - file path
  - commit hash
- Peek trigger key:
  - `Space` opens focused entity preview.
  - any key dismisses currently open popup.

## Implementation

- New module: `crates/forge-tui/src/quick_peek_popups.rs`
- Exported from crate root: `crates/forge-tui/src/lib.rs`

### Core API

- `PeekEntityRef` + `PeekEntityKind`: normalized target identity.
- `QuickPeekCatalog`: in-memory preview records.
- `build_quick_peek_popup(...)`: deterministic popup payload generation.
- `QuickPeekState` + `handle_quick_peek_key(...)`:
  - open on `Space` with focused target
  - dismiss on any key if popup visible
- File preview behavior:
  - first 20 lines max
  - `... +N more lines` indicator
  - recent-change summary entries

## Validation

- `cargo fmt --all`
- `cargo test -p forge-tui quick_peek_popups::`
- `cargo build -p forge-tui --lib`
