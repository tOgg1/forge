use forge_loop::stop_rules::parse_qual_signal;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    output: String,
    expected: Option<i32>,
}

#[test]
fn parse_qual_signal_matches_fixture() {
    let path = format!(
        "{}/testdata/qual_signal_fixture.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let raw = match std::fs::read_to_string(&path) {
        Ok(data) => data,
        Err(err) => panic!("failed reading fixture {path}: {err}"),
    };
    let fixture: Fixture = match serde_json::from_str(&raw) {
        Ok(data) => data,
        Err(err) => panic!("failed parsing fixture {path}: {err}"),
    };

    for case in fixture.cases {
        let got = parse_qual_signal(&case.output);
        assert_eq!(got, case.expected, "fixture case: {}", case.name);
    }
}
