use forge_loop::iteration_result::{build_persisted_run_update, LoopRunStatus};
use forge_loop::stop_rules::{parse_qual_signal, qual_should_stop};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    transcript_fixture: Option<String>,
    transcript_inline: Option<String>,
    on_invalid: String,
    exit_code: i32,
    err_text: Option<String>,
    ledger_tail: String,
    primary_output_enabled: bool,
    expected_parse_signal: Option<i32>,
    expected_should_stop: bool,
    expected_status: String,
    expected_output_tail_source: String,
}

#[test]
fn runtime_characterization_from_go_fixtures_matches_iteration_outcomes() {
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join("runtime_characterization_fixture.json");
    let fixture_raw = std::fs::read_to_string(&fixture_path)
        .unwrap_or_else(|err| panic!("failed reading fixture {}: {err}", fixture_path.display()));
    let fixture: Fixture = serde_json::from_str(&fixture_raw)
        .unwrap_or_else(|err| panic!("failed parsing fixture {}: {err}", fixture_path.display()));

    for case in fixture.cases {
        let transcript = load_transcript(&case);
        let parse_signal = parse_qual_signal(&transcript);
        assert_eq!(
            parse_signal, case.expected_parse_signal,
            "fixture case {}: parse_qual_signal drift",
            case.name
        );

        let should_stop = qual_should_stop(&transcript, &case.on_invalid);
        assert_eq!(
            should_stop, case.expected_should_stop,
            "fixture case {}: qual_should_stop drift",
            case.name
        );

        let primary_output = if case.primary_output_enabled {
            transcript.as_str()
        } else {
            ""
        };
        let update = build_persisted_run_update(
            case.exit_code,
            primary_output,
            &case.ledger_tail,
            case.err_text.as_deref(),
        );

        let expected_status = match case.expected_status.as_str() {
            "success" => LoopRunStatus::Success,
            "error" => LoopRunStatus::Error,
            "running" => LoopRunStatus::Running,
            other => panic!(
                "fixture case {} has invalid expected_status {other}",
                case.name
            ),
        };
        assert_eq!(
            update.status, expected_status,
            "fixture case {}: persisted status drift",
            case.name
        );

        let expected_tail = match case.expected_output_tail_source.as_str() {
            "primary" => primary_output,
            "fallback" => case.ledger_tail.as_str(),
            other => panic!(
                "fixture case {} has invalid expected_output_tail_source {other}",
                case.name
            ),
        };
        assert_eq!(
            update.output_tail, expected_tail,
            "fixture case {}: persisted output tail source drift",
            case.name
        );
    }
}

fn load_transcript(case: &Case) -> String {
    if let Some(inline) = &case.transcript_inline {
        return inline.clone();
    }
    let rel = case
        .transcript_fixture
        .as_ref()
        .unwrap_or_else(|| panic!("fixture case {} missing transcript input", case.name));
    let repo_root = repo_root();
    let path = repo_root.join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("fixture case {} read {}: {err}", case.name, path.display()))
}

fn repo_root() -> PathBuf {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_dir
        .ancestors()
        .nth(2)
        .unwrap_or_else(|| panic!("failed resolving repo root from {}", crate_dir.display()))
        .to_path_buf()
}
