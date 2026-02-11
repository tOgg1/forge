fn main() {
    fmail_cli::set_version(env!("CARGO_PKG_VERSION"));
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = fmail_cli::run_cli(&args, &fmail_cli::FilesystemFmailBackend);
    if !out.stdout.is_empty() {
        print!("{}", out.stdout);
    }
    if !out.stderr.is_empty() {
        eprint!("{}", out.stderr);
    }
    std::process::exit(out.exit_code);
}
