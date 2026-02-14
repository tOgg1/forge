# forge-0wg: rust-frankentui-pin-check fails when `rg` missing

## Problem

CI `rust-quality` failed with:

- `scripts/rust-frankentui-pin-check.sh: line 12: rg: command not found`

The script hard-required `rg`, but runner images can omit it.

## Changes

- Updated `scripts/rust-frankentui-pin-check.sh`:
  - Added `search_pattern()` helper.
  - Uses `rg -n` when available.
  - Falls back to `grep -En` when `rg` is unavailable.
  - Preserved the same pin assertions.

- Added regression test:
  - `old/go/internal/doccheck/rust_frankentui_pin_check_script_test.go`
  - Verifies fallback contract markers stay present in script (`command -v rg`, `grep -En`, `search_pattern`).

## Validation

- `scripts/rust-frankentui-pin-check.sh`
- `PATH=/usr/bin:/bin:/usr/sbin:/sbin scripts/rust-frankentui-pin-check.sh`
- `cd old/go && env -u GOROOT -u GOTOOLDIR go test ./internal/doccheck -run '^TestRustFrankentuiPinCheckScriptSupportsGrepFallback$' -count=1`

All passed.
