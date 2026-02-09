use forge_loop::queue_interactions::{
    build_queue_interaction_plan, should_inject_qualitative_stop, QueueControlItem,
};
use forge_loop::stop_rules::qual_should_stop;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    queue_items: Vec<String>,
    pending_steer_messages: usize,
    qual_due: bool,
    single_run: bool,
    judge_output: String,
    on_invalid: String,
    expect_inject_qualitative_stop: bool,
    expect_should_stop: bool,
}

#[test]
fn smart_stop_queue_fixture_matches_expected_semantics() {
    let path = format!(
        "{}/testdata/smart_stop_queue_fixture.json",
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
        let items: Vec<QueueControlItem> = case
            .queue_items
            .iter()
            .map(|item| map_queue_item(item))
            .collect();

        let plan = match build_queue_interaction_plan(&items, case.pending_steer_messages) {
            Ok(plan) => plan,
            Err(err) => panic!(
                "fixture case {}: unexpected planning error: {err}",
                case.name
            ),
        };

        let got_inject = should_inject_qualitative_stop(case.qual_due, case.single_run, &plan);
        assert_eq!(
            got_inject, case.expect_inject_qualitative_stop,
            "fixture case {}: inject qualitative stop",
            case.name
        );

        let got_should_stop = qual_should_stop(&case.judge_output, &case.on_invalid);
        assert_eq!(
            got_should_stop, case.expect_should_stop,
            "fixture case {}: qualitative stop decision",
            case.name
        );
    }
}

fn map_queue_item(value: &str) -> QueueControlItem {
    match value {
        "message_append" => QueueControlItem::MessageAppend,
        "next_prompt_override" => QueueControlItem::NextPromptOverride,
        "pause" => QueueControlItem::Pause,
        "stop_graceful" => QueueControlItem::StopGraceful,
        "kill_now" => QueueControlItem::KillNow,
        "steer_message" => QueueControlItem::SteerMessage,
        other => QueueControlItem::Unsupported(other.to_string()),
    }
}
