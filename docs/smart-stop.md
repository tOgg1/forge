# Smart Stop (Loops)

Forge loops can be configured with "smart stop" rules at loop creation time (`forge up`, `forge scale`).

Two types:

- **Quantitative stop**: run a shell command; match exit code/stdout/stderr; decision `stop|continue`.
- **Qualitative stop**: every N main iterations, run a "judge" iteration (same agent). Agent must print `0` (stop) or `1` (continue) as first token.

## Quantitative stop

Runs `bash -lc <cmd>` with workdir set to the repo root.

Cadence:

- `--quantitative-stop-every N` (every N total iterations)
- `--quantitative-stop-when before|after|both`

Matching:

- `--quantitative-stop-exit-codes 0,1,2`
- `--quantitative-stop-exit-invert`
- `--quantitative-stop-stdout any|empty|nonempty`
- `--quantitative-stop-stderr any|empty|nonempty`
- `--quantitative-stop-stdout-regex <re2>`
- `--quantitative-stop-stderr-regex <re2>`

Decision:

- `--quantitative-stop-decision stop|continue`

Example: stop when no epics remain.

```bash
forge up --name review \
  --quantitative-stop-cmd 'sv count --epic | rg -q "^0$"' \
  --quantitative-stop-exit-codes 0 \
  --quantitative-stop-every 1 \
  --quantitative-stop-when before
```

## Qualitative stop

Cadence:

- `--qualitative-stop-every N` (every N *main* iterations)

Prompt:

- `--qualitative-stop-prompt <path|prompt-name>` (resolved like other prompts)
- `--qualitative-stop-prompt-msg "<inline prompt>"`

Invalid output handling:

- `--qualitative-stop-on-invalid stop|continue` (default: `continue`)

Example: every 5 iters, ask agent if it should stop.

```bash
forge up --name review \
  --qualitative-stop-every 5 \
  --qualitative-stop-prompt stop-judge
```

## Notes / semantics

- Qualitative stop is implemented as a special next iteration (`prompt_source=qual_stop`).
- Operator `--next-prompt` overrides take precedence for that iteration (qual stop suppressed).
- `forge run <loop>` (single iteration) does not run qualitative stop checks.

