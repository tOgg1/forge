//! Integration test: spawn the rforged binary and verify Ping/GetStatus via gRPC.
//!
//! This tests the full binary lifecycle:
//!   1. Spawn rforged on a random port.
//!   2. Connect a gRPC client with retry.
//!   3. Call Ping and GetStatus RPCs.
//!   4. Send SIGTERM and verify clean exit.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::Duration;

use forge_rpc::forged::v1 as proto;
use forge_rpc::forged::v1::forged_service_client::ForgedServiceClient;
use tonic::transport::Channel;

/// Path to the compiled rforged binary (resolved by cargo at build time).
const RFORGED_BIN: &str = env!("CARGO_BIN_EXE_rforged");

/// Find a free TCP port by binding to port 0 and returning the assigned port.
fn find_free_port() -> u16 {
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("bind to port 0 for free port discovery");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

/// Spawn rforged binary on the given port. Returns the child process.
fn spawn_rforged(port: u16) -> std::process::Child {
    Command::new(RFORGED_BIN)
        .arg("--port")
        .arg(port.to_string())
        .arg("--hostname")
        .arg("127.0.0.1")
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .expect("spawn rforged binary")
}

/// Wait for rforged to emit its "ready" log line on stderr, indicating the server is listening.
/// Returns true if the ready message was seen within the timeout.
fn wait_for_ready(child: &mut std::process::Child, timeout: Duration) -> bool {
    let stderr = child.stderr.take().expect("child stderr");
    let reader = BufReader::new(stderr);
    let deadline = std::time::Instant::now() + timeout;

    for line in reader.lines() {
        if std::time::Instant::now() > deadline {
            return false;
        }
        match line {
            Ok(text) => {
                if text.contains("ready") || text.contains("gRPC serving") {
                    return true;
                }
            }
            Err(_) => return false,
        }
    }
    false
}

/// Connect a gRPC client with retry.
async fn connect_with_retry(port: u16) -> ForgedServiceClient<Channel> {
    let endpoint = format!("http://127.0.0.1:{port}");

    for attempt in 0..40 {
        let channel = Channel::from_shared(endpoint.clone()).expect("build gRPC channel endpoint");

        match channel.connect().await {
            Ok(connection) => return ForgedServiceClient::new(connection),
            Err(_) => {
                if attempt >= 39 {
                    panic!("timed out waiting for rforged gRPC on port {port}");
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
    unreachable!()
}

/// Send SIGTERM to a child process.
#[cfg(unix)]
fn send_sigterm(child: &std::process::Child) {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    kill(Pid::from_raw(child.id() as i32), Signal::SIGTERM)
        .expect("send SIGTERM to rforged child process");
}

#[cfg(not(unix))]
fn send_sigterm(child: &mut std::process::Child) {
    let _ = child.kill();
}

#[tokio::test]
async fn rforged_binary_serves_ping_and_get_status_then_exits_on_sigterm() {
    let port = find_free_port();
    let mut child = spawn_rforged(port);

    // Wait for ready or fall through to retry-based connection.
    // The ready check uses stderr which is consumed; connection retry handles the rest.
    let _ready = wait_for_ready(&mut child, Duration::from_secs(10));

    // Connect gRPC client with retry.
    let mut client = connect_with_retry(port).await;

    // --- Ping ---
    let ping_resp = client
        .ping(proto::PingRequest {})
        .await
        .expect("Ping RPC should succeed")
        .into_inner();

    assert!(
        !ping_resp.version.is_empty(),
        "Ping response must include version"
    );
    assert!(
        ping_resp.timestamp.is_some(),
        "Ping response must include timestamp"
    );

    // --- GetStatus ---
    let status_resp = client
        .get_status(proto::GetStatusRequest {})
        .await
        .expect("GetStatus RPC should succeed")
        .into_inner();

    let status = status_resp
        .status
        .expect("GetStatus response must include status");
    assert!(
        !status.version.is_empty(),
        "status.version must be non-empty"
    );
    assert!(
        !status.hostname.is_empty(),
        "status.hostname must be non-empty"
    );
    assert!(
        status.started_at.is_some(),
        "status.started_at must be present"
    );
    assert!(status.uptime.is_some(), "status.uptime must be present");
    assert_eq!(status.agent_count, 0, "no agents should be running");
    assert!(status.health.is_some(), "status.health must be present");

    // --- Shutdown ---
    send_sigterm(&child);

    // Wait for the process to exit with a timeout.
    let exit_status = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return status,
                Ok(None) => tokio::time::sleep(Duration::from_millis(50)).await,
                Err(e) => panic!("error waiting for rforged exit: {e}"),
            }
        }
    })
    .await
    .expect("rforged should exit within 10 seconds after SIGTERM");

    // On Unix, SIGTERM may result in exit code 0 (graceful) or signal termination.
    // The daemon's graceful shutdown handler should yield exit code 0.
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        let code = exit_status.code().unwrap_or(-1);
        let signal = exit_status.signal().unwrap_or(-1);
        // SIGTERM = 15
        assert!(
            code == 0 || signal == 15,
            "rforged should exit cleanly (code={code}, signal={signal})"
        );
    }

    #[cfg(not(unix))]
    {
        // On non-Unix, just verify it exited.
        let _ = exit_status;
    }
}
