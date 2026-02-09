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

