//! Lightweight perf measurement helpers.
//!
//! Intended for local regression checks. Not a CI gate.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerfResult {
    pub iterations: u64,
    pub total: Duration,
    pub per_iter: Duration,
}

#[must_use]
pub fn measure(mut iterations: u64, mut f: impl FnMut()) -> PerfResult {
    if iterations == 0 {
        iterations = 1;
    }
    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    let total = start.elapsed();
    let per_iter = Duration::from_nanos((total.as_nanos() / iterations as u128) as u64);
    PerfResult {
        iterations,
        total,
        per_iter,
    }
}
