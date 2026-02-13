#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;

#[test]
fn docs_cli_covers_root_help_command_families() {
    let help = run(&["--help"]);
    assert_eq!(help.exit_code, 0);

    let docs_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/cli.md");
    let docs_text = std::fs::read_to_string(&docs_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", docs_path.display()));

    let command_ids = parse_root_help_command_ids(&help.stdout);
    let documented_ids = parse_documented_command_ids(&docs_text);

    const DOC_EXCLUSIONS: [&str; 0] = [];

    let missing = command_ids
        .into_iter()
        .filter(|cmd| !DOC_EXCLUSIONS.contains(&cmd.as_str()) && !documented_ids.contains(cmd))
        .collect::<Vec<_>>();

    assert!(
        missing.is_empty(),
        "docs/cli.md missing command sections for: {}",
        missing.join(", ")
    );
}

fn run(args: &[&str]) -> forge_cli::RootCommandOutput {
    forge_cli::run_for_test(args)
}

fn parse_root_help_command_ids(help_text: &str) -> BTreeSet<String> {
    let mut in_commands = false;
    let mut ids = BTreeSet::new();

    for line in help_text.lines() {
        if line.trim() == "Commands:" {
            in_commands = true;
            continue;
        }
        if in_commands && line.trim() == "Global Flags:" {
            break;
        }
        if !in_commands {
            continue;
        }

        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(command_id) = trimmed.split_whitespace().next() {
            ids.insert(command_id.to_string());
        }
    }

    ids
}

fn parse_documented_command_ids(docs_text: &str) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();

    for line in docs_text.lines() {
        if !line.starts_with("### ") {
            continue;
        }

        let mut rest = line;
        while let Some(start_tick) = rest.find('`') {
            rest = &rest[start_tick + 1..];
            let Some(end_tick) = rest.find('`') else {
                break;
            };
            let code_span = &rest[..end_tick];
            rest = &rest[end_tick + 1..];

            let Some(command_span) = code_span.strip_prefix("forge ") else {
                continue;
            };

            let mut parts = command_span.split_whitespace();
            let Some(first) = parts.next() else {
                continue;
            };

            if first == "loop" {
                if let Some(subcommand) = parts.next() {
                    ids.insert(subcommand.to_string());
                }
            } else {
                ids.insert(first.to_string());
            }
        }
    }

    ids
}
