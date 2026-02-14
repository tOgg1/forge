# Forge TUI Unified FrankenTUI Bootstrap Plan (2026-02-13)

Status: active execution plan
Source baseline: `frankentui/docs/getting-started.md`
Related project: `sv project prj-9e5nsdh9`
Root epic: `forge-w4g`

## 1. Non-negotiable getting-started alignment

The bootstrap path must follow FrankenTUI's runtime model:

1. `Model`-driven app loop (`init`, `update`, `view`, `subscriptions`).
2. Runtime-owned render loop via `App::new(model).run()`.
3. Message-driven updates (`Event -> Message -> update -> view`).
4. Runtime frame/render/diff ownership (no custom poll/dirty loop in primary path).

## 2. Repo-level architecture decisions

1. Runtime ownership: **Option A** (direct `Model` in `forge-tui`).
2. Adapter role: Forge widget/style bridge and translators, not runtime mediator.
3. Interactive fallback: no implicit snapshot fallback in normal runtime path.
4. Dev fallback: explicit `FORGE_TUI_DEV_SNAPSHOT_FALLBACK=1` only.

## 3. Toolchain policy

Pinned rev in repo:
`23429fac0e739635c7b8e0b995bde09401ff6ea0`

Verified on stable toolchain:

1. `ftui` with `features=["runtime"]` -> `cargo check` OK.
2. `ftui` with `features=["runtime","crossterm"]` -> `cargo check` OK.

Policy for now: stable-first. Revisit nightly only if later feature adoption requires it.

## 4. Bootstrap execution sequence (unified)

## Phase A: Runtime foundation

1. Verify compile matrix and toolchain lock (`forge-6b9`, `forge-n6q`).
2. Build minimal hello model using getting-started pattern (`forge-9f7`).
3. Implement `ForgeShell` message/state model (`forge-jee`).
4. Replace interactive manual runtime loop with `App::new(...).run()` (`forge-j2z`).
5. Keep explicit dev fallback only (`forge-r9q`).

## Phase B: Adapter bridge

1. Confirm adapter boundary (`forge-4yc`).
2. Implement style translation (`forge-hgn`).
3. Expose minimum widget surface: Table/StatusLine/Badge/Flex (`forge-way`).
4. Add parity tests for cells/styles/events (`forge-67w`, `forge-znd`).

## Phase C: Shell + panes on upstream runtime

1. Shell chrome rewrite (`forge-ke7`, `forge-a2p`, `forge-qbh`, `forge-c6m`, `forge-ray`).
2. Pane rewrites: Overview/Runs/Inbox/Multi/Logs (`forge-wf0`, `forge-hza`, `forge-ch4`, `forge-b2q`, `forge-n3h`).
3. Preserve extension seams for post-cutover features.

## Phase D: Gates and cutover

1. Snapshot gates: 80x24/120x40/200x50 (`forge-438`).
2. Unicode truncation regressions (`forge-4tj`).
3. Workflow gate (<60s first-time flow) (`forge-1cf`).
4. Flip upstream default + demote/remove legacy paths (`forge-hsv`, `forge-apf`, `forge-mat`).

## 5. First checkpoint definition

A checkpoint is testable when all are true:

1. `cargo build -p forge-tui` passes.
2. No byte-slice truncation panic paths in touched shell/anchor code.
3. Interactive runtime failure behavior:
- default: exits with explicit error, no silent fallback.
- dev mode: fallback only when `FORGE_TUI_DEV_SNAPSHOT_FALLBACK=1`.
4. Unit regressions for unicode truncation + fallback env parsing are green.

## 6. Current completed slice (checkpoint-1)

Completed tasks:

1. `forge-6b9` compile matrix verification.
2. `forge-r9q` explicit-only dev fallback behavior.
3. `forge-4tj` unicode-safe truncation first pass + regressions.

## 7. Test commands for checkpoint-1

```bash
cargo build -p forge-tui
cargo test -p forge-tui trim_to_width_handles_unicode_without_panicking
cargo test -p forge-tui dev_snapshot_fallback_env_parser_matches_expected_values
cargo test -p forge-tui render_anchor_rows_truncates_unicode_without_panicking
```

Optional manual behavior check:

```bash
# default: should error-exit on runtime failure path (no implicit fallback)
# explicit dev fallback path
FORGE_TUI_DEV_SNAPSHOT_FALLBACK=1 cargo run -p forge-tui --bin forge-tui
```
