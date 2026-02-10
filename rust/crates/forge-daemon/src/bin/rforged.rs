//! Entry point for the Rust side-by-side daemon binary (`rforged`).

#[path = "shared/daemon_main.rs"]
mod daemon_main;

fn main() {
    daemon_main::run("rforged");
}
