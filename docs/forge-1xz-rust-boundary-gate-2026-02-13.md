# forge-1xz: CI rust-boundary gate broken (missing cmd/rust-boundary-check)

## Problem

CI `rust-boundary` job called `scripts/rust-boundary-check.sh`, which executed:

- `go run ./cmd/rust-boundary-check`

But `old/go/cmd/rust-boundary-check` did not exist, so the gate failed before policy validation ran.

## Changes

- Added new checker command:
  - `old/go/cmd/rust-boundary-check/main.go`
- Added regression tests:
  - `old/go/cmd/rust-boundary-check/main_test.go`
- Updated boundary policy map to include active workspace crate:
  - `docs/rust-crate-boundaries.json`: added `forge-agent` at layer `3`
- Updated policy doc layer listing:
  - `docs/rust-crate-boundary-policy.md`: layer 3 now includes `forge-agent`

## Validation

- `cd old/go && env -u GOROOT -u GOTOOLDIR go test ./cmd/rust-boundary-check -count=1`
- `scripts/rust-boundary-check.sh`

Both passed.
