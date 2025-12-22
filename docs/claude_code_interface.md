# Claude Code CLI interface notes

Source: local `claude` CLI help output and a probe run on this host.
Version observed: `claude --version` -> `2.0.73 (Claude Code)`.

## Invocation and spawn flags

- Usage: `claude [options] [command] [prompt]`.
- Interactive by default; non-interactive requires `-p/--print`.
- `--print` skips the workspace trust dialog; only use in trusted directories.
- Common spawn flags:
  - `--model <model>`: alias (e.g., `sonnet`, `opus`) or full model name.
  - `--agent <agent>`: override agent setting; `--agents <json>` defines custom agents.
  - `--system-prompt` / `--append-system-prompt`.
  - `--settings <file-or-json>` and `--setting-sources <sources>`.
  - `--add-dir <directories...>`: allow tool access to extra dirs.
  - `--tools`, `--allowedTools`, `--disallowedTools`.
  - `--mcp-config <configs...>` and `--strict-mcp-config`.
  - `--plugin-dir <paths...>` to load plugins for a session.
  - Session control: `--continue`, `--resume`, `--fork-session`, `--session-id`.
  - `--no-session-persistence` (only with `--print`).
  - Debug/verbosity: `--debug [filter]`, `--verbose`.
  - Structured output: `--json-schema <schema>` for JSON validation (print mode only).
  - Budget/fallback: `--max-budget-usd <amount>`, `--fallback-model <model>` (print mode only).
  - Feature toggles: `--disable-slash-commands`, `--ide`, `--chrome` / `--no-chrome`.

## Output formats and state indicators

- `--print` enables non-interactive output.
- `--output-format` options (only with `--print`):
  - `text` (default), `json` (single result), `stream-json` (line-delimited stream).
- `--input-format` options (only with `--print`): `text` (default), `stream-json`.
- `--include-partial-messages` and `--replay-user-messages` apply to stream-json mode.
- Observed requirement: `--output-format=stream-json` requires `--verbose` (error otherwise).

### Observed stream-json init event

A `claude -p --output-format stream-json --verbose "Hello"` probe emits a line like:

```
{"type":"system","subtype":"init","session_id":"...","tools":[...],"mcp_servers":[...],"model":"...","permissionMode":"...","slash_commands":[...],"apiKeySource":"...","claude_code_version":"...","agents":[...],"skills":[],"plugins":[]}
```

Notes:
- Output is line-delimited JSON objects (one per line).
- `type`/`subtype` and `permissionMode` are useful for state inference.
- `tools`, `mcp_servers`, and `claude_code_version` can be recorded for diagnostics.

## Approval / permissions mechanisms

Relevant flags from `claude --help`:
- `--permission-mode <mode>` choices: `acceptEdits`, `bypassPermissions`, `default`, `delegate`, `dontAsk`, `plan`.
- `--dangerously-skip-permissions` bypasses all permission checks.
- `--allow-dangerously-skip-permissions` enables the bypass option but does not force it.
- `--add-dir` and tool allow/deny lists (`--allowedTools`, `--disallowedTools`) gate tool access.

Implication for Swarm adapter: treat permission-mode and skip flags as approval policy hints, and
surface the chosen mode in logs/events.

## Log/output patterns

- Text mode: raw assistant output to stdout.
- JSON mode: single JSON object (structure not observed here).
- Stream JSON: per-line JSON objects. At least one `system/init` event appears at startup,
  even before auth failures; additional events should be expected as the session progresses.

## Commands surfaced by help

- `claude mcp ...` (server management: add/list/get/serve).
- `claude plugin ...` (install/update/enable/disable).
- `claude setup-token`, `doctor`, `update`, `install`.

## Open questions / follow-ups

- Confirm stream-json event types beyond `system/init` (requires authenticated run).
- Verify approval prompt messages and their exact stdout/stderr patterns.
- Check whether `claude` emits explicit state markers for idle/working outside stream-json.
