# Go Source Reorg Plan (`legacy/old-go`)

Task: `forge-jtw`  
Status: staged plan (no move executed yet)

## Goal

Move the current Go implementation under `legacy/old-go` without breaking
build/test flows during transition.

## Build guard

- Guard command: `make go-layout-guard`
- Guard script: `scripts/go-layout-guard.sh`
- Modes:
  - `root` (default): Go source must stay at repo root; `legacy/old-go` must be absent.
  - `mixed`: both root Go tree and `legacy/old-go` must be present.
  - `legacy`: only `legacy/old-go` Go tree may exist.

Current repo is pinned to `GO_LAYOUT_MODE=root`.

## Staged execution

1. Stage A: pre-move hardening (current)
   - Keep all build/test commands on current root layout.
   - Enforce `make go-layout-guard` in `make build`.
2. Stage B: dual-tree transition
   - Copy Go tree to `legacy/old-go` (no path switch yet).
   - Run guards with `GO_LAYOUT_MODE=mixed`.
   - Keep CI/build output parity green before switching paths.
3. Stage C: path switch
   - Update build/test path variables to `legacy/old-go/...`.
   - Flip guard mode to `GO_LAYOUT_MODE=legacy`.
   - Validate full Go + parity suite.
4. Stage D: post-switch cleanup
   - Remove temporary dual-tree compatibility shims.
   - Keep guard in `legacy` mode until Go tree retirement is complete.

## Required validation per stage

- `make go-layout-guard`
- `go test ./...`
- rust parity gates unaffected (`scripts/rust-quality-check.sh` / CI parity jobs)
