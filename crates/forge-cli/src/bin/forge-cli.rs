fn main() {
    forge_cli::set_version(
        option_env!("FORGE_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")),
        option_env!("FORGE_COMMIT").unwrap_or("none"),
        option_env!("FORGE_BUILD_DATE").unwrap_or("unknown"),
    );
    let code = forge_cli::run_from_env();
    std::process::exit(code);
}
