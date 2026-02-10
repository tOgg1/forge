---
loop_id: 246b21f4-6a68-41a0-b4ff-10fee5fef12e
loop_name: rust-parity-proof-codex
repo_path: /Users/trmd/Code/oss--forge/repos/forge
created_at: 2026-02-10T12:03:56Z
---

# Loop Ledger: rust-parity-proof-codex

## 2026-02-10T12:45:51Z

- run_id: 24e8f667-df7e-485b-b482-8656603ce8e4
- loop_name: rust-parity-proof-codex
- status: success
- profile: codex2
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/rust-swarm-dev-codex-continuous.md
- prompt_override: false
- started_at: 2026-02-10T12:03:56Z
- finished_at: 2026-02-10T12:45:51Z
- exit_code: 0

```
+      echo "crate=$crate (SKIP: no modified files)"
+      echo ""
+    } | tee -a "$per_crate_summary_path"
+    continue
+  fi
+
+  echo "enforcing $crate >= ${threshold}% line coverage on modified file(s)"
   {
     echo "crate=$crate"
-    cargo llvm-cov --package "$crate" --summary-only --fail-under-lines "$threshold"
+    while IFS= read -r abs; do
+      abs="$(trim "$abs")"
+      [[ -z "$abs" ]] && continue
+      if ! got="$(file_pct "$abs")"; then
+        echo "missing coverage data for file: $abs" >&2
+        exit 1
+      fi
+      pct="$(printf '%s' "$got" | cut -f1)"
+      lh="$(printf '%s' "$got" | cut -f2)"
+      lf="$(printf '%s' "$got" | cut -f3)"
+      printf 'file=%s lines=%s/%s pct=%s%% threshold=%s%%\n' "$abs" "$lh" "$lf" "$pct" "$threshold"
+      if awk -v pct="$pct" -v thr="$threshold" 'BEGIN{ exit !(pct+0 < thr+0) }'; then
+        echo "coverage below threshold for $crate: $abs is ${pct}% (< ${threshold}%)" >&2
+        exit 1
+      fi
+done <<< "$files"
     echo ""
   } | tee -a "$per_crate_summary_path"
-done < "$threshold_file"
+done < "$tmp_threshold_rows"
 
 if [[ -s "$tmp_waivers" ]]; then
   unknown_waiver_crates="$(comm -23 <(cut -f1 "$tmp_waivers" | sort -u) <(sort -u "$tmp_threshold_crates") || true)"

tokens used
354,994
Closed PAR tasks:
- `forge-800` (PAR-060): coverage gate now enforces thresholds on modified Rust source files via `rust/coverage/lcov.info`; CI/nightly updated to generate LCOV without global 100% fail-under; self-test updated; policy updated.
- `forge-j6t` (PAR-051): add Rust temp binary `rfmail` (`rust/crates/fmail-cli`) + smoke test; restore missing `rust/crates/forge-tui/src/bin/forge-tui.rs` so workspace builds/fmt works.
- `forge-c4g` (PAR-046): already satisfied (aliases + global flags); closed.
- `forge-a3s` (PAR-034): already satisfied (clean parity tests); closed.
- `forge-fzj` (PAR-032): already satisfied (rm parity tests); closed.
- `forge-qfq` (PAR-036): already satisfied (hidden loop command parity tests); closed.
- Batch-closed (parity tests already present; `cargo test --workspace`): `forge-k4k` (PAR-030), `forge-wzq` (PAR-028), `forge-sbd` (PAR-026), `forge-the` (PAR-024), `forge-b2v` (PAR-022), `forge-r6q` (PAR-020), `forge-505` (PAR-018), `forge-kkf` (PAR-016), `forge-994` (PAR-014), `forge-gz5` (PAR-012), `forge-4nt` (PAR-010), `forge-yvb` (PAR-008), `forge-c26` (PAR-006).

Key file changes:
- `scripts/rust-coverage-gate.sh`
- `.github/workflows/ci.yml`
- `.github/workflows/rust-coverage-nightly.yml`
- `.github/workflows/coverage-gate-self-test.yml`
- `docs/rust-coverage-policy.md`
- `rust/crates/fmail-cli/Cargo.toml`
- `rust/crates/fmail-cli/src/bin/rfmail.rs`
- `rust/crates/fmail-cli/tests/rfmail_wrapper_smoke_test.rs`
- `rust/crates/forge-tui/src/bin/forge-tui.rs`

Validation run:
- `scripts/rust-coverage-gate.sh`
- `env -u GOROOT -u GOTOOLDIR go test ./...`
- `(cd rust && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace)`
```

