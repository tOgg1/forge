# forge-82q: completion recursion trim + golden refresh (2026-02-13)

## Summary
- Fixed completion surface recursion over deep repeated subcommand paths.
- Refreshed completion goldens for current CLI command surface (new command families/subcommands).

## Changes
- `crates/forge-cli/src/completion.rs`
  - Reduced help traversal depth from `3` to `2` to avoid repeated third-level recursive paths (e.g. `.../check/check`, `.../ack/ack`) while keeping useful root + subcommand coverage.
- Updated completion goldens:
  - `crates/forge-cli/tests/golden/completion/bash.txt`
  - `crates/forge-cli/tests/golden/completion/zsh.txt`
  - `crates/forge-cli/tests/golden/completion/fish.txt`
  - Regenerated from current `forge completion <shell>` output.

## Validation
- `cargo test -p forge-cli --test completion_command_test` (green: 6 passed).
- `cargo test -p forge-cli --tests` proceeds past completion; later failures are unrelated profile alias env drift (tracked separately).

