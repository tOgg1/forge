# PAR-109 theme engine (ANSI16/ANSI256/truecolor + no-color policy)

Scope delivered:
- Added centralized capability-aware log highlighting theme policy in `crates/forge-cli/src/highlight_spec.rs`.
- Added deterministic token-to-style mapping for:
  - `ansi16`
  - `ansi256`
  - `truecolor`
- Added light/dark tone variants to keep warning/number/code colors readable across terminal backgrounds.
- Added explicit no-color policy resolution (`--no-color` and `NO_COLOR`) with deterministic precedence.
- Added theme resolution helpers:
  - `ThemeEnvHints::detect`
  - `resolve_theme`
  - `resolve_theme_from_env`
- Added capability/tone-aware rendering entrypoint:
  - `style_span_with_theme`

Validation:
- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test --workspace`

Additional regression fix required by workspace gate:
- Fixed diff continuation parsing in `crates/forge-cli/src/section_parser.rs` so unified-diff context lines with leading space are classified correctly.
