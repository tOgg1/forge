use std::io::Write;

const BASH_COMPLETION: &str =
    "# bash completion for forge\n__start_forge()\n{\n    :\n}\ncomplete -F __start_forge forge\n";
const ZSH_COMPLETION: &str = "#compdef forge\n_arguments '*: :->args'\n";
const FISH_COMPLETION: &str = "complete -c forge -f\n";

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

    fn script(&self) -> &'static str {
        match self {
            Self::Bash => BASH_COMPLETION,
            Self::Zsh => ZSH_COMPLETION,
            Self::Fish => FISH_COMPLETION,
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

    if let Err(err) = write!(stdout, "{}", shell.script()) {
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
