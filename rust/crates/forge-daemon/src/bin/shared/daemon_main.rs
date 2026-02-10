//! Shared entrypoint implementation for daemon binaries (`forged`, `rforged`).

use forge_daemon::bootstrap::{build_daemon_options, init_logger, DaemonArgs, VersionInfo};

pub fn run(process_label: &str) {
    let version = VersionInfo::default();
    let args = parse_args();

    // Load config (config file loading is optional; defaults are used if absent).
    let cfg = forge_core::config::Config::default();

    // Build daemon options and logging config from CLI args + config.
    let (opts, log_cfg) = build_daemon_options(&args, &cfg);
    let logger = init_logger(&log_cfg);

    // Ensure data/config directories exist.
    if let Err(e) = cfg.ensure_directories() {
        logger.warn_with("failed to create directories", &[("error", &e.to_string())]);
    }

    // Log startup.
    logger.info_with(
        &format!("{process_label} starting"),
        &[
            ("version", &version.version),
            ("commit", &version.commit),
            ("built", &version.date),
        ],
    );

    logger.info_with(
        &format!("{process_label} ready"),
        &[("bind", &opts.bind_addr())],
    );

    // Placeholder: full daemon Run() + signal handling will be wired once
    // the gRPC service, database, scheduler, and mail modules are connected.
    // For now, print crate label for parity smoke test.
    println!("{}", forge_daemon::crate_label());
}

fn parse_args() -> DaemonArgs {
    let mut args = DaemonArgs::default();
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--hostname" => {
                if let Some(v) = iter.next() {
                    args.hostname = v;
                }
            }
            "--port" => {
                if let Some(v) = iter.next() {
                    if let Ok(p) = v.parse::<u16>() {
                        args.port = p;
                    }
                }
            }
            "--config" => {
                if let Some(v) = iter.next() {
                    args.config_file = v;
                }
            }
            "--log-level" => {
                if let Some(v) = iter.next() {
                    args.log_level = v;
                }
            }
            "--log-format" => {
                if let Some(v) = iter.next() {
                    args.log_format = v;
                }
            }
            "--disk-path" => {
                if let Some(v) = iter.next() {
                    args.disk_path = v;
                }
            }
            "--disk-warn" => {
                if let Some(v) = iter.next() {
                    if let Ok(f) = v.parse::<f64>() {
                        args.disk_warn = f;
                    }
                }
            }
            "--disk-critical" => {
                if let Some(v) = iter.next() {
                    if let Ok(f) = v.parse::<f64>() {
                        args.disk_critical = f;
                    }
                }
            }
            "--disk-resume" => {
                if let Some(v) = iter.next() {
                    if let Ok(f) = v.parse::<f64>() {
                        args.disk_resume = f;
                    }
                }
            }
            "--disk-pause" => {
                args.disk_pause = true;
            }
            _ => {} // Ignore unknown flags for forward-compatibility.
        }
    }
    args
}
