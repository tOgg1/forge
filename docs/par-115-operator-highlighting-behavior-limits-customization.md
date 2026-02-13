# PAR-115 operator docs: highlighting behavior, limits, customization

Scope delivered:
- Documented how `forge logs` highlighting is built and ordered.
- Documented operator-facing controls (flags + env).
- Documented current limitations and practical workarounds.
- Documented performance tuning guidance for large logs.
- Documented troubleshooting flow for no-color and terminal capability issues.

## Highlighting model

`forge logs` rendering pipeline (`crates/forge-cli/src/logs.rs`):
1. Line intake:
   - `collect_render_lines` processes each line.
   - Claude stream-json lines are converted to readable text via `maybe_render_claude_stream_line`.
2. Section-aware pass:
   - `render_section_aware` uses `SectionParser` to detect headers, role markers, status, tool calls, thinking blocks, error blocks, command transcripts, and structured data.
   - Error blocks route through `render_error_lines`.
   - Command-looking blocks route through `render_command_lines`.
   - Structured lines route through `render_structured_data_line`.
3. Diff pass:
   - Final pass through `render_diff_lines` (or incremental variant while following).

Semantic token model (`crates/forge-cli/src/highlight_spec.rs`):
- 19 classified token kinds + plain fallback.
- Deterministic precedence (lower numeric precedence wins overlap).
- No-color fallback keeps text and uses signifiers only when needed:
  - `[ERROR]`, `[WARN]`, `==`, `>>`, `$`.

## Controls (flags + env)

CLI flags (`forge logs`):
- `--no-color`: disable colored rendering.
- `--raw`: bypass Claude stream-json transformation.
- `--compact`: collapse long thinking/code-fence interiors.
- `--lines N`: tail limit (default `50`).
- `--since VAL`: filter by timestamp marker.
- `--follow`: stream updates.
- `--all`: show all loops in repo.

Environment controls:
- `NO_COLOR`:
  - Disables color in `forge logs` renderer (`colors_enabled` path).
  - Also treated as no-color in theme resolver helpers.
- `FORGE_LOG_COLOR_CAPABILITY`:
  - Supported by theme resolver helpers with values:
    - `ansi16`/`16`
    - `ansi256`/`256`
    - `truecolor`/`24bit`
- `FORGE_LOG_COLOR_SCHEME`:
  - Supported by theme resolver helpers with values: `dark`, `light`.
- `TERM`, `COLORTERM`, `COLORFGBG`:
  - Used by theme resolver helper detection.

Important current-state note:
- `forge logs` renderers currently call `style_span(..., use_color)` default path.
- Capability/tone resolver helpers are implemented but not yet wired into runtime `forge logs` output.

## Known limitations

- Capability/tone overrides not active in `forge logs` runtime:
  - `FORGE_LOG_COLOR_CAPABILITY` and `FORGE_LOG_COLOR_SCHEME` exist in helper API, but are not yet plumbed through logs renderers.
- `--since` filtering currently matches only RFC3339 UTC bracketed line prefixes:
  - Expected line prefix format: `[YYYY-MM-DDTHH:MM:SSZ]`.
  - Non-bracketed timestamps and non-UTC/offset variants are ignored by filter matcher.
- `--compact` collapses only `thinking` and `code fence` interiors.
- Structured data parsing is heuristic line parsing (JSON/YAML/TOML line-level), not full multi-line document parse.
- Command transcript detection uses prompt/exit heuristics; uncommon prompt shapes may remain plain.
- Incremental diff renderer has bounded pending change memory (`DEFAULT_PENDING_LIMIT = 256`).

## Performance guidance

For high-volume logs:
- Prefer narrow tails:
  - `forge logs <loop> --lines 200`
- Prefer timestamp filtering when logs contain bracketed RFC3339 UTC stamps:
  - `forge logs <loop> --since 2026-02-13T06:00:00Z`
- Enable collapse for long reasoning/code sections:
  - `forge logs <loop> --compact`
- Use raw mode when format-preservation/throughput matters more than readability:
  - `forge logs <loop> --raw`
- Avoid `--all --follow` unless needed; it multiplies parsing/rendering workload.

## Troubleshooting

### No colors expected but still plain output

Checks:
1. Confirm `--no-color` not passed.
2. Confirm `NO_COLOR` not exported:
   - `env | rg '^NO_COLOR='`
3. Confirm terminal is not stripping ANSI (e.g. log capture wrappers).

### `--since` appears ignored

Checks:
1. Ensure log lines start with bracketed RFC3339 UTC:
   - Example: `[2026-02-13T06:15:07Z] ...`
2. Use an explicit UTC timestamp value for `--since`.
3. If logs lack bracketed timestamps, rely on `--lines` and/or external filtering.

### Terminal capability mismatch expectations

Current behavior:
- Runtime `forge logs` output uses current default color-style path.
- Capability/tone helper overrides are not yet connected to runtime renderer output.

Practical workaround:
- Use `--no-color` for deterministic monochrome output across terminals.
