# forge-k8j: forge-daemon node registry test compile fix (2026-02-13)

## Summary
- Fixed `forge-daemon` test compile failure in `node_registry` test module.

## Root cause
- `crates/forge-daemon/src/node_registry.rs` test helper used `Path` in `cleanup_dir` signature but test module no longer imported `std::path::Path`.

## Change
- Added `use std::path::Path;` in `#[cfg(test)] mod tests` of `crates/forge-daemon/src/node_registry.rs`.

## Validation
- `cargo test -p forge-daemon --lib`
  - Result: `141 passed; 0 failed`.

