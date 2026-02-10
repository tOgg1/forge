# Parity Regression Playbook

Goal: detect drift fast, localize surface, fix or refresh baseline intentionally.

## Where parity runs

- Pre-merge + post-merge: `.github/workflows/ci.yml` job `parity` (runs on PRs + `main` pushes).
- Nightly: `.github/workflows/parity-nightly.yml` (fails on drift; uploads log + diff artifact).

## First triage (CI)

- Identify failing test name in `parity` job.
- Map to gate doc:
  - CLI: `docs/rust-cli-gate.md`
  - DB/schema: `docs/rust-db-gate.md`
  - Runtime (queue/smart-stop/ledger/logging): `docs/rust-runtime-gate.md`
  - Daemon/proto: `docs/rust-daemon-proto-gate.md`
  - fmail CLI/TUI: `docs/rust-fmail-gate.md`, `docs/rust-fmail-tui-checklist.md`
  - Loop TUI: `docs/rust-loop-tui-gate.md`, `docs/rust-loop-tui-checklist.md`

## Artifacts

- If `parity` fails: GitHub Actions artifact `parity-diff` (generated via `cmd/parity-artifacts`).
- Drift artifact includes:
  - `normalized/report.json`: raw mismatch/missing/unexpected lists.
  - `normalized/drift-report.json`: machine triage schema (`parity.drift.v1`).
  - `normalized/drift-triage.md`: triage queue template (priority/type/path/owner/root cause/action/tracking).
  - `normalized/parity-alert-routing.json`: owner route summary (`parity.alert-routing.v1`).
  - `normalized/parity-alert-routing.md`: CI-ready owner notification summary appended to step summary on drift.
- Nightly always uploads `parity-nightly-log`; drift uploads `parity-diff`.
- Baseline snapshot bundle: CI artifact `rust-baseline-snapshot` (job `baseline-snapshot`).

## Triage format

- Fill every row in `normalized/drift-triage.md` before closing incident:
  - `Owner`: prefilled from parity path ownership mapping; reassign if ownership changed.
  - `Root cause`: concise cause statement.
  - `Action`: concrete fix or baseline-refresh action.
  - `Tracking issue`: issue/PR/task link.
- Keep one row per drift path.

## Reproduce locally

Note: if your shell exports `GOROOT`/`GOTOOLDIR` (mise vs Homebrew), prefer `env -u GOROOT -u GOTOOLDIR ...`.

```bash
env -u GOROOT -u GOTOOLDIR go test ./internal/parity -count=1

# Isolate a single gate by name (examples)
env -u GOROOT -u GOTOOLDIR go test ./internal/parity -run '^TestCLIGateRootOracleBaseline$' -count=1
env -u GOROOT -u GOTOOLDIR go test ./internal/parity -run '^TestSchemaFingerprintBaseline$' -count=1
env -u GOROOT -u GOTOOLDIR go test ./internal/parity -run '^TestRuntimeGateLoopQueueSmartStopLedger$' -count=1
```

Generate local diff artifacts for the oracle tree:

```bash
env -u GOROOT -u GOTOOLDIR go run ./cmd/parity-artifacts \
  --expected internal/parity/testdata/oracle/expected \
  --actual internal/parity/testdata/oracle/actual \
  --out parity-artifacts
```

Baseline snapshot drift check:

```bash
scripts/rust-baseline-snapshot.sh build/rust-baseline/latest --check
```

Loop lifecycle side-by-side harness (Go vs Rust binaries):

```bash
go build -o /tmp/forge-go ./cmd/forge
(cd rust && cargo build -p forge-cli --bin rforge)

go run ./cmd/parity-loop-lifecycle \
  --scenario internal/parity/testdata/lifecycle_harness/scenario.json \
  --fixture . \
  --go-bin /tmp/forge-go \
  --rust-bin ./rust/target/debug/rforge \
  --out build/parity-loop-lifecycle-report.json

# Scenario comparator script (stdout/stderr/exit + DB side effects):
scripts/parity-scenario-compare.sh \
  --scenario internal/parity/testdata/lifecycle_harness/scenario.json \
  --fixture . \
  --go-bin /tmp/forge-go \
  --rust-bin ./rust/target/debug/rforge \
  --out-dir build/parity-scenario/latest
```

## Intentional drift

- Drift is never “silent”: update the relevant gate docs + baseline artifacts in the same PR.
- Baseline inventory + drift policy: `docs/rust-baseline-inventory-v2026-02-09.md`.
