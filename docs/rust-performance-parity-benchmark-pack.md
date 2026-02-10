# Rust Performance Parity Benchmark Pack

Task: `forge-phd`  
Goal: compare Go vs Rust runtime speed for core operator commands and track
regression budget.

## Script

- `scripts/rust-parity-benchmark-pack.sh`

Default benchmark commands:

- `ps --json`
- `status --json`
- `tui --json`

## Standard run

```bash
scripts/rust-parity-benchmark-pack.sh \
  --go-bin ./build/forge \
  --rust-bin ./rust/target/release/rforge \
  --workdir . \
  --runs 10 \
  --budget-ratio 1.20 \
  --out-dir build/rust-parity-bench/latest
```

Outputs:

- `build/rust-parity-bench/latest/report.json`
- `build/rust-parity-bench/latest/summary.txt`

Pass criteria:

- Both binaries exit `0` for every command.
- `rust_mean_ms / go_mean_ms <= budget_ratio` for every command.

## Quick smoke run

Use a known-safe command when local state is not initialized:

```bash
scripts/rust-parity-benchmark-pack.sh \
  --go-bin ./build/forge \
  --rust-bin ./rust/target/release/rforge \
  --command "--version" \
  --runs 3 \
  --out-dir build/rust-parity-bench/smoke
```
