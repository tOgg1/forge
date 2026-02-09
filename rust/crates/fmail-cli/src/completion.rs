//! fmail completion: generate shell completion scripts.

use crate::CommandOutput;

const BASH_COMPLETION: &str =
    "# bash completion for fmail\n__start_fmail()\n{\n    :\n}\ncomplete -F __start_fmail fmail\n";
const ZSH_COMPLETION: &str = "#compdef fmail\n_arguments '*: :->args'\n";
const FISH_COMPLETION: &str = "complete -c fmail -f\n";

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

    fn script(&self) -> &'static str {
        match self {
            Self::Bash => BASH_COMPLETION,
            Self::Zsh => ZSH_COMPLETION,
            Self::Fish => FISH_COMPLETION,
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

    match Shell::parse(args[0].trim()) {
        Ok(shell) => CommandOutput {
            stdout: shell.script().to_string(),
            stderr: String::new(),
            exit_code: 0,
        },
        Err(message) => CommandOutput {
            stdout: String::new(),
            stderr: format!("error: {message}\n"),
            exit_code: 1,
        },
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
