# forge-jyx - db rust-mutation compat path fix (2026-02-13)

## Summary

`TestGoReadsRustMutatedDB` failed by trying to run cargo in `old/go/rust`, which no
longer exists in current repo layout.

## Changes

- Updated `old/go/internal/db/rust_mutation_compat_test.go`:
  - replaced fixed `old/go/rust` derivation with upward workspace discovery
  - new helper `workspaceRootFromDBTestFile` finds root containing:
    - `Cargo.toml`
    - `crates/forge-db`
  - cargo seed command now runs from workspace root

## Validation

```bash
cd old/go
gofmt -w internal/db/rust_mutation_compat_test.go
env -u GOROOT -u GOTOOLDIR go test ./internal/db -run '^TestGoReadsRustMutatedDB$' -count=1
env -u GOROOT -u GOTOOLDIR go test ./internal/db -count=1
```

All pass.
