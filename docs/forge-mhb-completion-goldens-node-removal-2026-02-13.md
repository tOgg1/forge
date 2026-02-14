# forge-mhb: completion golden drift after root `node` removal (2026-02-13)

## Summary
- Fixed completion golden drift introduced by removing legacy top-level `node` command from root dispatch/help.
- Refreshed completion snapshots to match current root command surface.

## Changes
- Regenerated:
  - `crates/forge-cli/tests/golden/completion/bash.txt`
  - `crates/forge-cli/tests/golden/completion/zsh.txt`
  - `crates/forge-cli/tests/golden/completion/fish.txt`

## Validation
- `cargo test -p forge-cli --test completion_command_test` (6 passed)
