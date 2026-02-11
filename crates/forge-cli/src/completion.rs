use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

const MAX_HELP_DEPTH: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

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

pub fn run(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    if args.len() >= 2 && matches!(args[1].as_str(), "-h" | "--help") {
        return write_help(stdout, stderr);
    }

    if args.len() != 2 {
        let _ = writeln!(stderr, "error: accepts exactly 1 argument: [bash|zsh|fish]");
        return 1;
    }

    let shell = match Shell::parse(args[1].trim()) {
        Ok(value) => value,
        Err(message) => {
            let _ = writeln!(stderr, "error: {message}");
            return 1;
        }
    };

    let snapshot = CompletionSnapshot::from_forge();
    let rendered = match shell {
        Shell::Bash => render_bash_script("forge", &snapshot),
        Shell::Zsh => render_zsh_script("forge", &snapshot),
        Shell::Fish => render_fish_script("forge", &snapshot),
    };

    if let Err(err) = write!(stdout, "{rendered}") {
        let _ = writeln!(stderr, "failed to write completion script: {err}");
        return 1;
    }

    0
}

fn write_help(stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    if let Err(err) = writeln!(
        stdout,
        "Usage: forge completion [bash|zsh|fish]\n\nGenerate shell completion scripts for bash, zsh, or fish."
    ) {
        let _ = writeln!(stderr, "failed to write help: {err}");
        return 1;
    }
    0
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct HelpSnapshot {
    commands: Vec<String>,
    flags: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CompletionSnapshot {
    /// Map of positional command path (e.g. "", "/config", "/lock/claim") to candidates.
    paths: BTreeMap<String, BTreeSet<String>>,
}

impl CompletionSnapshot {
    fn from_forge() -> Self {
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
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let _ = crate::run_with_args(&args, &mut stdout, &mut stderr);
    let rendered_stdout = String::from_utf8_lossy(&stdout).into_owned();
    if !rendered_stdout.trim().is_empty() {
        rendered_stdout
    } else {
        String::from_utf8_lossy(&stderr).into_owned()
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

pub fn run_for_test(args: &[&str]) -> CommandOutput {
    let owned: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit_code = run(&owned, &mut stdout, &mut stderr);
    CommandOutput {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit_code,
    }
}

#[cfg(test)]
mod tests {
    use super::run_for_test;

    #[test]
    fn bash_contains_start_function() {
        let out = run_for_test(&["completion", "bash"]);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert!(out.stdout.contains("__start_forge"));
    }

    #[test]
    fn unsupported_shell_errors() {
        let out = run_for_test(&["completion", "tcsh"]);
        assert_eq!(out.exit_code, 1);
        assert!(out.stdout.is_empty());
        assert_eq!(out.stderr, "error: unsupported shell: tcsh\n");
    }
}
