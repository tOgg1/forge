//! Entry point for the forged daemon binary.

#[path = "shared/daemon_main.rs"]
mod daemon_main;

fn main() {
    daemon_main::run("forged");
}
