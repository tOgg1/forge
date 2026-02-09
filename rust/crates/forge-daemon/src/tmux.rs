//! Tmux client abstraction for sending input to panes.

use std::process::Command;

/// Trait for sending keys/text to a tmux pane.
///
/// Abstracted for testability — the default implementation shells out to tmux.
pub trait TmuxClient: Send + Sync {
    /// Send text to a pane. If `literal` is true, use `-l` flag.
    /// If `enter` is true, press Enter after the text.
    fn send_keys(&self, target: &str, keys: &str, literal: bool, enter: bool)
        -> Result<(), String>;

    /// Send a special key (e.g. "C-c") to a pane without the `-l` flag.
    fn send_special_key(&self, target: &str, key: &str) -> Result<(), String>;

    /// Capture the current pane content.
    /// If `include_history` is true, include scrollback (`-S -`).
    fn capture_pane(&self, target: &str, include_history: bool) -> Result<String, String>;

    /// Check if a tmux session exists.
    fn has_session(&self, session_name: &str) -> Result<bool, String>;

    /// Create a new tmux session with the given name and working directory.
    fn new_session(&self, session_name: &str, working_dir: &str) -> Result<(), String>;

    /// Split a window in the given session, returning the new pane ID.
    /// If `horizontal` is true, split horizontally.
    fn split_window(
        &self,
        session_name: &str,
        horizontal: bool,
        working_dir: &str,
    ) -> Result<String, String>;

    /// Get the PID of the process running in a pane.
    fn get_pane_pid(&self, pane_id: &str) -> Result<i32, String>;

    /// Send Ctrl+C (interrupt) to a pane.
    fn send_interrupt(&self, pane_id: &str) -> Result<(), String>;

    /// Kill a tmux pane.
    fn kill_pane(&self, pane_id: &str) -> Result<(), String>;
}

/// Shell-based tmux client that execs `tmux send-keys`.
pub struct ShellTmuxClient;

impl TmuxClient for ShellTmuxClient {
    fn send_keys(
        &self,
        target: &str,
        keys: &str,
        literal: bool,
        enter: bool,
    ) -> Result<(), String> {
        if target.trim().is_empty() {
            return Err("target is required".to_string());
        }

        let escaped_target = escape_arg(target);
        let escaped_keys = escape_arg(keys);
        let literal_flag = if literal { " -l" } else { "" };

        let cmd_str = format!("tmux send-keys -t {escaped_target}{literal_flag} {escaped_keys}");
        exec_shell(&cmd_str)?;

        if enter {
            let enter_cmd = format!("tmux send-keys -t {escaped_target} Enter");
            exec_shell(&enter_cmd)?;
        }

        Ok(())
    }

    fn send_special_key(&self, target: &str, key: &str) -> Result<(), String> {
        if target.trim().is_empty() {
            return Err("target is required".to_string());
        }
        let cmd_str = format!("tmux send-keys -t {} {}", escape_arg(target), key);
        exec_shell(&cmd_str)
    }

    fn capture_pane(&self, target: &str, include_history: bool) -> Result<String, String> {
        if target.trim().is_empty() {
            return Err("target is required".to_string());
        }

        let escaped_target = escape_arg(target);
        let cmd_str = if include_history {
            format!("tmux capture-pane -t {escaped_target} -p -S -")
        } else {
            format!("tmux capture-pane -t {escaped_target} -p")
        };

        exec_shell_output(&cmd_str)
    }

    fn has_session(&self, session_name: &str) -> Result<bool, String> {
        if session_name.trim().is_empty() {
            return Err("session_name is required".to_string());
        }
        let cmd_str = format!("tmux has-session -t {}", escape_arg(session_name));
        let status = Command::new("sh")
            .args(["-c", &cmd_str])
            .status()
            .map_err(|e| format!("failed to execute tmux command: {e}"))?;
        Ok(status.success())
    }

    fn new_session(&self, session_name: &str, working_dir: &str) -> Result<(), String> {
        if session_name.trim().is_empty() {
            return Err("session_name is required".to_string());
        }
        let cmd_str = format!(
            "tmux new-session -d -s {} -c {}",
            escape_arg(session_name),
            escape_arg(working_dir),
        );
        exec_shell(&cmd_str)
    }

    fn split_window(
        &self,
        session_name: &str,
        horizontal: bool,
        working_dir: &str,
    ) -> Result<String, String> {
        if session_name.trim().is_empty() {
            return Err("session_name is required".to_string());
        }
        let split_flag = if horizontal { "-h" } else { "-v" };
        let cmd_str = format!(
            "tmux split-window {} -t {} -c {} -P -F '#{{pane_id}}'",
            split_flag,
            escape_arg(session_name),
            escape_arg(working_dir),
        );
        let output = exec_shell_output(&cmd_str)?;
        Ok(output.trim().to_string())
    }

    fn get_pane_pid(&self, pane_id: &str) -> Result<i32, String> {
        if pane_id.trim().is_empty() {
            return Err("pane_id is required".to_string());
        }
        let cmd_str = format!(
            "tmux display-message -t {} -p '#{{pane_pid}}'",
            escape_arg(pane_id),
        );
        let output = exec_shell_output(&cmd_str)?;
        output
            .trim()
            .parse::<i32>()
            .map_err(|e| format!("failed to parse PID: {e}"))
    }

    fn send_interrupt(&self, pane_id: &str) -> Result<(), String> {
        if pane_id.trim().is_empty() {
            return Err("pane_id is required".to_string());
        }
        let cmd_str = format!("tmux send-keys -t {} C-c", escape_arg(pane_id));
        exec_shell(&cmd_str)
    }

    fn kill_pane(&self, pane_id: &str) -> Result<(), String> {
        if pane_id.trim().is_empty() {
            return Err("pane_id is required".to_string());
        }
        let cmd_str = format!("tmux kill-pane -t {}", escape_arg(pane_id));
        exec_shell(&cmd_str)
    }
}

/// Shell-escape an argument using single quotes.
fn escape_arg(arg: &str) -> String {
    format!("'{}'", arg.replace('\'', "'\\''"))
}

fn exec_shell(cmd: &str) -> Result<(), String> {
    let status = Command::new("sh")
        .args(["-c", cmd])
        .status()
        .map_err(|e| format!("failed to execute tmux command: {e}"))?;

    if !status.success() {
        return Err(format!("tmux command failed with exit code: {status}"));
    }
    Ok(())
}

fn exec_shell_output(cmd: &str) -> Result<String, String> {
    let output = Command::new("sh")
        .args(["-c", cmd])
        .output()
        .map_err(|e| format!("failed to execute tmux command: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "tmux command failed with exit code: {}",
            output.status
        ));
    }

    String::from_utf8(output.stdout).map_err(|e| format!("tmux output was not valid UTF-8: {e}"))
}
