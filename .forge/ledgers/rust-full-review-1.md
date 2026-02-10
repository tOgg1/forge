---
loop_id: 6edf00b8-50da-4bba-9d62-fa25c970051a
loop_name: rust-full-review-1
repo_path: /Users/trmd/Code/oss--forge/repos/forge
created_at: 2026-02-09T16:15:33Z
---

# Loop Ledger: rust-full-review-1

## 2026-02-09T16:18:59Z

- run_id: 2509b9d7-73dc-4b79-b7ae-402eff842134
- loop_name: rust-full-review-1
- status: success
- profile: cc3
- harness: claude
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/rust-swarm-review.md
- prompt_override: false
- started_at: 2026-02-09T16:15:33Z
- finished_at: 2026-02-09T16:18:59Z
- exit_code: 0

```
## Review Complete: `forge-3sw` (Package Include Matrix)

### Findings Summary

| # | Severity | Finding | Location | Fix |
|---|----------|---------|----------|-----|
| 1 | **P0** | `internal/adapters` missing from include matrix | Runtime dep via `internal/agent/service.go:15`, `internal/state/engine.go:12` | Add to include table → `forge-runner` or `forge-core` |
| 2 | **P0** | `internal/vault` missing from include matrix | Runtime dep via `internal/account/service.go:21` — credential resolution for agent spawn | Add to include table → `forge-core`, note "CLI drop, package include" |
| 3 | **P1** | `internal/beads` missing from include matrix | Runtime dep via `internal/workspace/service.go:13` | Add to include table → `forge-core` |
| 4 | **P1** | `internal/cli` missing from include matrix | CLI wiring package for `cmd/forge/main.go:11` | Add to include table → `forge-cli` |
| 5 | **P2** | Reachability drift: `internal/account/caam` and `internal/recipes` appear in `go list -deps` despite being correctly classified as drop | Compile-time import from legacy CLI files | Document known false-positive in reachability section |
| 6 | **P2** | False completeness claim on line 53 | `docs/rust-package-include-matrix.md:53` | Fix claim after adding missing packages |
| 7 | **P3** | Go tests blocked by toolchain mismatch (go1.25.7 vs go1.25.6) | Environment | Not a code issue |

### Validation Method
- Enumerated all `internal/` directories
- Ran `go list -deps` across all 5 binaries (`forge`, `forged`, `forge-agent-runner`, `fmail`, `fmail-tui`)
- Cross-referenced reachability output against both the include and non-include tables
- Traced import chains to confirm runtime vs compile-time-only dependencies

### Verdict: **Issues** — 4 runtime packages must be added before acceptance criteria ("No included runtime package is left unmapped") are met.
```

## 2026-02-09T16:25:36Z

- run_id: 664edc5b-cc62-420c-9888-d98d991a6181
- loop_name: rust-full-review-1
- status: success
- profile: cc3
- harness: claude
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/rust-swarm-review.md
- prompt_override: false
- started_at: 2026-02-09T16:20:59Z
- finished_at: 2026-02-09T16:25:36Z
- exit_code: 0

```
## Review Cycle Complete

### Tasks Reviewed

| Task | Area | Verdict |
|------|------|---------|
| **forge-kh6** | forge-core crate bootstrap | **ISSUES** (5 findings) |
| **forge-wf5** | Oracle harness + comparator | **PASS** (1 minor note) |
| **forge-8a8** | Schema fingerprint baseline | **PASS** |
| **forge-fxv** | Root/global flags snapshot | **PASS** |

### Test Results
- `go test ./internal/parity/...` — **10/10 PASS**
- `go test ...doccheck -run TestForgeRootSnapshotsCurrent` — **PASS**
- `cargo test -p forge-core -p fmail-core -p forge-parity-stub` — **17/17 PASS**
- `cargo check --all-targets` — **FAIL** (forge-tui missing bin file)

### Findings Summary

**P0-BLOCKER** (1):
1. `forge-tui` workspace build broken — `Cargo.toml` declares `[[bin]] path="src/bin/forge-tui.rs"` but file doesn't exist. `cargo check --all-targets` fails.

**P1-HIGH** (2):
2. `EventType` enum has ~18 parity gaps vs Go `internal/models/event.go` — many variants missing or renamed (e.g. `approval.granted` vs Go's `approval.approved`, `rate_limit.hit` vs `rate_limit.detected`).
3. `LoopQueueItemStatus` missing `Skipped` variant and uses `Processing` where Go uses `Dispatched`.

**P2-MEDIUM** (2):
4. `QueueItemStatus` enum completely missing from Rust (Go has 5 variants).
5. `fmail-core/Cargo.toml` uses hardcoded edition/license/publish instead of workspace references.

**P3-LOW** (1):
6. Comparator `reUnixHomePath` regex only normalizes `/Users/` (macOS), not `/home/` (Linux). CI runs on Linux.

All findings sent via fmail to task channel and orchestrator.
```

## 2026-02-09T16:32:01Z

- run_id: dd6f8b0c-a917-4a44-aa29-e62113e07f3b
- loop_name: rust-full-review-1
- status: success
- profile: cc3
- harness: claude
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/rust-swarm-review.md
- prompt_override: false
- started_at: 2026-02-09T16:27:36Z
- finished_at: 2026-02-09T16:32:01Z
- exit_code: 0

```
## Review Complete — Bootstrap Tasks (forge-jrx, forge-q2g, forge-srd, forge-gq8)

### Fix Applied
- **`rust/crates/forge-cli/Cargo.toml`** — Added missing `[lints] workspace = true`. Without this, workspace lint policy (forbid unsafe_code, deny unwrap_used/dbg_macro/expect_used/todo) was silently unenforced for the entire forge-cli crate.

### Findings Summary

| # | Severity | Task | Finding | File |
|---|----------|------|---------|------|
| 1 | **HIGH** (FIXED) | forge-q2g | `forge-cli` missing `[lints] workspace = true` | `rust/crates/forge-cli/Cargo.toml` |
| 2 | **HIGH** | forge-jrx | EventType enum has 10+ naming/semantic mismatches vs Go (`node.online` vs `NodeCreated`, `approval.approved` vs `ApprovalGranted`, missing `approval.denied`, `message.completed`, `message.failed`, etc.) | `rust/crates/forge-core/src/event.rs:9` |
| 3 | **HIGH** | forge-jrx | `LoopQueueItemStatus` uses `Processing` where Go has `Dispatched`; missing `Skipped` variant (4 vs 5 variants) | `rust/crates/forge-core/src/queue.rs:33` |
| 4 | **MEDIUM** | forge-jrx | Doccheck test only validates 5/12 crates for workspace keys and does NOT check `[lints] workspace = true` at all | `internal/doccheck/rust_workspace_lint_policy_test.go:72` |
| 5 | **MEDIUM** | forge-jrx | Config struct ~2% complete (only `DatabaseConfig` + `LoggingConfig` vs 11+ sections in Go) | `rust/crates/forge-core/src/config.rs:8` |
| 6 | **LOW** | forge-jrx | 12 enums missing `Display` impl — no string serialization parity testable | `rust/crates/forge-core/src/models.rs` |

### Validation
- `cargo check --workspace`: clean
- `cargo test --workspace`: 28 tests, all pass
- `cargo clippy --workspace --all-targets`: clean
- `cargo fmt --all -- --check`: clean
```

## 2026-02-09T17:47:11Z

- run_id: 1f5608ab-c4af-482d-84fa-a4321f86dc26
- loop_name: rust-full-review-1
- status: success
- profile: codex3
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/rust-swarm-review.md
- prompt_override: false
- started_at: 2026-02-09T17:43:24Z
- finished_at: 2026-02-09T17:47:11Z
- exit_code: 0

```
+   - Evidence:
+     - Unknown command fallback: `rust/crates/forge-cli/src/lib.rs:188`
+     - Probe: `cargo run -q -p forge-cli -- --definitely-not-a-real-flag` => `unknown forge command: --definitely-not-a-real-flag` + full help.
+   - Parity risk: Go snapshot expects `unknown flag: --definitely-not-a-real-flag` (`docs/forge/help/forge-root-invalid-flag.stderr.txt:1`).
+   - Fix hint: root-level detection for unrecognized `-`/`--` tokens, classify as invalid flag (`ERR_INVALID_FLAG` in JSON modes).
+
+4. **Medium**: no-args behavior drift.
+   - Evidence:
+     - Rust: empty command branch renders help: `rust/crates/forge-cli/src/lib.rs:68`
+     - Go: default root action runs TUI: `internal/cli/root.go:53`
+   - Parity risk: root UX contract mismatch (help claims no-args launches TUI).
+   - Fix hint: map no-args root path to TUI/preflight entrypoint.
+
+5. **Medium**: regression test gaps for root parity.
+   - Evidence:
+     - Existing root tests focus on help/version/unknown-command only: `rust/crates/forge-cli/tests/root_command_test.rs:6`
+     - Missing cases: `--robot-help`, invalid-root-flag shape, `--config` invalid path failure, `--chdir` invalid path failure, no-args TUI default behavior.
+   - Fix hint: add root parity tests + golden fixtures for error surface.
+
+## Validation Notes
+
+- Ran: `cargo test -p forge-cli --test root_command_test` (pass).
+- Ran: `cargo test -p forge-cli --lib` (pass).
+- Attempted Go parity tests: `go test ./internal/cli -run 'TestRoot|TestRobotHelp|TestWatchRequiresJSONL|TestForgeRootSnapshotsCurrent'` (blocked by local Go toolchain mismatch: stdlib/object version `go1.25.7` vs tool `go1.25.6`).

tokens used
72,960
1. High: `--config` / `--chdir` parsed but never applied.  
`rust/crates/forge-cli/src/error_envelope.rs:61`, `rust/crates/forge-cli/src/error_envelope.rs:85`, `rust/crates/forge-cli/src/lib.rs:57`  
Evidence:  
- `cargo run -q -p forge-cli -- --config /definitely/not/real.yaml ps` => exit `0`, `No loops found`  
- `cargo run -q -p forge-cli -- --chdir /definitely/not/a/dir ps` => exit `0`, `No loops found`  
Parity baseline applies/fails fast in Go root init: `internal/cli/root.go:106`, `internal/cli/root.go:119`.

2. High: `--robot-help` dead flag (parsed, no behavior).  
`rust/crates/forge-cli/src/error_envelope.rs:59`, `rust/crates/forge-cli/src/lib.rs:67`  
Evidence: `cargo run -q -p forge-cli -- --robot-help` prints normal help, not robot-help payload.  
Go behavior exists: `internal/cli/root.go:72`, `internal/cli/robot_help_test.go:10`.

3. Medium: unknown root flags misclassified as unknown command + full help.  
`rust/crates/forge-cli/src/lib.rs:188`  
Evidence: `cargo run -q -p forge-cli -- --definitely-not-a-real-flag` => `unknown forge command: ...` + help text.  
Expected snapshot: `docs/forge/help/forge-root-invalid-flag.stderr.txt:1` (`unknown flag: --definitely-not-a-real-flag`).

4. Medium: no-args behavior drift.  
`rust/crates/forge-cli/src/lib.rs:68`  
Rust prints help; Go default action is TUI path: `internal/cli/root.go:53`.

5. Medium: missing regression tests for above root semantics.  
`rust/crates/forge-cli/tests/root_command_test.rs:6`  
Gaps: robot-help, invalid root flag shape, config/chdir invalid-path failures, no-args TUI/default path.

Summary:
- Reviewed task: `forge-5dc`.
- Sent findings via `fmail` to `task` and summary to `@forge-orchestrator`.
- Added review note: `docs/review/2026-02-09-forge-5dc-root-cli-review.md`.
- Validation run:
  - `cargo test -p forge-cli --test root_command_test` passed.
  - `cargo test -p forge-cli --lib` passed.
  - Go parity test attempt blocked by local toolchain mismatch (`go1.25.7` stdlib vs `go1.25.6` tool).
```

## 2026-02-09T17:53:10Z

- run_id: f367a7a6-b4da-4e86-8b90-93e203e76e8a
- loop_name: rust-full-review-1
- status: success
- profile: codex3
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/rust-swarm-review.md
- prompt_override: false
- started_at: 2026-02-09T17:49:11Z
- finished_at: 2026-02-09T17:53:10Z
- exit_code: 0

```
+ return Err(format!("unsupported queue item type \"{value}\""));
- assert_eq!(err, "unsupported queue item type 'unknown'");
+ assert_eq!(err, "unsupported queue item type \"unknown\"");
```

**Summary**
1. Reviewed task: `forge-xwm` (memory/work injection composition scope).  
2. Validation run:
   - `cargo test -p forge-loop -p forge-db` passed.
   - `cargo test -p forge-loop` passed after fix (87 unit + 3 integration).
   - `GOTOOLCHAIN=go1.25.7 go test ./internal/loop ./internal/db` passed.
3. Reported via fmail:
   - initial issue + follow-up fix to `task`
   - summary to `@forge-orchestrator` (`issues` then `pass (fixed locally)`).
file update:
diff --git a/rust/crates/forge-loop/src/queue_interactions.rs b/rust/crates/forge-loop/src/queue_interactions.rs
index 794967558d6141320abf25546a9e8bf8ddaabd12..ad17f0bc20aec57ecb820d0776d20f0580d35204
--- a/rust/crates/forge-loop/src/queue_interactions.rs
+++ b/rust/crates/forge-loop/src/queue_interactions.rs
@@ -52,7 +52,7 @@
                 return Ok(plan);
             }
             QueueControlItem::Unsupported(value) => {
-                return Err(format!("unsupported queue item type '{value}'"));
+                return Err(format!("unsupported queue item type \"{value}\""));
             }
         }
     }
@@ -217,6 +217,6 @@
             Ok(_) => panic!("expected error"),
             Err(err) => err,
         };
-        assert_eq!(err, "unsupported queue item type 'unknown'");
+        assert_eq!(err, "unsupported queue item type \"unknown\"");
     }
 }

tokens used
98,094
**Findings**
1. `medium` parity miss found and fixed: unsupported queue-item error used single quotes, Go uses `%q` (double quotes).  
   - Fixed in `rust/crates/forge-loop/src/queue_interactions.rs:55`  
   - Test updated in `rust/crates/forge-loop/src/queue_interactions.rs:220`  
   - Diff:
```diff
- return Err(format!("unsupported queue item type '{value}'"));
+ return Err(format!("unsupported queue item type \"{value}\""));
- assert_eq!(err, "unsupported queue item type 'unknown'");
+ assert_eq!(err, "unsupported queue item type \"unknown\"");
```

**Summary**
1. Reviewed task: `forge-xwm` (memory/work injection composition scope).  
2. Validation run:
   - `cargo test -p forge-loop -p forge-db` passed.
   - `cargo test -p forge-loop` passed after fix (87 unit + 3 integration).
   - `GOTOOLCHAIN=go1.25.7 go test ./internal/loop ./internal/db` passed.
3. Reported via fmail:
   - initial issue + follow-up fix to `task`
   - summary to `@forge-orchestrator` (`issues` then `pass (fixed locally)`).
```

## 2026-02-09T17:58:35Z

- run_id: f99c04eb-910d-4a24-969f-a6eaffe0bcaf
- loop_name: rust-full-review-1
- status: success
- profile: codex3
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/rust-swarm-review.md
- prompt_override: false
- started_at: 2026-02-09T17:55:10Z
- finished_at: 2026-02-09T17:58:35Z
- exit_code: 0

```
-    fs::write(ledger_path, content).map_err(|err| err.to_string())
+    let mut options = OpenOptions::new();
+    options.create(true).write(true).truncate(true);
+    #[cfg(unix)]
+    {
+        use std::os::unix::fs::OpenOptionsExt;
+        options.mode(0o644);
+    }
+    let mut file = options.open(ledger_path).map_err(|err| err.to_string())?;
+    file.write_all(content.as_bytes())
+        .map_err(|err| err.to_string())
 }
 
 pub fn append_ledger_entry(
@@ -110,9 +119,14 @@
         return Ok(());
     }
 
-    let mut file = OpenOptions::new()
-        .create(true)
-        .append(true)
+    let mut options = OpenOptions::new();
+    options.create(true).append(true);
+    #[cfg(unix)]
+    {
+        use std::os::unix::fs::OpenOptionsExt;
+        options.mode(0o644);
+    }
+    let mut file = options
         .open(&loop_record.ledger_path)
         .map_err(|err| err.to_string())?;
 

tokens used
88,802
Hi. One parity bug caught and fixed; ledger writer now matches Go file-mode behavior.

**Findings**
1. `Medium` (fixed): ledger file create mode parity drift on Unix with `umask 0002`.
`rust/crates/forge-loop/src/ledger_writer.rs:81`
`rust/crates/forge-loop/src/ledger_writer.rs:122`
Fix: explicit `0o644` on create paths via `OpenOptionsExt::mode(0o644)` (create/truncate and create/append).

**Exact Diff Applied**
1. Replaced `fs::write(...)` create path with `OpenOptions` + `write_all(...)` + Unix `mode(0o644)`.
2. Replaced append open chain with `OpenOptions` builder + Unix `mode(0o644)` before `.open(...)`.

**Validation**
1. `cargo test -p forge-loop ledger_writer --manifest-path rust/Cargo.toml` passed (`5` ledger-related tests including scenario).
2. `go test ./internal/loop/...` blocked by environment mismatch: stdlib compiled with `go1.25.7`, tool is `go1.25.6`.

**Residual Risk**
1. Full Go parity validation pending toolchain alignment.

**Reporting Sent**
1. `fmail send task` finding/fix + residual-risk note.
2. `fmail send @forge-orchestrator "review summary: forge-mjb pass"`.

**Review Doc**
1. `docs/review/2026-02-09-forge-mjb-ledger-writer-review.md`
```

## 2026-02-09T18:04:18Z

- run_id: dfcb60ee-7b81-4c57-9dba-c7fa0a5f08d0
- loop_name: rust-full-review-1
- status: success
- profile: codex3
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/rust-swarm-review.md
- prompt_override: false
- started_at: 2026-02-09T18:00:35Z
- finished_at: 2026-02-09T18:04:18Z
- exit_code: 0

```
+++ b/internal/paritydash/dashboard_test.go
@@ -39,6 +39,27 @@
 	}
 }
 
+func TestBuildFailsWhenOutcomeUnknown(t *testing.T) {
+	now := time.Date(2026, 2, 9, 12, 0, 0, 0, time.UTC)
+
+	d, err := Build(Input{
+		Checks: []InputCheck{
+			{ID: "oracle", Name: "Oracle", Outcome: "success"},
+			{ID: "schema", Name: "Schema", Outcome: ""},
+		},
+	}, now)
+	if err != nil {
+		t.Fatalf("build: %v", err)
+	}
+
+	if d.Summary.Unknown != 1 {
+		t.Fatalf("unknown summary count: %+v", d.Summary)
+	}
+	if d.Summary.Status != "fail" {
+		t.Fatalf("status: %q", d.Summary.Status)
+	}
+}
+
 func TestWriteFilesWritesJSONAndMarkdown(t *testing.T) {
 	now := time.Date(2026, 2, 9, 12, 0, 0, 0, time.UTC)
 	d, err := Build(Input{
@@ -72,4 +93,3 @@
 		t.Fatalf("stat md: %v", err)
 	}
 }
-

tokens used
110,477
**Findings**
1. Medium (fixed): parity dashboard was fail-open for unknown outcomes.  
`internal/paritydash/dashboard.go:113` now fails summary when `Unknown > 0` (not only `Failed > 0`).  
Fix landed + regression test added at `internal/paritydash/dashboard_test.go:42`.

**Summary**
- Reviewed task: `forge-4ss`.
- Minimal fix applied:
  - `internal/paritydash/dashboard.go`
  - `internal/paritydash/dashboard_test.go`
- Review note written: `docs/review/2026-02-09-forge-4ss-parity-drift-review.md`

**Validation**
- `env -u GOROOT -u GOTOOLDIR go test ./internal/paritydash -count=1` ✅
- `env -u GOROOT -u GOTOOLDIR go test ./cmd/parity-dashboard -count=1` ✅
- `env -u GOROOT -u GOTOOLDIR go test ./internal/parity -run '^(TestWriteDiffArtifactsSchema|TestBaselineRefreshScriptDryRunPass|TestBaselineRefreshScriptDryRunFailAndAllowDrift|TestBaselineRefreshScriptRejectsInvalidApproval)$' -count=1` ✅
- Full `internal/parity` suite still has pre-existing red: `TestProtoWireGateCriticalRPCFixtures` (fixture drift), not introduced by this patch.

**Loop reporting**
- Sent task messages + orchestrator summary via `fmail`:
  - issue/fix report
  - residual-risk note
  - `review summary: forge-4ss pass (1 medium issue fixed, residual risk noted)`
```

## 2026-02-09T18:09:35Z

- run_id: 2aa7b0bf-ec19-459b-ae42-5e7cd7e014ff
- loop_name: rust-full-review-1
- status: success
- profile: codex3
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/rust-swarm-review.md
- prompt_override: false
- started_at: 2026-02-09T18:06:18Z
- finished_at: 2026-02-09T18:09:35Z
- exit_code: 0

```
- Reported via fmail: task messages `20260209-180902-2901`, `20260209-180902-2902`; orchestrator summary `20260209-180902-2903`.
- Review doc written: `docs/review/2026-02-09-forge-4ss-review-loop.md`.
- Validation run:
1. `cargo test -p forge-loop` passed.
2. `cargo test -p forge-db loop_run_repository` passed.
3. `go test ./internal/loop ./internal/db` blocked by local Go tool mismatch (`go1.25.7` stdlib vs `go1.25.6` tool).
file update:
diff --git a/docs/review/2026-02-09-forge-4ss-review-loop.md b/docs/review/2026-02-09-forge-4ss-review-loop.md
new file mode 100644
index 0000000000000000000000000000000000000000..29368a5994bf62af9a37d37337e99be97efbfc61
--- /dev/null
+++ b/docs/review/2026-02-09-forge-4ss-review-loop.md
@@ -0,0 +1,27 @@
+# Review: `forge-4ss` (iteration result persistence)
+
+## Findings
+
+1. **High**: interrupt status parity miss (`killed` missing in Rust helper)
+   - Rust `iteration_result` status enum only has `Running|Success|Error` and `status_from_error` only maps `None -> Success`, `Some -> Error` (`rust/crates/forge-loop/src/iteration_result.rs:2`, `rust/crates/forge-loop/src/iteration_result.rs:16`).
+   - Go runner explicitly persists `killed` on interrupt (`internal/loop/runner.go:519`).
+   - Risk: interrupted runs can be persisted with wrong status once this helper is wired into persistence path.
+   - Fix hint: add `Killed` variant + interrupt-aware mapping API (or pass explicit final status), plus regression test for interrupt -> `killed`.
+
+2. **Medium**: unknown DB status coerced to `running`
+   - Rust scan path defaults parse failures to `Running` (`rust/crates/forge-db/src/loop_run_repository.rs:266`).
+   - Go scan path does not coerce to running; it preserves raw DB status text (`internal/db/loop_run_repository.go:218`).
+   - Risk: malformed/corrupt status rows get silently misreported as active `running`.
+   - Fix hint: remove `unwrap_or_default` coercion; preserve unknown status or surface explicit validation error. Add malformed-status test.
+
+## Validation
+
+- `cargo test -p forge-loop` ✅ (127 tests incl scenario tests)
+- `cargo test -p forge-db loop_run_repository` ✅
+- `go test ./internal/loop ./internal/db` ❌ blocked by local Go toolchain mismatch:
+  - compile errors show `go1.25.7` stdlib vs `go1.25.6` tool.
+
+## Summary
+
+- Result: **issues found** (2).
+- Residual risk: Go parity validation for touched oracle packages is currently blocked until local Go version mismatch is resolved.

tokens used
133,705
hi. momentum: bugs caught before merge.

1. **High** parity miss: interrupted run status (`killed`) not representable in Rust helper.  
`rust/crates/forge-loop/src/iteration_result.rs:2` and `rust/crates/forge-loop/src/iteration_result.rs:16` only model/map `success|error`; Go sets `killed` on interrupt at `internal/loop/runner.go:519`.  
Fix hint: add `Killed` + interrupt-aware status mapping; add regression test for interrupt -> `killed`.

2. **Medium** status coercion bug: malformed DB status becomes `running`.  
`rust/crates/forge-db/src/loop_run_repository.rs:266` uses `unwrap_or_default()` (defaults to `Running`), while Go preserves raw status text at `internal/db/loop_run_repository.go:218`.  
Fix hint: remove default-to-running coercion; preserve unknown status or return explicit validation error; add malformed-status test.

Summary:
- Reported via fmail: task messages `20260209-180902-2901`, `20260209-180902-2902`; orchestrator summary `20260209-180902-2903`.
- Review doc written: `docs/review/2026-02-09-forge-4ss-review-loop.md`.
- Validation run:
1. `cargo test -p forge-loop` passed.
2. `cargo test -p forge-db loop_run_repository` passed.
3. `go test ./internal/loop ./internal/db` blocked by local Go tool mismatch (`go1.25.7` stdlib vs `go1.25.6` tool).
```

## 2026-02-09T18:16:17Z

- run_id: 8e9db6cc-f315-40d1-9ccc-c4dd37ac2e92
- loop_name: rust-full-review-1
- status: success
- profile: codex3
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/rust-swarm-review.md
- prompt_override: false
- started_at: 2026-02-09T18:11:35Z
- finished_at: 2026-02-09T18:16:17Z
- exit_code: 0

```
+## Iteration: `forge-qe5` (repository parity pass)
+
+### Findings
+
+1. **High**: non-atomic port allocation path can race under contention
+   - Rust `allocate` performs `find_available_port` and `INSERT` outside a transaction (`rust/crates/forge-db/src/port_repository.rs:134`).
+   - Go path does both in one DB transaction (`internal/db/port_repository.go:70`).
+   - Risk: duplicate-port race/failed allocation bursts under concurrent allocators.
+   - Fix hint: run select+insert in same transaction (or retry wrapper) to preserve parity semantics.
+
+2. **High**: `set_current` unique-conflict fallback swallows write failures
+   - On unique conflict, fallback `tx.execute(...)` result is discarded and function returns `Ok(())` (`rust/crates/forge-db/src/loop_work_state_repository.rs:204`).
+   - Risk: false-success write path; caller sees success even when fallback update fails.
+   - Fix hint: check fallback update result and return `DbError::Transaction` on failure.
+
+3. **Medium**: unknown loop-run status coerced to `running`
+   - Scanner uses `parse(...).unwrap_or_default()` (`rust/crates/forge-db/src/loop_run_repository.rs:266`), defaulting unknown status to `Running`.
+   - Go scanner preserves raw status text (`internal/db/loop_run_repository.go:218`).
+   - Risk: malformed rows can be misreported as active/running.
+   - Fix hint: preserve unknown status explicitly or return validation error; add malformed-status regression test.
+
+### Validation
+
+- `cargo test -p forge-db` run reached `file_lock_repository_test` failure unrelated to reviewed files; earlier reviewed repository tests in that run passed before failure.
+- Targeted rerun blocked by concurrent workspace churn introducing `event_repository` compile error:
+  - `rust/crates/forge-db/src/event_repository.rs:246` (`Option<String>::flatten()`).
+- `go test ./internal/db/...` blocked by local toolchain mismatch:
+  - stdlib compiled with `go1.25.7`, tool is `go1.25.6`.
+
+### Summary
+
+- Result: **issues found** (3).

tokens used
133,963
**Findings**
1. High: non-atomic port allocation parity miss. `allocate` does `find_available_port` + `INSERT` outside tx, unlike Go transactional path; race window under contention.  
`rust/crates/forge-db/src/port_repository.rs:134` (Go ref: `internal/db/port_repository.go:70`)  
Fix hint: wrap select+insert in one DB transaction (or retry tx).

2. High: `set_current` unique-conflict fallback can false-success. Fallback `tx.execute(...)` result ignored, then returns `Ok(())` even if update fails.  
`rust/crates/forge-db/src/loop_work_state_repository.rs:204`  
Fix hint: check fallback execute result; return `DbError::Transaction` on failure.

3. Medium: loop-run status coercion bug. Unknown DB status is coerced to `Running` via `unwrap_or_default`, masking malformed rows/parity behavior.  
`rust/crates/forge-db/src/loop_run_repository.rs:266` (Go ref: `internal/db/loop_run_repository.go:218`)  
Fix hint: preserve unknown explicitly or return validation error; add malformed-status regression test.

**Validation**
- `cargo test -p forge-db`: many reviewed repo tests passed; run failed later in unrelated `file_lock_repository_test`.
- Targeted rerun then blocked by concurrent churn compile break in `rust/crates/forge-db/src/event_repository.rs:246`.
- `go test ./internal/db/...` blocked by local Go mismatch (`go1.25.7` stdlib vs `go1.25.6` tool).

**Reporting**
- Sent to task channel:
  - `review: forge-qe5 ...` (3 findings)
- Sent orchestrator summary:
  - `review summary: forge-qe5 issues`
- Appended review note:
  - `docs/review/2026-02-09-forge-4ss-review-loop.md`
```

## 2026-02-09T18:22:12Z

- run_id: 3d8664c7-66f7-49f2-bff4-c8d2b93c2f0f
- loop_name: rust-full-review-1
- status: success
- profile: codex3
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/rust-swarm-review.md
- prompt_override: false
- started_at: 2026-02-09T18:18:17Z
- finished_at: 2026-02-09T18:22:12Z
- exit_code: 0

```
**Residual Risk**
1. Go parity validation still blocked by toolchain mismatch.
2. No explicit Rust test yet for broadcast inbox cross-workspace isolation behavior.

**Reports Sent**
1. `fmail send task "review: forge-qe5 ..."` sent (`20260209-182148-3056`).
2. `fmail send @forge-orchestrator "review summary: forge-qe5 pass ..."` sent (`20260209-182148-3055`).

**Review Log**
1. Appended iteration note in `docs/review/2026-02-09-forge-4ss-review-loop.md`.
file update:
diff --git a/docs/review/2026-02-09-forge-4ss-review-loop.md b/docs/review/2026-02-09-forge-4ss-review-loop.md
index c5705aadc8a90fe86a3c6f4a66284bd26dd0774a..bb8dd419b8f8a3a6fb84939664f26662d085b249
--- a/docs/review/2026-02-09-forge-4ss-review-loop.md
+++ b/docs/review/2026-02-09-forge-4ss-review-loop.md
@@ -60,3 +60,23 @@
 ### Summary
 
 - Result: **issues found** (3).
+
+---
+
+## Iteration: `forge-qe5` (mail repository delta)
+
+### Findings
+
+1. No concrete defects found in reviewed delta (`rust/crates/forge-db/src/mail_repository.rs`, `rust/crates/forge-db/tests/mail_repository_test.rs`, `rust/crates/forge-db/src/lib.rs`).
+
+### Validation
+
+- `cargo test -p forge-db --test mail_repository_test` ✅ (3 passed)
+- `cargo test -p forge-db --test migration_006_test` ✅ (1 passed)
+- `go test ./internal/db/...` ❌ blocked by local Go toolchain mismatch:
+  - stdlib compiled with `go1.25.7`, tool is `go1.25.6`.
+
+### Summary
+
+- Result: **pass**.
+- Residual risk: Go parity checks remain blocked by toolchain mismatch; broadcast inbox cross-workspace isolation is not explicitly covered by current Rust tests.

tokens used
85,312
**Findings**
1. No concrete defects found in reviewed `forge-qe5` delta.

**Validation**
1. `cargo test -p forge-db --test mail_repository_test` passed (3/3).
2. `cargo test -p forge-db --test migration_006_test` passed (1/1).
3. `go test ./internal/db/...` blocked: stdlib built with `go1.25.7`, local tool `go1.25.6`.

**Residual Risk**
1. Go parity validation still blocked by toolchain mismatch.
2. No explicit Rust test yet for broadcast inbox cross-workspace isolation behavior.

**Reports Sent**
1. `fmail send task "review: forge-qe5 ..."` sent (`20260209-182148-3056`).
2. `fmail send @forge-orchestrator "review summary: forge-qe5 pass ..."` sent (`20260209-182148-3055`).

**Review Log**
1. Appended iteration note in `docs/review/2026-02-09-forge-4ss-review-loop.md`.
```

## 2026-02-09T18:25:50Z

- run_id: 2f58f276-57f3-432f-9c79-08d299fca21e
- loop_name: rust-full-review-1
- status: success
- profile: codex3
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/rust-swarm-review.md
- prompt_override: false
- started_at: 2026-02-09T18:24:12Z
- finished_at: 2026-02-09T18:25:50Z
- exit_code: 0

```
pub mod approval_repository;
pub mod event_repository;
pub mod file_lock_repository;
pub mod loop_queue_repository;
pub mod loop_repository;
pub mod loop_run_repository;
pub mod loop_work_state_repository;
pub mod mail_repository;
pub mod pool_repository;
pub mod port_repository;
pub mod profile_repository;
pub mod transcript_repository;
pub mod usage_repository;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
 succeeded in 52ms:


thinking
**Rechecking command range and output**
exec
/bin/zsh -lc "nl -ba rust/crates/forge-db/src/mail_repository.rs | sed -n '200,380p'" in /Users/trmd/Code/oss--forge/repos/forge succeeded in 52ms:


thinking
**Checking file accessibility and size**
exec
/bin/zsh -lc 'wc -l rust/crates/forge-db/src/mail_repository.rs && head -n 5 rust/crates/forge-db/src/mail_repository.rs' in /Users/trmd/Code/oss--forge/repos/forge succeeded in 52ms:
       0 rust/crates/forge-db/src/mail_repository.rs

thinking
**Pausing due to unexpected file change**
codex
hard blocker: `rust/crates/forge-db/src/mail_repository.rs` changed underneath review and is now `0` lines (`wc -l`), which I did not do. per safety rule I must stop here. how do you want to proceed: wait for writer to finish, or review a different task/file set now?

thinking
**Providing final user prompt**
codex
Blocked on concurrent change.

Observed during review:
- `rust/crates/forge-db/src/mail_repository.rs` was readable earlier, then became empty (`wc -l` => `0`) without edits from me.
- This explains current Rust test compile failures (missing `mail_repository` exports/types).

Need your direction before I continue:
1. Wait/retry after other agent finishes writing that file.
2. Switch review target to another active task right now.
tokens used
47,110
Blocked on concurrent change.

Observed during review:
- `rust/crates/forge-db/src/mail_repository.rs` was readable earlier, then became empty (`wc -l` => `0`) without edits from me.
- This explains current Rust test compile failures (missing `mail_repository` exports/types).

Need your direction before I continue:
1. Wait/retry after other agent finishes writing that file.
2. Switch review target to another active task right now.
```

## 2026-02-09T18:30:22Z

- run_id: 40b822bc-a958-492c-afeb-c846b5b1465c
- loop_name: rust-full-review-1
- status: success
- profile: codex3
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/rust-swarm-review.md
- prompt_override: false
- started_at: 2026-02-09T18:27:50Z
- finished_at: 2026-02-09T18:30:22Z
- exit_code: 0

```
test loop_queue_repository::tests::enqueue_auto_increments_position ... ok
test loop_queue_repository::tests::enqueue_rejects_empty_type ... ok
test loop_queue_repository::tests::enqueue_rejects_empty_payload ... ok

thinking
**Preparing detailed review notifications**
codex
issuing findings to `task` + orchestrator now with exact file/line and fix hints.
exec
/bin/zsh -lc 'cd /Users/trmd/Code/oss--forge/repos/forge && fmail send task "review: forge-3kr high mail_repository_test no longer compiles after RecipientType migration (E0308 on string literals) rust/crates/forge-db/tests/mail_repository_test.rs:76 Use RecipientType::Workspace/RecipientType::Broadcast in fixtures and enum-aware asserts"' in /Users/trmd/Code/oss--forge/repos/forgeexec
/bin/zsh -lc 'cd /Users/trmd/Code/oss--forge/repos/forge && fmail send task "review: forge-3kr medium validation regression coverage removed for non-broadcast missing recipient_id path rust/crates/forge-db/tests/mail_repository_test.rs:164 Re-add negative create_message case asserting Err(DbError::Validation(_))"' in /Users/trmd/Code/oss--forge/repos/forgeexec
/bin/zsh -lc 'cd /Users/trmd/Code/oss--forge/repos/forge && fmail send task "review: forge-3kr medium MailThreadAlreadyExists/MailMessageAlreadyExists added but create_* still bubbles rusqlite Open on UNIQUE conflicts rust/crates/forge-db/src/mail_repository.rs:85 Map UNIQUE errors to new DbError variants and add duplicate insert tests"' in /Users/trmd/Code/oss--forge/repos/forgeexec
/bin/zsh -lc 'cd /Users/trmd/Code/oss--forge/repos/forge && fmail send @forge-orchestrator "review summary: forge-3kr issues"' in /Users/trmd/Code/oss--forge/repos/forge succeeded in 52ms:
20260209-183010-3078
 succeeded in 51ms:
20260209-183010-3079
 succeeded in 51ms:
20260209-183010-3080
 succeeded in 51ms:
20260209-183010-3081

thinking
**Structuring concise final report**
codex
**Findings**
1. `high` Compile break in touched test file: `MailMessage.recipient_type` moved to enum, but tests still assign strings and compare to `&str` in `rust/crates/forge-db/tests/mail_repository_test.rs:76`, `rust/crates/forge-db/tests/mail_repository_test.rs:155`, `rust/crates/forge-db/tests/mail_repository_test.rs:170`.  
Fix hint: use `RecipientType::Workspace` / `RecipientType::Broadcast` in fixtures and enum equality assertions.

2. `medium` Coverage regression: negative validation case for non-broadcast message without `recipient_id` was removed from `rust/crates/forge-db/tests/mail_repository_test.rs` (around prior validation block near current `broadcast_and_validation_paths`).  
Fix hint: re-add assertion that workspace/agent recipient with missing `recipient_id` returns `Err(DbError::Validation(_))`.

3. `medium` Parity/API risk: new errors `MailThreadAlreadyExists` / `MailMessageAlreadyExists` were added in `rust/crates/forge-db/src/lib.rs:109` and `rust/crates/forge-db/src/lib.rs:113`, but `create_thread` / `create_message` still propagate raw sqlite errors via `?` in `rust/crates/forge-db/src/mail_repository.rs:85` and `rust/crates/forge-db/src/mail_repository.rs:170`.  
Fix hint: detect unique-constraint errors and map to the new typed `DbError` variants; add duplicate insert tests.

**Validation Evidence**
1. `cargo test -p forge-db --test mail_repository_test` -> failed with E0308 at the lines above.
2. `cargo test -p forge-db --lib` -> passed (`89 passed`).

**Reported**
1. Sent 3 task findings via `fmail send task` for `forge-3kr`.
2. Sent `fmail send @forge-orchestrator "review summary: forge-3kr issues"`.
tokens used
39,038
**Findings**
1. `high` Compile break in touched test file: `MailMessage.recipient_type` moved to enum, but tests still assign strings and compare to `&str` in `rust/crates/forge-db/tests/mail_repository_test.rs:76`, `rust/crates/forge-db/tests/mail_repository_test.rs:155`, `rust/crates/forge-db/tests/mail_repository_test.rs:170`.  
Fix hint: use `RecipientType::Workspace` / `RecipientType::Broadcast` in fixtures and enum equality assertions.

2. `medium` Coverage regression: negative validation case for non-broadcast message without `recipient_id` was removed from `rust/crates/forge-db/tests/mail_repository_test.rs` (around prior validation block near current `broadcast_and_validation_paths`).  
Fix hint: re-add assertion that workspace/agent recipient with missing `recipient_id` returns `Err(DbError::Validation(_))`.

3. `medium` Parity/API risk: new errors `MailThreadAlreadyExists` / `MailMessageAlreadyExists` were added in `rust/crates/forge-db/src/lib.rs:109` and `rust/crates/forge-db/src/lib.rs:113`, but `create_thread` / `create_message` still propagate raw sqlite errors via `?` in `rust/crates/forge-db/src/mail_repository.rs:85` and `rust/crates/forge-db/src/mail_repository.rs:170`.  
Fix hint: detect unique-constraint errors and map to the new typed `DbError` variants; add duplicate insert tests.

**Validation Evidence**
1. `cargo test -p forge-db --test mail_repository_test` -> failed with E0308 at the lines above.
2. `cargo test -p forge-db --lib` -> passed (`89 passed`).

**Reported**
1. Sent 3 task findings via `fmail send task` for `forge-3kr`.
2. Sent `fmail send @forge-orchestrator "review summary: forge-3kr issues"`.
```

## 2026-02-10T05:22:09Z

- run_id: 840354a2-b537-44e1-972a-51e8b7a89079
- loop_name: rust-full-review-1
- status: success
- profile: codex3
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/rust-swarm-review.md
- prompt_override: false
- started_at: 2026-02-10T05:18:09Z
- finished_at: 2026-02-10T05:22:09Z
- exit_code: 0

```
#[test]
+    fn set_messages_enforces_limit() {
+        let mut vm = OperatorViewModel::new("operator");
+        let msgs: Vec<OperatorMessage> = (0..(OPERATOR_MESSAGE_LIMIT + 5))
+            .map(|i| OperatorMessage {
+                id: format!("msg-{i}"),
+                from: "a".into(),
+                to: "@operator".into(),
+                body: "x".into(),
+                time_label: "10:00".into(),
+                priority: String::new(),
+                tags: Vec::new(),
+                reply_to: String::new(),
+                reply_preview: String::new(),
+                is_mine: false,
+            })
+            .collect();
+
+        vm.set_messages(msgs);
+
+        assert_eq!(vm.messages().len(), OPERATOR_MESSAGE_LIMIT);
+        assert_eq!(vm.messages()[0].id, "msg-5");
+        assert_eq!(
+            vm.messages()[OPERATOR_MESSAGE_LIMIT - 1].id,
+            format!("msg-{}", OPERATOR_MESSAGE_LIMIT + 4)
+        );
+    }
+
+    #[test]
     fn compose_push_pop_clear() {
         let mut vm = OperatorViewModel::new("operator");
         vm.compose_push('/');

tokens used
109,742
Hi. Tight loop, real defects found and reduced.

**Findings**
1. `HIGH` parity bug: operator compose submit path missing for `Enter`/`Ctrl+Enter` in Rust input handler; key consumed, no submit action (`rust/crates/fmail-tui/src/operator.rs:464`). Go baseline submits on those keys (`internal/fmailtui/operator_view.go:304`, `internal/fmailtui/operator_view.go:306`).  
Fix hint: return submit command/event (or don’t consume submit keys), add regression tests for both keys.

**Summary**
- Reviewed task: `forge-4kf`.
- Small fixes landed during review:
1. Graph `Shift+Tab` reverse selection parity + test (`rust/crates/fmail-tui/src/graph.rs:776`, `rust/crates/fmail-tui/src/graph.rs:1273`).
2. Enforce operator message cap `250` + test (`rust/crates/fmail-tui/src/operator.rs:188`).

**Validation**
- `env -u GOROOT -u GOTOOLDIR go test ./...` passed.
- `cd rust && cargo test -p fmail-tui` passed (261 passed, 1 ignored).

**Reporting**
- Sent to `task`:
  - high finding (open)
  - info note for 2 fixes landed
- Sent to `@forge-orchestrator`:
  - `review summary: forge-4kf issues (1 high open, 2 low-risk fixes landed with tests)`

**Review doc**
- `docs/review/2026-02-10-forge-4kf-review-loop.md`
```

## 2026-02-10T05:31:07Z

- run_id: 7fc02770-89a8-4176-bdd1-a3777ebe6d32
- loop_name: rust-full-review-1
- status: success
- profile: codex3
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/rust-swarm-review.md
- prompt_override: false
- started_at: 2026-02-10T05:24:09Z
- finished_at: 2026-02-10T05:31:07Z
- exit_code: 0

```
+
+        let _ = apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Enter)));
+        let action = apply_agents_input(&mut view, InputEvent::Key(KeyEvent::plain(Key::Enter)));
+        assert_eq!(
+            action,
+            AgentsAction::OpenThread {
+                target: "topic-a".to_owned()
+            }
+        );
+    }
+
+    #[test]
+    fn empty_cached_detail_still_requires_refresh() {
+        let now: i64 = 1_707_480_000;
+        let mut view = AgentsViewModel::new();
+        view.set_now(now);
+        view.load_agents(vec![AgentRecord::new("agent-empty")]);
+        view.load_detail("agent-empty", Vec::new());
+        assert!(view.needs_detail());
+    }
+
+    #[test]
     fn relative_time_cases() {
         assert_eq!(relative_time(0, 1000), "-");
         assert_eq!(relative_time(970, 1000), "30s");

tokens used
236,815
**Findings (reviewed `forge-4kf`)**
1. **HIGH** fixed: history selection/open-thread could target wrong message due render-sort vs unsorted navigation lookup mismatch.  
   - Fix: unified sorted history source for render/navigation/open (`rust/crates/fmail-tui/src/agents.rs:292`, `rust/crates/fmail-tui/src/agents.rs:501`, `rust/crates/fmail-tui/src/agents.rs:876`).
2. **HIGH** fixed: history could show/open wrong agent when `detail_agent` stale vs selected roster row.  
   - Fix: history now binds to `selected_agent`; Enter syncs `detail_agent` (`rust/crates/fmail-tui/src/agents.rs:501`, `rust/crates/fmail-tui/src/agents.rs:627`, `rust/crates/fmail-tui/src/agents.rs:876`).
3. **MEDIUM** fixed: presence indicator parity drift for future `last_seen` timestamps (Go uses signed delta semantics).  
   - Fix: switched from absolute to signed threshold logic (`rust/crates/fmail-tui/src/agents.rs:934`).

**Exact small-fix diff scope**
- Updated logic in `rust/crates/fmail-tui/src/agents.rs`:
  - `history_target`, `needs_detail`, `move_history`, `render_history`, `agent_presence_indicator`
  - Added `sorted_history_results` helper
- Added regression tests in `rust/crates/fmail-tui/src/agents.rs`:
  - `presence_indicator_future_timestamp_matches_go_behavior` (`:1112`)
  - `history_enter_uses_sorted_order` (`:1410`)
  - `history_uses_selected_agent_not_stale_detail_agent` (`:1452`)
  - `empty_cached_detail_still_requires_refresh` (`:1504`)
- Appended review log: `docs/review/2026-02-10-forge-4kf-review-loop.md`

**Validation**
- `cd rust && cargo check -p fmail-tui --lib` ✅
- `cd rust && cargo test -p fmail-tui --lib` ✅ (300 passed, 1 ignored)
- `GOTOOLCHAIN=go1.25.7 go test ./internal/fmailtui/...` ✅
- `GOTOOLCHAIN=go1.25.7 go test ./...` ⚠️ unrelated existing failure: `internal/parity` fixture drift (`TestProtoWireGateCriticalRPCFixtures`)

**Protocol comms sent**
- `fmail send task ...` for all findings + residual risk
- `fmail send @forge-orchestrator "review summary: forge-4kf pass ..."`

**Summary**
- Reviewed task `forge-4kf`; 3 real issues found and patched; no open defects left in touched agents-view scope.
- Residual risk: no Rust end-to-end provider/event-loop integration assertion yet for agents view.
```

