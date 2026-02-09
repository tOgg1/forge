# Forge Rust Rewrite Scope (In-Repo)

Scoped: 2026-02-09
Owner: Tormod + Forge contributors

## Goal

Rewrite Forge to Rust, in-repo, with near 1:1 behavior parity.
Use FrankenTUI (`frankentui`) for TUI layers.
Use rewrite window to remove legacy code safely.

Companion manifest: `docs/rust-port-manifest.md`.
Baseline snapshot: `docs/rust-baseline-inventory-v2026-02-09.md`.
Parity matrix: `docs/rust-parity-matrix.md`.
Legacy drop freeze: `docs/rust-legacy-drop-list.md`.
Coverage gate policy: `docs/rust-coverage-policy.md`.
Crate boundary policy: `docs/rust-crate-boundary-policy.md`.
Crate contribution contracts: `docs/rust-crate-contracts.md`.

## Non-Negotiables

- Functional parity first. No accidental behavior drift.
- In-repo migration. No parallel external repo.
- Legacy removal via evidence, not assumptions.
- Keep shipping path open during migration.

## Current System Inventory (baseline)

### Binaries

- `forge` (CLI + loop TUI)
- `forged` (daemon, gRPC)
- `forge-agent-runner` (runner process)
- `fmail` (mail CLI, launches mail TUI on no args)
- `fmail-tui`

### Runtime Surface

- `forge --help` visible commands: 39
- Loop-centric commands active (`up`, `ps`, `logs`, `msg`, `stop`, `kill`, `resume`, `rm`, `clean`, `scale`, `queue`, `run`, `mem`, `work`, etc.)
- Legacy command framework exists; several command groups disabled in loop mode (`agent`, `workspace`, `node`, `accounts`, `vault`, `attach`, `recipe`)

### Data + Protocol

- SQLite migrations present through `012_loop_work_state`
- Tables created in up migrations: 23
- gRPC proto present: `proto/forged/v1/forged.proto`
- Generated bindings in `gen/forged/v1/`

### Code Size (Go)

- Total Go files in `cmd/ internal/ pkg/ proto/`: 582
- `internal/*.go`: 542 files
- Largest subsystems by LOC:
  - `internal/fmailtui`: 29k+
  - `internal/cli`: 26k+
  - `internal/tui`: 12k+
  - `internal/db`: 10k+
  - `internal/forged`: 8k+

### Test Baseline

- `go test ./...` passes on 2026-02-09
- Existing tests are strong; use as behavior oracle during rewrite

## FrankenTUI Reality Check (current)

Source checked: GitHub + raw docs on 2026-02-09.

- Repo active same day (`pushedAt` 2026-02-09)
- No GitHub releases listed
- Docs mark project early-stage / API moving
- Requires Rust nightly (`rust-toolchain.toml`)
- Crates currently on crates.io (checked): `ftui`, `ftui-core`, `ftui-runtime`, `ftui-widgets`, `ftui-layout`, `ftui-i18n` at `0.1.1`
- Docs still mention partial publish; doc/package status mismatch = churn signal

Implication:
- Pin exact git commit for reproducibility
- Wrap FrankenTUI behind local adapter crate to isolate churn

## Scope Strategy (recommended)

### Migration Pattern

Strangler pattern inside same repo.

- Keep Go path as reference implementation during transition
- Add Rust workspace side-by-side
- Port vertical slices with parity tests
- Flip command ownership gradually
- Remove Go legacy only after parity gates pass

### Proposed Repo Additions

```text
rust/
  Cargo.toml
  crates/
    forge-core          # domain types + policies
    forge-db            # sqlite + migrations + repos
    forge-loop          # loop runtime/queue/smart-stop
    forge-cli           # clap-based CLI
    forge-daemon        # gRPC server (tonic/prost)
    forge-runner        # runner process
    forge-tui           # loop TUI on frankentui
    fmail-core          # fmail model/store
    fmail-cli
    fmail-tui           # frankentui-based mail TUI
    forge-ftui-adapter  # insulation layer around frankentui
```

## Parity Contract (what must match)

### 1. CLI Contract

- Command names, args, flags, defaults, exit codes
- Human output shape + `--json`/`--jsonl` schemas
- Error semantics

Method:
- Snapshot `forge --help` and key subcommand help from Go
- Golden tests in Rust compare help/output contracts

### 2. DB Contract

- Migration order + resulting schema + constraints + indexes
- Record lifecycle semantics (loops, queue, runs, mem, work)

Method:
- Schema introspection tests against migrated DB
- CRUD behavior tests against seeded fixtures

### 3. Loop Runtime Contract

- Queue item semantics
- Smart stop behavior
- Profile/pool selection and cooldown behavior
- Logs + ledger side-effects

Method:
- Characterization tests run same scenario in Go and Rust, diff normalized outputs

### 4. Daemon/Runner Protocol Contract

- `forged` gRPC request/response compatibility
- Runner ownership/liveness semantics

Method:
- Proto-lock tests + interoperability tests (Rust client vs Go server, and inverse while dual-stack exists)

### 5. TUI Contract

- Critical workflows and keybindings:
  - Loop TUI tabs/actions
  - fmail/fmail-tui navigation + core actions
- Must preserve operational speed + no regressions in log readability

Method:
- Feature matrix checklist + scripted smoke harness
- Manual acceptance pass for complex interaction flows

## Legacy Removal Scope

### Remove Early (after freeze; before full port)

- Dead/hidden command paths not in target parity scope, if explicitly declared out-of-scope
- Old compatibility shims duplicated by loop-centric model

### Keep Until End

- Any code path touching persisted state, queue semantics, or protocol edges
- Anything needed to validate parity oracle

Rule:
- No deletion without one of:
  - parity replacement landed, or
  - explicit product cut decision documented

## Phase Plan + Exit Criteria

### Phase 0: Freeze + Characterize

Deliverables:
- Baseline command/output snapshots
- DB schema fingerprint tests
- Core loop scenario fixture set

Exit:
- We can detect behavior drift automatically

### Phase 1: Rust Core Headless (no TUI flip yet)

Deliverables:
- `forge-core`, `forge-db`, `forge-loop`, `forge-cli` for minimal loop commands (`init`, `up`, `ps`, `logs`, `msg`, `stop`, `kill`, `resume`, `run`)
- Dual-run verification harness

Exit:
- Rust passes parity for minimal loop lifecycle

### Phase 2: Daemon + Runner Parity

Deliverables:
- Rust `forge-daemon` + `forge-runner` implementing existing proto/behavior

Exit:
- gRPC compatibility tests green

### Phase 3: FrankenTUI Loop UI

Deliverables:
- Rust `forge-tui` on frankentui adapter
- Key workflow parity with current loop TUI

Exit:
- Manual + scripted TUI acceptance pass

### Phase 4: fmail + fmail-tui Port

Deliverables:
- Rust mail core/CLI/TUI parity

Exit:
- `fmail` command set + operator workflows parity

### Phase 5: Cutover + Legacy Deletion

Deliverables:
- Default binaries from Rust
- Go paths removed or archived
- CI/CD migrated to Rust toolchain + packaging

Exit:
- Release candidate accepted with parity checklist complete

## Immediate Week-1 Execution Plan

1. Add Rust workspace skeleton + CI job (`fmt`, `clippy`, `test`) without touching shipping Go CI.
2. Build characterization harness:
   - CLI help snapshots
   - JSON schema snapshots for key commands
   - DB schema fingerprint test
3. Implement first thin vertical slice in Rust:
   - `forge ps` against existing DB
   - match JSON output and exit behavior
4. Add `forge-ftui-adapter` crate and pin FrankenTUI commit.
5. Write explicit product cut list: what legacy commands are permanently dropped vs deferred.

## Key Risks

- FrankenTUI API churn (pre-1.0 + nightly): mitigate with adapter + pin.
- Hidden semantics in Go code/tests: mitigate with characterization first.
- Scope explosion from old surfaces: mitigate with explicit parity matrix and product cut list.
- Dual-runtime drift during long migration: mitigate with parity CI gates at every slice.

## Decisions Needed From Product/Owner

1. Canonical parity scope for v1 Rust:
   - `loop-only`, or
   - full current visible `forge --help` surface.
2. Legacy groups (`agent/workspace/node/accounts/vault/...`):
   - drop now, port later, or keep hidden but supported.
3. FrankenTUI risk posture:
   - accept nightly + pinned commit for production, or
   - keep fallback TUI backend until FrankenTUI stabilizes.
4. Cutover style:
   - one final switch, or staged command-by-command switch.

## Owner Decision Register (Scope Lock)

Decision snapshot date: 2026-02-09

| Decision area | Current proposal | Artifact | Owner sign-off |
|---|---|---|---|
| Non-legacy command parity | Port all non-legacy command surface | `docs/rust-port-manifest.md` | signed 2026-02-09 (owner directive; task comment) |
| Legacy command groups | Drop groups behind `addLegacyCommand(...)` | `docs/rust-legacy-drop-list.md` | signed 2026-02-09 (owner directive; task comment) |
| fmail/fmail-tui command surface | Port full current help-visible surface | `docs/rust-fmail-command-manifest.md` | signed 2026-02-09 (owner directive; task comment) |
| Parity gates and cutover style | Single final switch gated by parity evidence | `docs/adr/0005-rust-single-switch-policy.md` | signed 2026-02-09 (owner directive; task comment) |

Scope lock note:
- All visible command groups are now explicitly classified in docs as port/drop.
- Legacy carry-over is disallowed unless re-opened via explicit owner decision.
