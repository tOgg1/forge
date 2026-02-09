use forge_cli::run_for_test;

#[test]
fn completion_bash_matches_golden() {
    let out = run(&["completion", "bash"]);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty());
    assert_eq!(out.stdout, include_str!("golden/completion/bash.txt"));
}

#[test]
fn completion_zsh_matches_golden() {
    let out = run(&["completion", "zsh"]);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty());
    assert_eq!(out.stdout, include_str!("golden/completion/zsh.txt"));
}

#[test]
fn completion_fish_matches_golden() {
    let out = run(&["completion", "fish"]);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty());
    assert_eq!(out.stdout, include_str!("golden/completion/fish.txt"));
}

#[test]
fn completion_unsupported_shell_errors() {
    let out = run(&["completion", "tcsh"]);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, "error: unsupported shell: tcsh\n");
}

#[test]
fn completion_requires_one_argument() {
    let out = run(&["completion"]);
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.is_empty());
    assert_eq!(
        out.stderr,
        "error: accepts exactly 1 argument: [bash|zsh|fish]\n"
    );
}

#[test]
fn completion_help_outputs_usage() {
    let out = run(&["completion", "--help"]);
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty());
    assert_eq!(out.stdout, include_str!("golden/completion/help.txt"));
}

fn run(args: &[&str]) -> forge_cli::RootCommandOutput {
    run_for_test(args)
}
