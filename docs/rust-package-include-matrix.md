# Rust Rewrite Package Include Matrix

Task: `forge-3sw`
Freeze date: 2026-02-09

## Include set (mapped to Rust crate owners)

| Go package | Rust target crate | Status |
|---|---|---|
| `internal/config` | `forge-core` | include |
| `internal/db` | `forge-db` | include |
| `internal/models` | `forge-core` | include |
| `internal/loop` | `forge-loop` | include |
| `internal/harness` | `forge-loop` | include |
| `internal/hooks` | `forge-core` | include |
| `internal/logging` | `forge-core` | include |
| `internal/events` | `forge-core` | include |
| `internal/queue` | `forge-loop` | include |
| `internal/templates` | `forge-core` | include |
| `internal/sequences` | `forge-core` | include |
| `internal/workflows` | `forge-core` | include |
| `internal/skills` | `forge-core` | include |
| `internal/procutil` | `forge-core` | include |
| `internal/names` | `forge-core` | include |
| `internal/forged` | `forge-daemon` | include |
| `internal/agent` | `forge-runner` | include |
| `internal/agent/runner` | `forge-runner` | include |
| `internal/node` | `forge-core` | include |
| `internal/workspace` | `forge-core` | include |
| `internal/tmux` | `forge-core` | include |
| `internal/ssh` | `forge-core` | include |
| `internal/scheduler` | `forge-loop` | include |
| `internal/state` | `forge-core` | include |
| `internal/account` | `forge-core` | include (`caam` excluded) |
| `internal/looptui` | `forge-tui` | include |
| `internal/fmail` | `fmail-core` + `fmail-cli` | include |
| `internal/fmailtui` | `fmail-tui` | include |
| `internal/agentmail` | `fmail-core` | include |
| `internal/teammsg` | `fmail-core` | include |

## Explicit non-include set

| Go package | Classification | Reason |
|---|---|---|
| `internal/account/caam` | drop | Legacy account import path |
| `internal/recipes` | drop | Legacy recipe subsystem |
| `internal/tui` | drop | Replaced by loop TUI path |
| `internal/doccheck` | tooling-only | Doc consistency test helpers |
| `internal/parity` | tooling-only | Parity harness/testing utilities |
| `internal/testutil` | test-only | Test scaffolding |
| `internal/testutil/mocks` | test-only | Test mocks |

No included runtime package is left unmapped.

## Static import reachability check (documented)

Generate runtime dependency closure:

```bash
env -u GOROOT -u GOTOOLDIR go list -deps \
  ./cmd/forge ./cmd/forged ./cmd/forge-agent-runner ./cmd/fmail ./cmd/fmail-tui \
  | sort -u > build/rust-baseline/reachability/go-runtime-deps.txt
```

Then compare `internal/*` entries from that file against this matrix and
`docs/rust-legacy-drop-list.md` for drift review.
