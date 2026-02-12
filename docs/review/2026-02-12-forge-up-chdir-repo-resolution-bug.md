# Bug Report: `forge up` ignores `-C/--chdir` for repo resolution

Date: 2026-02-12  
Repo: `trmdy/forge`

## Summary

`forge up` appears to resolve loop `repo_path` from the caller's current working directory, not from `-C/--chdir`.

This breaks harnesses that rely on `forge -C <workspace> up ...` to isolate runs.

## Impact

- Loops bind to wrong repo.
- Agent work lands in unintended tree (repo root pollution).
- Benchmark/task isolation fails (cross-agent contamination risk).

## Reproduction

From inside `~/Code/personal--agents-testing-grounds`:

```bash
forge -C /tmp up \
  --count 1 \
  --name repo-detect-cflag-<ts> \
  --profile codex1 \
  --spawn-owner daemon \
  --max-runtime 2m \
  --max-iterations 1 \
  --tags <ts> \
  --prompt-msg "Print pwd and exit"

forge ps --tag <ts> --json | jq -r '.[0].repo_path'
```

Observed:

```text
/Users/trmd/Code/personal--agents-testing-grounds
```

Expected:

```text
/private/tmp
```

Control case (works):

```bash
cd /tmp
forge up --count 1 --name repo-detect-cd-<ts> --profile codex1 --spawn-owner daemon --max-runtime 2m --max-iterations 1 --tags <ts> --prompt-msg "Print pwd and exit"
forge ps --tag <ts> --json | jq -r '.[0].repo_path'
```

Observed/expected:

```text
/private/tmp
```

## Expected behavior

`forge -C <dir> up ...` should behave like running `cd <dir> && forge up ...` for repo resolution and loop binding.

## Actual behavior

Global `-C/--chdir` does not affect repo resolution for `up` (or is applied too late in execution path).

## Workaround

Use subshell `cd` instead of `-C` when launching loops:

```bash
( cd "$workspace_dir" && forge up ... )
```

## Notes

This surfaced while running benchmark campaign automation that starts loops with per-agent workspace directories.

## Tracking

GitHub issue: https://github.com/trmdy/forge/issues/3
