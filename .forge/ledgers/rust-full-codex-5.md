---
loop_id: 36a5aa6e-db2e-45fc-9037-bddd190a0f74
loop_name: rust-full-codex-5
repo_path: /Users/trmd/Code/oss--forge/repos/forge
created_at: 2026-02-09T16:15:33Z
---

# Loop Ledger: rust-full-codex-5

## 2026-02-09T16:57:02Z

- run_id: e0826128-5d61-44a0-870e-74292d6f202d
- loop_name: rust-full-codex-5
- status: success
- profile: codex2
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/rust-swarm-dev-codex-continuous.md
- prompt_override: false
- started_at: 2026-02-09T16:15:33Z
- finished_at: 2026-02-09T16:57:02Z
- exit_code: 0

```
let frame = bootstrap_frame();
-        assert_eq!(
-            frame.snapshot(),
-            "Forge TUI           \nstatus: ready       "
+        assert_render_frame_snapshot(
+            "forge_tui_bootstrap_frame",
+            &frame,
+            "Forge TUI           \nstatus: ready       ",
         );
     }
 
     #[test]
+    #[ignore]
+    fn perf_bootstrap_frame_build() {
+        let result = forge_ftui_adapter::perf::measure(10_000, || {
+            let _ = bootstrap_frame();
+        });
+        assert!(result.total.as_nanos() > 0);
+    }
+
+    #[test]
     fn uses_adapter_widget_primitives_for_loop_tui() {
         let widgets = loop_dashboard_widgets();
         let snapshot = format!(
@@ -102,4 +122,23 @@
         assert_eq!(columns[1].title, "Status");
         assert_eq!(columns[3].width, 10);
     }
+
+    #[test]
+    fn uses_adapter_input_abstraction() {
+        assert_eq!(
+            map_input(InputEvent::Key(KeyEvent::plain(Key::Up))),
+            UiAction::MoveUp
+        );
+        assert_eq!(
+            map_input(InputEvent::Key(KeyEvent {
+                key: Key::Char('c'),
+                modifiers: Modifiers {
+                    shift: false,
+                    ctrl: true,
+                    alt: false,
+                },
+            })),
+            UiAction::Compose
+        );
+    }
 }

tokens used
496,672
Closed tasks (prj-vr0104gr):

- `forge-wmb`: pinned Rust coverage tool+format via regression (`internal/doccheck/rust_coverage_policy_test.go`), aligned w/ `docs/rust-coverage-policy.md` + `.github/workflows/ci.yml`. Validation: `env -u GOROOT -u GOTOOLDIR go test ./...`
- `forge-322`: loop lifecycle oracle fixtures + golden (`internal/cli/loop_oracle_lifecycle_fixtures_test.go`, `internal/cli/testdata/oracle/loop_lifecycle.json`) covering `up/ps/logs/msg/queue/stop/kill/resume/run/rm/clean/scale` + state snapshots. Validation: `env -u GOROOT -u GOTOOLDIR go test ./...`
- `forge-fxt`: FrankenTUI adapter snapshot+perf harness (`rust/crates/forge-ftui-adapter/src/snapshot.rs`, `rust/crates/forge-ftui-adapter/src/perf.rs`) + used by `forge-tui`/`fmail-tui` tests; ran `cargo fmt` (fixed existing fmt drift). Validation: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` (with isolated `CARGO_HOME`/`CARGO_TARGET_DIR`)
- `forge-pwe`: mem/work fixtures + golden (`internal/cli/loop_mem_work_oracle_test.go`, `internal/cli/testdata/oracle/mem_work.json`), side-effect free (unset `FORGE_LOOP_ID`, `FMAIL_AGENT`, silenced migration logs). Validation: `env -u GOROOT -u GOTOOLDIR go test ./...`
- `forge-h19`: operational help snapshots `docs/forge/help/forge-help-*.txt` + doccheck (`internal/doccheck/forge_operational_help_snapshots_test.go`) + JSON shape smoke for `status` (`internal/cli/operational_json_shape_test.go`). Validation: `env -u GOROOT -u GOTOOLDIR go test ./...`

Queue now: `sv task ready --project prj-vr0104gr` => none.
```

## 2026-02-09T17:49:43Z

- run_id: 815dacfb-afc1-4c72-9633-427dfc32798d
- loop_name: rust-full-codex-5
- status: success
- profile: codex2
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/rust-swarm-dev-codex-continuous.md
- prompt_override: false
- started_at: 2026-02-09T16:57:32Z
- finished_at: 2026-02-09T17:49:43Z
- exit_code: 0

```
+    }
+
+    fn index_exists(db_path: &Path, index: &str) -> bool {
+        object_exists(db_path, "index", index)
+    }
+
+    fn trigger_exists(db_path: &Path, trigger: &str) -> bool {
+        object_exists(db_path, "trigger", trigger)
+    }
+
+    fn column_exists(db_path: &Path, table: &str, column: &str) -> bool {
+        let conn = match Connection::open(db_path) {
+            Ok(conn) => conn,
+            Err(err) => panic!("open sqlite connection {}: {err}", db_path.display()),
+        };
+        let sql = format!("PRAGMA table_info({})", table);
+        let mut stmt = match conn.prepare(&sql) {
+            Ok(stmt) => stmt,
+            Err(err) => panic!("prepare table_info for {table}: {err}"),
+        };
+        let rows = match stmt.query_map([], |row| row.get::<_, String>(1)) {
+            Ok(rows) => rows,
+            Err(err) => panic!("query table_info for {table}: {err}"),
+        };
+        for row in rows {
+            let col_name = match row {
+                Ok(name) => name,
+                Err(err) => panic!("read column name: {err}"),
+            };
+            if col_name == column {
+                return true;
+            }
+        }
+        false
+    }
+
+    fn object_exists(db_path: &Path, object_type: &str, name: &str) -> bool {
+        let conn = match Connection::open(db_path) {
+            Ok(conn) => conn,
+            Err(err) => panic!("open sqlite connection {}: {err}", db_path.display()),
+        };
+        let exists: i64 = match conn.query_row(
+            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = ?1 AND name = ?2)",
+            params![object_type, name],
+            |row| row.get(0),
+        ) {
+            Ok(exists) => exists,
+            Err(err) => panic!("sqlite_master lookup ({object_type}/{name}): {err}"),
+        };
+        exists == 1
+    }
 }

tokens used
846,772
Queue snapshot `2026-02-09T17:49:03Z`: `sv task ready --project prj-vr0104gr` => `0` tasks.

`128` tasks `open`, but blocked behind active `in_progress` foundations (not stale), so no safe claim per policy.

Sent `fmail` updates to `task` + `@forge-orchestrator` (`20260209-174908-2685/2686`).
```

## 2026-02-09T17:54:18Z

- run_id: 226d86ba-8896-44ac-a92e-543301c1a757
- loop_name: rust-full-codex-5
- status: success
- profile: codex2
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/rust-swarm-dev-codex-continuous.md
- prompt_override: false
- started_at: 2026-02-09T17:50:13Z
- finished_at: 2026-02-09T17:54:18Z
- exit_code: 0

```
-              echo "- See parity-nightly.log artifact"
+              echo "- Checks: internal/parity suite (oracle/CLI/runtime/daemon/proto/fmail/schema)"
+              echo "- See parity-nightly.log and parity-diff artifacts"
             } >> "$GITHUB_STEP_SUMMARY"
           fi
+      - name: Generate parity diff artifacts on drift
+        if: steps.parity.outputs.exit_code != '0'
+        run: |
+          go run ./cmd/parity-artifacts \
+            --expected internal/parity/testdata/oracle/expected \
+            --actual internal/parity/testdata/oracle/actual \
+            --out parity-artifacts || true
       - uses: actions/upload-artifact@v4
         if: always()
         with:
           name: parity-nightly-log
           path: parity-nightly.log
           if-no-files-found: ignore
+      - uses: actions/upload-artifact@v4
+        if: steps.parity.outputs.exit_code != '0'
+        with:
+          name: parity-diff
+          path: parity-artifacts/
+          if-no-files-found: ignore
       - name: Fail on drift
         if: steps.parity.outputs.exit_code != '0'
         run: |
           echo "nightly parity drift detected"
           exit 1
-
-  rust-coverage-nightly:
-    runs-on: ubuntu-latest
-    steps:
-      - uses: actions/checkout@v4
-      - uses: dtolnay/rust-toolchain@stable
-        with:
-          components: llvm-tools-preview
-      - uses: taiki-e/install-action@cargo-llvm-cov
-      - name: Enforce per-crate thresholds
-        run: scripts/rust-coverage-gate.sh
-      - name: Generate nightly rust coverage
-        working-directory: rust
-        run: cargo llvm-cov --workspace --all-features --lcov --output-path coverage/nightly-lcov.info
-      - name: Summarize nightly rust coverage
-        working-directory: rust
-        run: |
-          {
-            echo "## Nightly Rust Coverage"
-            echo ""
-            cargo llvm-cov report --summary-only
-          } >> "$GITHUB_STEP_SUMMARY"
-      - uses: actions/upload-artifact@v4
-        with:
-          name: rust-coverage-nightly
-          path: rust/coverage/nightly-lcov.info
-          if-no-files-found: error

tokens used
112,750
`forge-qgw` scope: add CI alert routing on parity drift (create/update GH issue + owner mention). scanning existing workflows/alert plumbing.
```

## 2026-02-09T17:59:21Z

- run_id: 76d96673-86ae-4b70-874e-a21dcd3067d0
- loop_name: rust-full-codex-5
- status: success
- profile: codex2
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/rust-swarm-dev-codex-continuous.md
- prompt_override: false
- started_at: 2026-02-09T17:54:48Z
- finished_at: 2026-02-09T17:59:21Z
- exit_code: 0

```
+- Map to gate doc:
+  - CLI: `docs/rust-cli-gate.md`
+  - DB/schema: `docs/rust-db-gate.md`
+  - Runtime (queue/smart-stop/ledger/logging): `docs/rust-runtime-gate.md`
+  - Daemon/proto: `docs/rust-daemon-proto-gate.md`
+  - fmail CLI/TUI: `docs/rust-fmail-gate.md`, `docs/rust-fmail-tui-checklist.md`
+  - Loop TUI: `docs/rust-loop-tui-gate.md`, `docs/rust-loop-tui-checklist.md`
+
+## Artifacts
+
+- If `parity` fails: GitHub Actions artifact `parity-diff` (generated via `cmd/parity-artifacts`).
+- Nightly always uploads `parity-nightly-log`; drift uploads `parity-diff`.
+- Baseline snapshot bundle: CI artifact `rust-baseline-snapshot` (job `baseline-snapshot`).
+
+## Reproduce locally
+
+Note: if your shell exports `GOROOT`/`GOTOOLDIR` (mise vs Homebrew), prefer `env -u GOROOT -u GOTOOLDIR ...`.
+
+```bash
+env -u GOROOT -u GOTOOLDIR go test ./internal/parity -count=1
+
+# Isolate a single gate by name (examples)
+env -u GOROOT -u GOTOOLDIR go test ./internal/parity -run '^TestCLIGateRootOracleBaseline$' -count=1
+env -u GOROOT -u GOTOOLDIR go test ./internal/parity -run '^TestSchemaFingerprintBaseline$' -count=1
+env -u GOROOT -u GOTOOLDIR go test ./internal/parity -run '^TestRuntimeGateLoopQueueSmartStopLedger$' -count=1
+```
+
+Generate local diff artifacts for the oracle tree:
+
+```bash
+env -u GOROOT -u GOTOOLDIR go run ./cmd/parity-artifacts \
+  --expected internal/parity/testdata/oracle/expected \
+  --actual internal/parity/testdata/oracle/actual \
+  --out parity-artifacts
+```
+
+Baseline snapshot drift check:
+
+```bash
+scripts/rust-baseline-snapshot.sh build/rust-baseline/latest --check
+```
+
+## Intentional drift
+
+- Drift is never “silent”: update the relevant gate docs + baseline artifacts in the same PR.
+- Baseline inventory + drift policy: `docs/rust-baseline-inventory-v2026-02-09.md`.
+

tokens used
101,623
forge-x0f closed.

Changes
- add triage doc: `docs/parity-regression-playbook.md` (CI surface map, artifacts, local repro; note `env -u GOROOT -u GOTOOLDIR` workaround)

Validation
- `env -u GOROOT -u GOTOOLDIR go test ./internal/parity -count=1` PASS

Queue
- `sv task ready --project prj-vr0104gr`: 0 (next tasks `forge-4ss`, `forge-7sd` now `in_progress` by others).
```

