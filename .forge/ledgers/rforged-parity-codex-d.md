---
loop_id: e0b539a2-af08-4d2d-bfb0-e17d8f6a8061
loop_name: rforged-parity-codex-d
repo_path: /Users/trmd/Code/oss--forge/repos/forge
created_at: 2026-02-11T06:01:43Z
---

# Loop Ledger: rforged-parity-codex-d

## 2026-02-11T06:41:39Z

- run_id: 303d82d3-080d-47ae-b686-0413035e0c34
- loop_name: rforged-parity-codex-d
- status: success
- profile: codex2
- harness: codex
- prompt_source: base
- prompt_override: false
- started_at: 2026-02-11T06:01:43Z
- finished_at: 2026-02-11T06:41:39Z
- exit_code: 0

```
+            ],
+            &mut backend,
+        );
+        assert_eq!(out.exit_code, 0, "stderr: {}", out.stderr);
+
+        let db = forge_db::Db::open(forge_db::Config::new(&db_path))
+            .unwrap_or_else(|err| panic!("reopen db: {err}"));
+        let loop_repo = forge_db::loop_repository::LoopRepository::new(&db);
+        let created = loop_repo
+            .get_by_name("local-loop")
+            .unwrap_or_else(|err| panic!("load loop: {err}"));
+        let metadata = created
+            .metadata
+            .unwrap_or_else(|| panic!("missing loop metadata"));
+
+        assert_eq!(metadata.get("runner_owner"), Some(&json!("local")));
+        assert!(
+            metadata
+                .get("runner_instance_id")
+                .and_then(|v| v.as_str())
+                .is_some_and(|v| !v.trim().is_empty()),
+            "local owner should set runner_instance_id"
+        );
+
+        let _ = std::fs::remove_file(db_path);
+    }
+
+    #[test]
     fn up_sqlite_backend_persists_stop_config_metadata() {
         let db_path = temp_db_path("sqlite-stop-config");
         let _db = init_db(&db_path);

tokens used
824,679
Hi. Good momentum today; parity gap closed and validated.

- Closed `forge-97e`, `forge-sz7`, `forge-7yg`.
- Started/attempted next ready tasks; all remaining were already owned/in-progress by other agents. `sv task ready --project prj-2gawkgmn` now returns `0` ready.
- Commit created: `d169134` (`fix(forge-cli): reconcile daemon runner before stale mark`).
- No push performed.

Changed files:
- `rust/crates/forge-cli/src/ps.rs`
- `rust/crates/forge-cli/Cargo.toml`
- `rust/Cargo.lock`

What shipped:
- `forge ps` sqlite path now does Go-parity liveness reconciliation:
  - list daemon runners once
  - compute live pid/daemon liveness per loop
  - mark stale only when conditions match parity rules
  - preserve daemon-owned running loops when daemon unreachable
  - persist stale metadata (`runner_liveness` + `stale_runner`) on stale transition
- Added sqlite regression tests for stale cases and daemon-runner consult behavior.

Validation run:
- `cargo test -p forge-cli --lib ps::tests`
- `cargo test -p forge-cli --test ps_command_test`
- `EDITOR=true VISUAL=true GIT_EDITOR=true cargo test -p forge-cli`
- `cargo test -p forge-cli --no-run`
```

