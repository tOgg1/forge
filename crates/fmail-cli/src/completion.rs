//! fmail completion: generate shell completion scripts.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use fmail_core::agent_registry::AgentRecord;
use fmail_core::message::Message;
use fmail_core::store::TopicSummary;

use crate::{CommandOutput, FmailBackend};

const MAX_HELP_DEPTH: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Shell {
    Bash,
    Zsh,
    Fish,
}

impl Shell {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "bash" => Ok(Self::Bash),
            "zsh" => Ok(Self::Zsh),
            "fish" => Ok(Self::Fish),
            other => Err(format!("unsupported shell: {other}")),
        }
    }
}

pub fn run_completion_for_test(args: &[&str]) -> CommandOutput {
    if args.first().is_some_and(|a| *a == "-h" || *a == "--help") {
        return CommandOutput {
            stdout: "Usage: fmail completion [bash|zsh|fish]\n\nGenerate shell completion scripts for bash, zsh, or fish.\n".to_string(),
            stderr: String::new(),
            exit_code: 0,
        };
    }

    if args.len() != 1 {
        return CommandOutput {
            stdout: String::new(),
            stderr: "error: accepts exactly 1 argument: [bash|zsh|fish]\n".to_string(),
            exit_code: 1,
        };
    }

    let shell = match Shell::parse(args[0].trim()) {
        Ok(shell) => shell,
        Err(message) => {
            return CommandOutput {
                stdout: String::new(),
                stderr: format!("error: {message}\n"),
                exit_code: 1,
            };
        }
    };

    let snapshot = CompletionSnapshot::from_fmail();
    let rendered = match shell {
        Shell::Bash => render_bash_script("fmail", &snapshot),
        Shell::Zsh => render_zsh_script("fmail", &snapshot),
        Shell::Fish => render_fish_script("fmail", &snapshot),
    };

    CommandOutput {
        stdout: rendered,
        stderr: String::new(),
        exit_code: 0,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct HelpSnapshot {
    commands: Vec<String>,
    flags: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CompletionSnapshot {
    paths: BTreeMap<String, BTreeSet<String>>,
}

impl CompletionSnapshot {
    fn from_fmail() -> Self {
        let root = render_help(&[]);
        let parsed_root = parse_help_snapshot(&root);
        let root_flags = parsed_root.flags.clone();

        let mut snapshot = Self::default();
        let mut root_candidates = BTreeSet::new();
        root_candidates.extend(root_flags.iter().cloned());
        root_candidates.extend(parsed_root.commands.iter().cloned());
        snapshot.paths.insert(String::new(), root_candidates);

        let mut visited = BTreeSet::new();
        for command in parsed_root.commands {
            let path = vec![command];
            collect_path(&path, 1, &root_flags, &mut snapshot, &mut visited);
        }

        snapshot
    }
}

fn collect_path(
    path: &[String],
    depth: usize,
    root_flags: &[String],
    snapshot: &mut CompletionSnapshot,
    visited: &mut BTreeSet<String>,
) {
    if depth > MAX_HELP_DEPTH {
        return;
    }

    let key = format!("/{}", path.join("/"));
    if !visited.insert(key.clone()) {
        return;
    }

    let help = render_help(path);
    let parsed = parse_help_snapshot(&help);

    let mut candidates = BTreeSet::new();
    candidates.extend(root_flags.iter().cloned());
    candidates.extend(parsed.flags.iter().cloned());
    candidates.extend(parsed.commands.iter().cloned());
    snapshot.paths.insert(key, candidates);

    for subcommand in parsed.commands {
        let mut next = path.to_vec();
        next.push(subcommand);
        collect_path(&next, depth + 1, root_flags, snapshot, visited);
    }
}

fn render_help(path: &[String]) -> String {
    let mut args = path.to_vec();
    args.push("--help".to_string());
    let refs: Vec<&str> = args.iter().map(|part| part.as_str()).collect();
    let out = crate::run_cli_for_test(&refs, &HelpOnlyBackend);
    if !out.stdout.trim().is_empty() {
        out.stdout
    } else {
        out.stderr
    }
}

fn parse_help_snapshot(help: &str) -> HelpSnapshot {
    #[derive(Clone, Copy)]
    enum Section {
        None,
        Commands,
        Flags,
    }

    let mut section = Section::None;
    let mut commands = BTreeSet::new();
    let mut flags = BTreeSet::new();

    for raw_line in help.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if is_commands_heading(trimmed) {
            section = Section::Commands;
            continue;
        }
        if is_flags_heading(trimmed) {
            section = Section::Flags;
            continue;
        }
        if trimmed.ends_with(':') {
            section = Section::None;
            continue;
        }

        match section {
            Section::Commands => {
                if let Some(command) = parse_command_entry(trimmed) {
                    commands.insert(command);
                }
            }
            Section::Flags => {
                for flag in parse_flag_entries(trimmed) {
                    flags.insert(flag);
                }
            }
            Section::None => {}
        }
    }

    HelpSnapshot {
        commands: commands.into_iter().collect(),
        flags: flags.into_iter().collect(),
    }
}

fn is_commands_heading(line: &str) -> bool {
    line.ends_with("Commands:") || line.ends_with("Subcommands:")
}

fn is_flags_heading(line: &str) -> bool {
    line.ends_with("Flags:")
}

fn parse_command_entry(line: &str) -> Option<String> {
    let first = line.split_whitespace().next()?;
    let token = first.trim_matches(|ch: char| ch == ',' || ch == ':');
    if token.is_empty() || token.starts_with('-') {
        return None;
    }
    if token
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        Some(token.to_string())
    } else {
        None
    }
}

fn parse_flag_entries(line: &str) -> Vec<String> {
    let mut entries = BTreeSet::new();
    let mut token = String::new();

    for ch in line.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_alphanumeric() || ch == '-' {
            token.push(ch);
            continue;
        }

        let is_long_flag = token.starts_with("--")
            && token.len() > 2
            && token
                .chars()
                .skip(2)
                .all(|part| part.is_ascii_alphanumeric() || part == '-');
        let is_short_flag = token.starts_with('-')
            && !token.starts_with("--")
            && token.len() == 2
            && token
                .chars()
                .nth(1)
                .is_some_and(|part| part.is_ascii_alphanumeric());
        if is_long_flag || is_short_flag {
            entries.insert(token.clone());
        }

        token.clear();
    }

    entries.into_iter().collect()
}

fn render_bash_script(binary: &str, snapshot: &CompletionSnapshot) -> String {
    let mut out = String::new();
    out.push_str(&format!("# bash completion for {binary}\n"));
    out.push_str(&format!("__start_{binary}()\n"));
    out.push_str("{\n");
    out.push_str("    local cur path w\n");
    out.push_str("    COMPREPLY=()\n");
    out.push_str("    cur=\"${COMP_WORDS[COMP_CWORD]}\"\n");
    out.push_str("    path=\"\"\n");
    out.push_str("    local i\n");
    out.push_str("    for ((i=1; i<COMP_CWORD; i++)); do\n");
    out.push_str("        w=\"${COMP_WORDS[i]}\"\n");
    out.push_str("        [[ \"$w\" == -* ]] && continue\n");
    out.push_str("        path+=\"/$w\"\n");
    out.push_str("    done\n");
    out.push_str("    local opts=\"\"\n");
    out.push_str("    case \"$path\" in\n");
    for (path, candidates) in &snapshot.paths {
        let case_path = if path.is_empty() {
            "''".to_string()
        } else {
            format!("'{path}'")
        };
        let opts = candidates_as_space_list(candidates);
        out.push_str(&format!("        {case_path}) opts=\"{opts}\" ;;\n"));
    }
    out.push_str("        *) opts=\"\" ;;\n");
    out.push_str("    esac\n");
    out.push_str("    COMPREPLY=( $(compgen -W \"$opts\" -- \"$cur\") )\n");
    out.push_str("}\n");
    out.push_str(&format!(
        "complete -o default -F __start_{binary} {binary}\n"
    ));
    out
}

fn render_zsh_script(binary: &str, snapshot: &CompletionSnapshot) -> String {
    let mut out = String::new();
    out.push_str(&format!("#compdef {binary}\n"));
    out.push_str(&format!("__start_{binary}() {{\n"));
    out.push_str("  local cur path w\n");
    out.push_str("  cur=\"${words[CURRENT]}\"\n");
    out.push_str("  path=\"\"\n");
    out.push_str("  local i\n");
    out.push_str("  for ((i=2; i<CURRENT; i++)); do\n");
    out.push_str("    w=\"${words[i]}\"\n");
    out.push_str("    [[ \"$w\" == -* ]] && continue\n");
    out.push_str("    path+=\"/$w\"\n");
    out.push_str("  done\n");
    out.push_str("  local -a opts\n");
    out.push_str("  case \"$path\" in\n");
    for (path, candidates) in &snapshot.paths {
        let case_path = if path.is_empty() {
            "''".to_string()
        } else {
            format!("'{path}'")
        };
        let opts = candidates_as_space_list(candidates);
        out.push_str(&format!("    {case_path}) opts=({opts}) ;;\n"));
    }
    out.push_str("    *) opts=() ;;\n");
    out.push_str("  esac\n");
    out.push_str("  compadd -- $opts\n");
    out.push_str("}\n");
    out.push_str(&format!("__start_{binary} \"$@\"\n"));
    out
}

fn render_fish_script(binary: &str, snapshot: &CompletionSnapshot) -> String {
    let mut out = String::new();
    out.push_str(&format!("# fish completion for {binary}\n"));
    out.push_str(&format!("function __{binary}_path_is\n"));
    out.push_str("    set -l expected $argv\n");
    out.push_str("    set -l tokens (commandline -opc)\n");
    out.push_str("    if test (count $tokens) -lt 1\n");
    out.push_str("        return 1\n");
    out.push_str("    end\n");
    out.push_str("    set -e tokens[1]\n");
    out.push_str("    set -l actual\n");
    out.push_str("    for token in $tokens\n");
    out.push_str("        if string match -qr '^-' -- $token\n");
    out.push_str("            continue\n");
    out.push_str("        end\n");
    out.push_str("        set actual $actual $token\n");
    out.push_str("    end\n");
    out.push_str("    if test (count $actual) -ne (count $expected)\n");
    out.push_str("        return 1\n");
    out.push_str("    end\n");
    out.push_str("    for idx in (seq (count $expected))\n");
    out.push_str("        if test \"$actual[$idx]\" != \"$expected[$idx]\"\n");
    out.push_str("            return 1\n");
    out.push_str("        end\n");
    out.push_str("    end\n");
    out.push_str("    return 0\n");
    out.push_str("end\n");
    out.push('\n');

    for (path, candidates) in &snapshot.paths {
        let opts = candidates_as_space_list(candidates);
        let condition = if path.is_empty() {
            format!("__{binary}_path_is")
        } else {
            let parts = path.trim_start_matches('/').replace('/', " ");
            format!("__{binary}_path_is {parts}")
        };
        out.push_str(&format!(
            "complete -c {binary} -f -n \"{condition}\" -a \"{opts}\"\n"
        ));
    }

    out
}

fn candidates_as_space_list(candidates: &BTreeSet<String>) -> String {
    candidates
        .iter()
        .cloned()
        .collect::<Vec<String>>()
        .join(" ")
}

struct HelpOnlyBackend;

impl FmailBackend for HelpOnlyBackend {
    fn list_agent_records(&self) -> Result<Option<Vec<AgentRecord>>, String> {
        Err("help-only backend".to_string())
    }

    fn read_agent_record(&self, _name: &str) -> Result<Option<AgentRecord>, String> {
        Err("help-only backend".to_string())
    }

    fn now_utc(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn register_agent_record(&self, _name: &str, _host: &str) -> Result<AgentRecord, String> {
        Err("help-only backend".to_string())
    }

    fn set_agent_status(
        &self,
        _name: &str,
        _status: &str,
        _host: &str,
    ) -> Result<AgentRecord, String> {
        Err("help-only backend".to_string())
    }

    fn hostname(&self) -> String {
        "localhost".to_string()
    }

    fn agent_name(&self) -> Result<String, String> {
        Err("help-only backend".to_string())
    }

    fn save_message(&self, _message: &mut Message) -> Result<String, String> {
        Err("help-only backend".to_string())
    }

    fn read_file(&self, _path: &str) -> Result<String, String> {
        Err("help-only backend".to_string())
    }

    fn list_topics(&self) -> Result<Option<Vec<TopicSummary>>, String> {
        Err("help-only backend".to_string())
    }

    fn list_message_files(&self, _target: Option<&str>) -> Result<Vec<PathBuf>, String> {
        Err("help-only backend".to_string())
    }

    fn read_message_at(&self, _path: &std::path::Path) -> Result<Message, String> {
        Err("help-only backend".to_string())
    }

    fn init_project(&self, _project_id: Option<&str>) -> Result<(), String> {
        Err("help-only backend".to_string())
    }

    fn gc_messages(&self, _days: i64, _dry_run: bool) -> Result<String, String> {
        Err("help-only backend".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::run_completion_for_test;

    #[test]
    fn bash_contains_start_function() {
        let out = run_completion_for_test(&["bash"]);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert!(out.stdout.contains("__start_fmail"));
    }

    #[test]
    fn zsh_contains_compdef() {
        let out = run_completion_for_test(&["zsh"]);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert!(out.stdout.contains("#compdef fmail"));
    }

    #[test]
    fn fish_contains_complete() {
        let out = run_completion_for_test(&["fish"]);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert!(out.stdout.contains("complete -c fmail"));
    }

    #[test]
    fn unsupported_shell_errors() {
        let out = run_completion_for_test(&["tcsh"]);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(out.stderr, "error: unsupported shell: tcsh\n");
    }

    #[test]
    fn requires_one_argument() {
        let out = run_completion_for_test(&[]);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(
            out.stderr,
            "error: accepts exactly 1 argument: [bash|zsh|fish]\n"
        );
    }

    #[test]
    fn help_flag_outputs_usage() {
        let out = run_completion_for_test(&["--help"]);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert!(out.stdout.contains("Usage: fmail completion"));
    }

    #[test]
    fn short_help_flag_outputs_usage() {
        let out = run_completion_for_test(&["-h"]);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert!(out.stdout.contains("Usage: fmail completion"));
    }

    #[test]
    fn too_many_arguments_errors() {
        let out = run_completion_for_test(&["bash", "zsh"]);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(
            out.stderr,
            "error: accepts exactly 1 argument: [bash|zsh|fish]\n"
        );
    }
}
