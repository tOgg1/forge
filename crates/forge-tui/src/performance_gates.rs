//! Performance benchmark suite + SLO gate evaluation helpers for Forge TUI views.

use std::collections::BTreeSet;
use std::time::Instant;

use serde_json::{Map, Value};

pub const PERF_GATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkCase {
    pub view_id: String,
    pub warmup_iterations: u64,
    pub measure_iterations: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkSuite {
    pub schema_version: u32,
    pub cases: Vec<BenchmarkCase>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkSuiteLoadOutcome {
    pub suite: BenchmarkSuite,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkSample {
    pub view_id: String,
    pub latency_ms: Vec<u64>,
    pub throughput_per_second: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewSlo {
    pub view_id: String,
    pub max_p50_ms: u64,
    pub max_p95_ms: u64,
    pub min_throughput_per_second: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SloMetricBreach {
    pub view_id: String,
    pub metric: String,
    pub actual: u64,
    pub threshold: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SloGateReport {
    pub passed: bool,
    pub checked_views: usize,
    pub missing_views: Vec<String>,
    pub breaches: Vec<SloMetricBreach>,
}

impl Default for BenchmarkSuite {
    fn default() -> Self {
        default_benchmark_suite()
    }
}

#[must_use]
pub fn default_benchmark_suite() -> BenchmarkSuite {
    BenchmarkSuite {
        schema_version: PERF_GATE_SCHEMA_VERSION,
        cases: vec![
            BenchmarkCase {
                view_id: "overview".to_owned(),
                warmup_iterations: 10,
                measure_iterations: 100,
            },
            BenchmarkCase {
                view_id: "logs".to_owned(),
                warmup_iterations: 10,
                measure_iterations: 120,
            },
            BenchmarkCase {
                view_id: "runs".to_owned(),
                warmup_iterations: 10,
                measure_iterations: 100,
            },
            BenchmarkCase {
                view_id: "multi-logs".to_owned(),
                warmup_iterations: 10,
                measure_iterations: 80,
            },
        ],
    }
}

#[must_use]
pub fn default_view_slos() -> Vec<ViewSlo> {
    vec![
        ViewSlo {
            view_id: "overview".to_owned(),
            max_p50_ms: 16,
            max_p95_ms: 33,
            min_throughput_per_second: 70,
        },
        ViewSlo {
            view_id: "logs".to_owned(),
            max_p50_ms: 20,
            max_p95_ms: 45,
            min_throughput_per_second: 55,
        },
        ViewSlo {
            view_id: "runs".to_owned(),
            max_p50_ms: 18,
            max_p95_ms: 40,
            min_throughput_per_second: 60,
        },
        ViewSlo {
            view_id: "multi-logs".to_owned(),
            max_p50_ms: 30,
            max_p95_ms: 60,
            min_throughput_per_second: 35,
        },
    ]
}

pub fn run_benchmark_case(case: &BenchmarkCase, mut f: impl FnMut()) -> BenchmarkSample {
    run_benchmark_case_with_work_units(case, 1, move || {
        f();
    })
}

pub fn run_benchmark_case_with_work_units(
    case: &BenchmarkCase,
    work_units_per_iteration: u64,
    mut f: impl FnMut(),
) -> BenchmarkSample {
    let warmup = case.warmup_iterations.max(1);
    for _ in 0..warmup {
        f();
    }

    let iterations = case.measure_iterations.max(1);
    let work_units_per_iteration = work_units_per_iteration.max(1);
    let mut latencies = Vec::with_capacity(iterations as usize);
    let total_start = Instant::now();
    for _ in 0..iterations {
        let start = Instant::now();
        f();
        let elapsed_ms = (start.elapsed().as_nanos() / 1_000_000) as u64;
        latencies.push(elapsed_ms);
    }
    let total_ms = (total_start.elapsed().as_nanos() / 1_000_000) as u64;
    let total_work_units = iterations.saturating_mul(work_units_per_iteration);
    let throughput_per_second = if total_ms == 0 {
        total_work_units
    } else {
        total_work_units.saturating_mul(1_000) / total_ms
    };

    BenchmarkSample {
        view_id: normalize_id(&case.view_id),
        latency_ms: latencies,
        throughput_per_second,
    }
}

#[must_use]
pub fn evaluate_slo_gates(slos: &[ViewSlo], samples: &[BenchmarkSample]) -> SloGateReport {
    let mut missing_views = Vec::new();
    let mut breaches = Vec::new();

    for slo in slos {
        let view_id = normalize_id(&slo.view_id);
        let Some(sample) = samples
            .iter()
            .find(|sample| normalize_id(&sample.view_id) == view_id)
        else {
            missing_views.push(view_id);
            continue;
        };

        let p50 = percentile_ms(&sample.latency_ms, 50).unwrap_or(u64::MAX);
        let p95 = percentile_ms(&sample.latency_ms, 95).unwrap_or(u64::MAX);

        if p50 > slo.max_p50_ms {
            breaches.push(SloMetricBreach {
                view_id: view_id.clone(),
                metric: "p50_ms".to_owned(),
                actual: p50,
                threshold: slo.max_p50_ms,
            });
        }
        if p95 > slo.max_p95_ms {
            breaches.push(SloMetricBreach {
                view_id: view_id.clone(),
                metric: "p95_ms".to_owned(),
                actual: p95,
                threshold: slo.max_p95_ms,
            });
        }
        if sample.throughput_per_second < slo.min_throughput_per_second {
            breaches.push(SloMetricBreach {
                view_id: view_id.clone(),
                metric: "throughput_per_second".to_owned(),
                actual: sample.throughput_per_second,
                threshold: slo.min_throughput_per_second,
            });
        }
    }

    missing_views.sort();
    let passed = missing_views.is_empty() && breaches.is_empty();
    SloGateReport {
        passed,
        checked_views: slos.len(),
        missing_views,
        breaches,
    }
}

#[must_use]
pub fn format_ci_gate_summary(report: &SloGateReport) -> String {
    if report.passed {
        return format!(
            "SLO gate: PASS (views={}, breaches=0)",
            report.checked_views
        );
    }

    let mut lines = vec![format!(
        "SLO gate: FAIL (views={}, missing={}, breaches={})",
        report.checked_views,
        report.missing_views.len(),
        report.breaches.len()
    )];

    for view in &report.missing_views {
        lines.push(format!("missing sample: {view}"));
    }
    for breach in &report.breaches {
        lines.push(format!(
            "breach: view={} metric={} actual={} threshold={}",
            breach.view_id, breach.metric, breach.actual, breach.threshold
        ));
    }

    lines.join("\n")
}

#[must_use]
pub fn persist_benchmark_suite(suite: &BenchmarkSuite) -> String {
    let normalized = normalize_suite(suite.clone(), &mut Vec::new());
    let mut root = Map::new();
    root.insert(
        "schema_version".to_owned(),
        Value::from(PERF_GATE_SCHEMA_VERSION),
    );
    root.insert(
        "cases".to_owned(),
        Value::Array(
            normalized
                .cases
                .iter()
                .map(|case| {
                    let mut item = Map::new();
                    item.insert("view_id".to_owned(), Value::from(case.view_id.clone()));
                    item.insert(
                        "warmup_iterations".to_owned(),
                        Value::from(case.warmup_iterations),
                    );
                    item.insert(
                        "measure_iterations".to_owned(),
                        Value::from(case.measure_iterations),
                    );
                    Value::Object(item)
                })
                .collect(),
        ),
    );

    match serde_json::to_string_pretty(&Value::Object(root)) {
        Ok(json) => json,
        Err(_) => "{}".to_owned(),
    }
}

#[must_use]
pub fn restore_benchmark_suite(raw: &str) -> BenchmarkSuiteLoadOutcome {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return BenchmarkSuiteLoadOutcome {
            suite: default_benchmark_suite(),
            warnings: Vec::new(),
        };
    }

    let mut warnings = Vec::new();
    let value = match serde_json::from_str::<Value>(trimmed) {
        Ok(value) => value,
        Err(err) => {
            return BenchmarkSuiteLoadOutcome {
                suite: default_benchmark_suite(),
                warnings: vec![format!("invalid benchmark suite json ({err})")],
            };
        }
    };

    let Some(obj) = value.as_object() else {
        return BenchmarkSuiteLoadOutcome {
            suite: default_benchmark_suite(),
            warnings: vec!["benchmark suite must be an object".to_owned()],
        };
    };

    let schema_version = obj
        .get("schema_version")
        .and_then(Value::as_u64)
        .unwrap_or(PERF_GATE_SCHEMA_VERSION as u64) as u32;
    if schema_version != PERF_GATE_SCHEMA_VERSION {
        warnings.push(format!(
            "unknown schema_version={schema_version}; attempting best-effort parse"
        ));
    }

    let cases = obj
        .get("cases")
        .and_then(Value::as_array)
        .map(|items| parse_cases(items, &mut warnings))
        .unwrap_or_default();

    let suite = BenchmarkSuite {
        schema_version: PERF_GATE_SCHEMA_VERSION,
        cases,
    };

    BenchmarkSuiteLoadOutcome {
        suite: normalize_suite(suite, &mut warnings),
        warnings,
    }
}

fn parse_cases(values: &[Value], warnings: &mut Vec<String>) -> Vec<BenchmarkCase> {
    let mut cases = Vec::new();
    for (index, value) in values.iter().enumerate() {
        let Some(obj) = value.as_object() else {
            warnings.push(format!("cases[{index}] ignored (not object)"));
            continue;
        };

        let view_id = obj
            .get("view_id")
            .and_then(Value::as_str)
            .map(normalize_id)
            .unwrap_or_default();
        if view_id.is_empty() {
            warnings.push(format!("cases[{index}] ignored (empty view_id)"));
            continue;
        }

        let warmup_iterations = obj
            .get("warmup_iterations")
            .and_then(Value::as_u64)
            .unwrap_or(10)
            .max(1);
        let measure_iterations = obj
            .get("measure_iterations")
            .and_then(Value::as_u64)
            .unwrap_or(100)
            .max(1);

        cases.push(BenchmarkCase {
            view_id,
            warmup_iterations,
            measure_iterations,
        });
    }
    cases
}

fn normalize_suite(mut suite: BenchmarkSuite, warnings: &mut Vec<String>) -> BenchmarkSuite {
    suite.schema_version = PERF_GATE_SCHEMA_VERSION;

    let mut seen = BTreeSet::new();
    suite.cases.retain(|case| {
        let view_id = normalize_id(&case.view_id);
        if view_id.is_empty() {
            return false;
        }
        if !seen.insert(view_id.clone()) {
            warnings.push(format!("duplicate benchmark case '{}' ignored", view_id));
            return false;
        }
        true
    });

    for case in &mut suite.cases {
        case.view_id = normalize_id(&case.view_id);
        case.warmup_iterations = case.warmup_iterations.max(1);
        case.measure_iterations = case.measure_iterations.max(1);
    }

    suite.cases.sort_by(|a, b| {
        a.view_id
            .cmp(&b.view_id)
            .then(a.measure_iterations.cmp(&b.measure_iterations))
    });

    if suite.cases.is_empty() {
        suite = default_benchmark_suite();
    }

    suite
}

fn normalize_id(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace(' ', "-")
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect()
}

fn percentile_ms(values: &[u64], percentile: u8) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let percentile = percentile.clamp(1, 100) as usize;
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = ((percentile * sorted.len()).saturating_sub(1)) / 100;
    sorted.get(rank).copied()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::hint::black_box;

    use super::{
        default_benchmark_suite, default_view_slos, evaluate_slo_gates, format_ci_gate_summary,
        persist_benchmark_suite, restore_benchmark_suite, run_benchmark_case,
        run_benchmark_case_with_work_units, BenchmarkCase, BenchmarkSample, ViewSlo,
    };
    use crate::app::{App, LogTailView, LoopView, MainTab, RunView};
    use forge_cli::logs::{render_lines_for_layer, LogRenderLayer};

    #[test]
    fn default_suite_has_expected_views() {
        let suite = default_benchmark_suite();
        let ids = suite
            .cases
            .iter()
            .map(|case| case.view_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["overview", "logs", "runs", "multi-logs"]);
    }

    #[test]
    fn run_case_produces_latency_series_and_throughput() {
        let case = BenchmarkCase {
            view_id: "overview".to_owned(),
            warmup_iterations: 2,
            measure_iterations: 5,
        };
        let sample = run_benchmark_case(&case, || {
            let _ = 1 + 1;
        });

        assert_eq!(sample.view_id, "overview");
        assert_eq!(sample.latency_ms.len(), 5);
        assert!(sample.throughput_per_second >= 1);
    }

    #[test]
    fn throughput_scales_with_work_units() {
        let case = BenchmarkCase {
            view_id: "stream-follow".to_owned(),
            warmup_iterations: 1,
            measure_iterations: 8,
        };
        let sample = run_benchmark_case_with_work_units(&case, 64, || {});
        assert_eq!(sample.view_id, "stream-follow");
        assert_eq!(sample.latency_ms.len(), 8);
        assert!(sample.throughput_per_second >= 64);
    }

    #[test]
    fn gate_passes_when_samples_meet_thresholds() {
        let slos = vec![ViewSlo {
            view_id: "overview".to_owned(),
            max_p50_ms: 10,
            max_p95_ms: 20,
            min_throughput_per_second: 5,
        }];
        let samples = vec![BenchmarkSample {
            view_id: "overview".to_owned(),
            latency_ms: vec![1, 2, 2, 3, 4],
            throughput_per_second: 50,
        }];

        let report = evaluate_slo_gates(&slos, &samples);
        assert!(report.passed);
        assert!(report.breaches.is_empty());
        assert!(report.missing_views.is_empty());
    }

    #[test]
    fn gate_reports_latency_and_throughput_breaches() {
        let slos = vec![ViewSlo {
            view_id: "overview".to_owned(),
            max_p50_ms: 2,
            max_p95_ms: 3,
            min_throughput_per_second: 100,
        }];
        let samples = vec![BenchmarkSample {
            view_id: "overview".to_owned(),
            latency_ms: vec![1, 3, 4, 5, 6],
            throughput_per_second: 10,
        }];

        let report = evaluate_slo_gates(&slos, &samples);
        assert!(!report.passed);
        assert_eq!(report.breaches.len(), 3);
        assert!(report
            .breaches
            .iter()
            .any(|breach| breach.metric == "p50_ms"));
        assert!(report
            .breaches
            .iter()
            .any(|breach| breach.metric == "p95_ms"));
        assert!(report
            .breaches
            .iter()
            .any(|breach| breach.metric == "throughput_per_second"));
    }

    #[test]
    fn gate_reports_missing_views() {
        let report = evaluate_slo_gates(
            &[ViewSlo {
                view_id: "overview".to_owned(),
                max_p50_ms: 10,
                max_p95_ms: 20,
                min_throughput_per_second: 1,
            }],
            &[],
        );
        assert!(!report.passed);
        assert_eq!(report.missing_views, vec!["overview"]);
    }

    #[test]
    fn ci_summary_formats_pass_and_fail() {
        let pass = evaluate_slo_gates(
            &[ViewSlo {
                view_id: "overview".to_owned(),
                max_p50_ms: 10,
                max_p95_ms: 20,
                min_throughput_per_second: 1,
            }],
            &[BenchmarkSample {
                view_id: "overview".to_owned(),
                latency_ms: vec![1, 1, 1],
                throughput_per_second: 100,
            }],
        );
        assert!(format_ci_gate_summary(&pass).contains("PASS"));

        let fail = evaluate_slo_gates(&default_view_slos(), &[]);
        let summary = format_ci_gate_summary(&fail);
        assert!(summary.contains("FAIL"));
        assert!(summary.contains("missing sample"));
    }

    #[test]
    fn suite_persist_restore_round_trip() {
        let suite = default_benchmark_suite();
        let json = persist_benchmark_suite(&suite);
        let restored = restore_benchmark_suite(&json);

        assert!(restored.warnings.is_empty());
        assert_eq!(restored.suite.schema_version, suite.schema_version);
        assert_eq!(restored.suite.cases.len(), suite.cases.len());
        let ids = restored
            .suite
            .cases
            .iter()
            .map(|case| case.view_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["logs", "multi-logs", "overview", "runs"]);
    }

    #[test]
    fn restore_rejects_invalid_entries_and_dedupes() {
        let raw = r#"
        {
          "schema_version": 9,
          "cases": [
            {"view_id": "overview", "warmup_iterations": 2, "measure_iterations": 3},
            {"view_id": "overview", "warmup_iterations": 1, "measure_iterations": 1},
            {"view_id": "", "warmup_iterations": 2, "measure_iterations": 3},
            "bad"
          ]
        }
        "#;

        let restored = restore_benchmark_suite(raw);
        assert!(!restored.warnings.is_empty());
        assert_eq!(restored.suite.cases.len(), 1);
        assert_eq!(restored.suite.cases[0].view_id, "overview");
    }

    fn sample_loop(index: usize) -> LoopView {
        LoopView {
            id: format!("loop-{index:03}"),
            short_id: format!("l{index:03}"),
            name: format!("operator-loop-{index:03}"),
            state: if index.is_multiple_of(6) {
                "error".to_owned()
            } else if index.is_multiple_of(3) {
                "sleeping".to_owned()
            } else {
                "running".to_owned()
            },
            repo_path: format!("/repo/operator-{index}"),
            runs: 40 + index,
            queue_depth: (index * 3) % 19,
            last_run_at: Some("2026-02-13T12:00:00Z".to_owned()),
            interval_seconds: 60,
            max_runtime_seconds: 900,
            max_iterations: 200,
            last_error: if index.is_multiple_of(6) {
                "exit status 1".to_owned()
            } else {
                String::new()
            },
            profile_name: "ops-prod".to_owned(),
            profile_harness: "codex".to_owned(),
            profile_auth: "sso".to_owned(),
            profile_id: "p-prod".to_owned(),
            pool_name: "fleet-main".to_owned(),
            pool_id: "pool-main".to_owned(),
        }
    }

    fn sample_run(index: usize) -> RunView {
        RunView {
            id: format!("run-{index:04}"),
            status: if index.is_multiple_of(7) {
                "error".to_owned()
            } else if index.is_multiple_of(5) {
                "running".to_owned()
            } else {
                "success".to_owned()
            },
            exit_code: if index.is_multiple_of(7) {
                Some(1)
            } else {
                Some(0)
            },
            duration: format!("{}s", 5 + index),
            profile_name: "ops-prod".to_owned(),
            harness: "codex".to_owned(),
            auth_kind: "sso".to_owned(),
        }
    }

    fn sample_log_lines(line_count: usize) -> Vec<String> {
        const PATTERNS: &[&str] = &[
            "[2026-02-13T12:00:00Z] status: running loop=operator",
            "tool: read file src/app.rs",
            "$ cargo test -p forge-tui",
            "diff --git a/src/app.rs b/src/app.rs",
            "@@ -1,2 +1,2 @@",
            "-old line",
            "+new line",
            "Traceback (most recent call last):",
            "  File \"runner.py\", line 42, in <module>",
            "ValueError: boom",
            "```rust",
            "fn main() { println!(\"ok\"); }",
            "```",
        ];

        (0..line_count)
            .map(|index| PATTERNS[index % PATTERNS.len()].to_owned())
            .collect()
    }

    fn build_render_fixture() -> App {
        let mut app = App::new("default", 400);
        let loops = (0..48).map(sample_loop).collect::<Vec<_>>();
        let runs = (0..64).map(sample_run).collect::<Vec<_>>();
        let selected_log = LogTailView {
            lines: sample_log_lines(1_200),
            message: String::new(),
        };

        let mut multi_logs = HashMap::new();
        for loop_view in loops.iter().take(12) {
            multi_logs.insert(
                loop_view.id.clone(),
                LogTailView {
                    lines: sample_log_lines(180),
                    message: String::new(),
                },
            );
        }

        app.set_loops(loops);
        app.set_run_history(runs);
        app.set_selected_log(selected_log);
        app.set_multi_logs(multi_logs);
        app
    }

    fn render_gate_slos() -> Vec<ViewSlo> {
        vec![
            ViewSlo {
                view_id: "overview".to_owned(),
                max_p50_ms: 18,
                max_p95_ms: 40,
                min_throughput_per_second: 45,
            },
            ViewSlo {
                view_id: "logs".to_owned(),
                max_p50_ms: 18,
                max_p95_ms: 40,
                min_throughput_per_second: 45,
            },
            ViewSlo {
                view_id: "runs".to_owned(),
                max_p50_ms: 18,
                max_p95_ms: 40,
                min_throughput_per_second: 45,
            },
            ViewSlo {
                view_id: "multi-logs".to_owned(),
                max_p50_ms: 35,
                max_p95_ms: 70,
                min_throughput_per_second: 25,
            },
        ]
    }

    fn measure_render_for_view(view_id: &str, app: &mut App) -> BenchmarkSample {
        let tab = match view_id {
            "overview" => MainTab::Overview,
            "logs" => MainTab::Logs,
            "runs" => MainTab::Runs,
            "multi-logs" => MainTab::MultiLogs,
            _ => MainTab::Overview,
        };
        app.set_tab(tab);
        let case = BenchmarkCase {
            view_id: view_id.to_owned(),
            warmup_iterations: 16,
            measure_iterations: 160,
        };
        run_benchmark_case(&case, || {
            black_box(app.render());
        })
    }

    #[test]
    fn ci_render_latency_and_throughput_budgets_hold() {
        let mut app = build_render_fixture();
        let samples = vec![
            measure_render_for_view("overview", &mut app),
            measure_render_for_view("logs", &mut app),
            measure_render_for_view("runs", &mut app),
            measure_render_for_view("multi-logs", &mut app),
        ];

        let report = evaluate_slo_gates(&render_gate_slos(), &samples);
        assert!(report.passed, "{}", format_ci_gate_summary(&report));
    }

    #[test]
    fn ci_follow_throughput_budget_holds() {
        const CHUNK_LINES: usize = 180;
        const MAX_WINDOW_LINES: usize = 1_200;
        let chunk = sample_log_lines(CHUNK_LINES);
        let mut rolling = Vec::new();
        let case = BenchmarkCase {
            view_id: "follow".to_owned(),
            warmup_iterations: 12,
            measure_iterations: 140,
        };

        let sample = run_benchmark_case_with_work_units(&case, CHUNK_LINES as u64, || {
            rolling.extend(chunk.iter().cloned());
            if rolling.len() > MAX_WINDOW_LINES {
                let overflow = rolling.len().saturating_sub(MAX_WINDOW_LINES);
                rolling.drain(0..overflow);
            }
            let rendered = render_lines_for_layer(&rolling, LogRenderLayer::Raw, true);
            black_box(rendered.len());
        });

        let report = evaluate_slo_gates(
            &[ViewSlo {
                view_id: "follow".to_owned(),
                max_p50_ms: 12,
                max_p95_ms: 24,
                min_throughput_per_second: 12_000,
            }],
            &[sample],
        );
        assert!(report.passed, "{}", format_ci_gate_summary(&report));
    }
}
