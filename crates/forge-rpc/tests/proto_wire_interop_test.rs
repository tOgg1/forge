#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use forge_rpc::forged::v1 as proto;
use prost::Message;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ProtoWireFixture {
    rpc: String,
    wire_hex: String,
}

#[derive(Debug, Deserialize)]
struct ProtoWireSummary {
    fixtures: Vec<ProtoWireFixture>,
}

#[test]
fn rust_wire_encoding_matches_go_oracle_fixtures() {
    let summary = load_go_proto_wire_summary();
    let by_rpc: HashMap<String, String> = summary
        .fixtures
        .into_iter()
        .map(|f| (f.rpc, f.wire_hex))
        .collect();

    let base_ts = prost_types::Timestamp {
        seconds: 1_770_652_800,
        nanos: 0,
    };

    assert_wire_eq(
        &by_rpc,
        "SpawnAgentRequest",
        &proto::SpawnAgentRequest {
            agent_id: "agent-1".to_string(),
            workspace_id: "ws-1".to_string(),
            command: "forge".to_string(),
            args: vec!["run".to_string()],
            env: HashMap::new(),
            working_dir: "/tmp/repo".to_string(),
            session_name: "sess-1".to_string(),
            adapter: "codex".to_string(),
            resource_limits: None,
        },
    );

    assert_wire_eq(
        &by_rpc,
        "SpawnAgent",
        &proto::SpawnAgentResponse {
            agent: Some(proto::Agent {
                id: "agent-1".to_string(),
                workspace_id: "ws-1".to_string(),
                state: proto::AgentState::Idle as i32,
                pane_id: "sess:0.1".to_string(),
                pid: 1234,
                command: "forge".to_string(),
                adapter: "codex".to_string(),
                spawned_at: Some(base_ts),
                last_activity_at: Some(base_ts),
                content_hash: String::new(),
                resource_limits: None,
                resource_usage: None,
            }),
            pane_id: "sess:0.1".to_string(),
        },
    );

    assert_wire_eq(
        &by_rpc,
        "KillAgentRequest",
        &proto::KillAgentRequest {
            agent_id: "agent-1".to_string(),
            force: true,
            grace_period: None,
        },
    );

    assert_wire_eq(
        &by_rpc,
        "KillAgent",
        &proto::KillAgentResponse { success: true },
    );

    assert_wire_eq(
        &by_rpc,
        "SendInputRequest",
        &proto::SendInputRequest {
            agent_id: "agent-1".to_string(),
            text: "status".to_string(),
            send_enter: true,
            keys: vec!["C-c".to_string()],
        },
    );

    assert_wire_eq(
        &by_rpc,
        "SendInput",
        &proto::SendInputResponse { success: true },
    );

    assert_wire_eq(
        &by_rpc,
        "StartLoopRunnerRequest",
        &proto::StartLoopRunnerRequest {
            loop_id: "loop-1".to_string(),
            config_path: "/tmp/loop.yaml".to_string(),
            command_path: "forge".to_string(),
        },
    );

    assert_wire_eq(
        &by_rpc,
        "StartLoopRunner",
        &proto::StartLoopRunnerResponse {
            runner: Some(proto::LoopRunner {
                loop_id: "loop-1".to_string(),
                instance_id: "inst-1".to_string(),
                config_path: String::new(),
                command_path: String::new(),
                pid: 4242,
                state: proto::LoopRunnerState::Running as i32,
                last_error: String::new(),
                started_at: Some(base_ts),
                stopped_at: None,
            }),
        },
    );

    assert_wire_eq(
        &by_rpc,
        "StopLoopRunnerRequest",
        &proto::StopLoopRunnerRequest {
            loop_id: "loop-1".to_string(),
            force: true,
        },
    );

    assert_wire_eq(
        &by_rpc,
        "StopLoopRunner",
        &proto::StopLoopRunnerResponse {
            success: true,
            runner: Some(proto::LoopRunner {
                loop_id: "loop-1".to_string(),
                instance_id: "inst-1".to_string(),
                config_path: String::new(),
                command_path: String::new(),
                pid: 0,
                state: proto::LoopRunnerState::Stopped as i32,
                last_error: String::new(),
                started_at: Some(base_ts),
                stopped_at: Some(prost_types::Timestamp {
                    seconds: 1_770_653_400,
                    nanos: 0,
                }),
            }),
        },
    );

    assert_wire_eq(&by_rpc, "GetStatusRequest", &proto::GetStatusRequest {});

    assert_wire_eq(
        &by_rpc,
        "GetStatus",
        &proto::GetStatusResponse {
            status: Some(proto::DaemonStatus {
                version: "v0.0.1".to_string(),
                hostname: "node-a".to_string(),
                started_at: Some(base_ts),
                uptime: Some(prost_types::Duration {
                    seconds: 7200,
                    nanos: 0,
                }),
                agent_count: 2,
                resources: None,
                health: Some(proto::HealthStatus {
                    health: proto::Health::Healthy as i32,
                    checks: vec![],
                }),
            }),
        },
    );

    assert_wire_eq(&by_rpc, "PingRequest", &proto::PingRequest {});

    assert_wire_eq(
        &by_rpc,
        "Ping",
        &proto::PingResponse {
            timestamp: Some(base_ts),
            version: "v0.0.1".to_string(),
        },
    );
}

#[test]
fn rust_can_decode_go_wire_fixtures() {
    let summary = load_go_proto_wire_summary();
    let by_rpc: HashMap<String, String> = summary
        .fixtures
        .into_iter()
        .map(|f| (f.rpc, f.wire_hex))
        .collect();

    decode_roundtrip::<proto::SpawnAgentRequest>(&by_rpc, "SpawnAgentRequest");
    decode_roundtrip::<proto::SpawnAgentResponse>(&by_rpc, "SpawnAgent");
    decode_roundtrip::<proto::KillAgentRequest>(&by_rpc, "KillAgentRequest");
    decode_roundtrip::<proto::KillAgentResponse>(&by_rpc, "KillAgent");
    decode_roundtrip::<proto::SendInputRequest>(&by_rpc, "SendInputRequest");
    decode_roundtrip::<proto::SendInputResponse>(&by_rpc, "SendInput");
    decode_roundtrip::<proto::StartLoopRunnerRequest>(&by_rpc, "StartLoopRunnerRequest");
    decode_roundtrip::<proto::StartLoopRunnerResponse>(&by_rpc, "StartLoopRunner");
    decode_roundtrip::<proto::StopLoopRunnerRequest>(&by_rpc, "StopLoopRunnerRequest");
    decode_roundtrip::<proto::StopLoopRunnerResponse>(&by_rpc, "StopLoopRunner");
    decode_roundtrip::<proto::GetStatusRequest>(&by_rpc, "GetStatusRequest");
    decode_roundtrip::<proto::GetStatusResponse>(&by_rpc, "GetStatus");
    decode_roundtrip::<proto::PingRequest>(&by_rpc, "PingRequest");
    decode_roundtrip::<proto::PingResponse>(&by_rpc, "Ping");
}

fn load_go_proto_wire_summary() -> ProtoWireSummary {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../../old/go/internal/parity/testdata/oracle/expected/forged/proto-wire/summary.json",
    );
    let body = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read proto-wire summary {}: {err}", path.display()));
    serde_json::from_str(&body)
        .unwrap_or_else(|err| panic!("parse proto-wire summary {}: {err}", path.display()))
}

fn assert_wire_eq<M: Message>(fixtures: &HashMap<String, String>, rpc: &str, msg: &M) {
    let want = fixtures
        .get(rpc)
        .unwrap_or_else(|| panic!("missing fixture for rpc {rpc}"));
    let got = encode_hex(&msg.encode_to_vec());
    assert_eq!(got, *want, "wire mismatch for rpc {rpc}");
}

fn decode_roundtrip<M: Message + Default>(fixtures: &HashMap<String, String>, rpc: &str) {
    let wire_hex = fixtures
        .get(rpc)
        .unwrap_or_else(|| panic!("missing fixture for rpc {rpc}"));
    let bytes = decode_hex(wire_hex);
    let msg = M::decode(bytes.as_slice())
        .unwrap_or_else(|err| panic!("decode fixture for rpc {rpc}: {err}"));
    let reencoded = msg.encode_to_vec();
    assert_eq!(
        encode_hex(&reencoded),
        *wire_hex,
        "roundtrip mismatch for rpc {rpc}"
    );
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn decode_hex(input: &str) -> Vec<u8> {
    assert!(input.len() % 2 == 0, "invalid hex length");
    let mut out = Vec::with_capacity(input.len() / 2);
    let bytes = input.as_bytes();
    for i in (0..bytes.len()).step_by(2) {
        let hi = from_hex_nibble(bytes[i]);
        let lo = from_hex_nibble(bytes[i + 1]);
        out.push((hi << 4) | lo);
    }
    out
}

fn from_hex_nibble(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => panic!("invalid hex digit: {}", b as char),
    }
}
