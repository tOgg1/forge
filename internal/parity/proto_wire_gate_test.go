package parity

import (
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"slices"
	"testing"
	"time"

	forgedv1 "github.com/tOgg1/forge/gen/forged/v1"
	"google.golang.org/protobuf/encoding/protojson"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/known/durationpb"
	"google.golang.org/protobuf/types/known/timestamppb"
)

type protoWireFixture struct {
	RPC      string `json:"rpc"`
	Message  string `json:"message"`
	WireHex  string `json:"wire_hex"`
	JSONForm string `json:"json_form"`
}

type protoWireSummary struct {
	Fixtures []protoWireFixture `json:"fixtures"`
}

func TestProtoWireGateBaseline(t *testing.T) {
	t.Parallel()

	expected := filepath.Join("testdata", "oracle", "expected", "forged", "proto-wire")
	actual := filepath.Join("testdata", "oracle", "actual", "forged", "proto-wire")

	report, err := CompareTrees(expected, actual)
	if err != nil {
		t.Fatalf("compare proto wire baseline trees: %v", err)
	}
	if report.HasDrift() {
		t.Fatalf("proto wire gate drift detected: %+v", report)
	}
}

func TestProtoWireGateCriticalRPCFixtures(t *testing.T) {
	t.Parallel()

	summary := buildProtoWireSummary(t)
	got, err := json.MarshalIndent(summary, "", "  ")
	if err != nil {
		t.Fatalf("marshal proto wire summary: %v", err)
	}
	got = append(got, '\n')

	root := repoRoot(t)
	expectedPath := filepath.Join(root, "internal/parity/testdata/oracle/expected/forged/proto-wire/summary.json")
	actualPath := filepath.Join(root, "internal/parity/testdata/oracle/actual/forged/proto-wire/summary.json")

	if os.Getenv("FORGE_UPDATE_GOLDENS") == "1" {
		if err := os.MkdirAll(filepath.Dir(expectedPath), 0o755); err != nil {
			t.Fatalf("mkdir expected fixture dir: %v", err)
		}
		if err := os.MkdirAll(filepath.Dir(actualPath), 0o755); err != nil {
			t.Fatalf("mkdir actual fixture dir: %v", err)
		}
		if err := os.WriteFile(expectedPath, got, 0o644); err != nil {
			t.Fatalf("write expected fixture: %v", err)
		}
		if err := os.WriteFile(actualPath, got, 0o644); err != nil {
			t.Fatalf("write actual fixture: %v", err)
		}
	}

	want := readFile(t, expectedPath)
	if string(normalize(got)) != string(normalize([]byte(want))) {
		t.Fatalf("proto wire fixture drift; regenerate with FORGE_UPDATE_GOLDENS=1 go test ./internal/parity -run '^TestProtoWireGateCriticalRPCFixtures$' -count=1")
	}

	requiredRPCs := []string{
		"SpawnAgentRequest",
		"SpawnAgent",
		"KillAgentRequest",
		"KillAgent",
		"SendInputRequest",
		"SendInput",
		"StartLoopRunnerRequest",
		"StartLoopRunner",
		"StopLoopRunnerRequest",
		"StopLoopRunner",
		"GetStatusRequest",
		"GetStatus",
		"PingRequest",
		"Ping",
	}
	seen := make([]string, 0, len(summary.Fixtures))
	for _, fx := range summary.Fixtures {
		seen = append(seen, fx.RPC)
		if fx.JSONForm == "" {
			t.Fatalf("fixture %s has empty JSON form", fx.RPC)
		}
	}
	for _, rpc := range requiredRPCs {
		if !slices.Contains(seen, rpc) {
			t.Fatalf("missing required proto wire fixture for rpc %s", rpc)
		}
	}
}

func buildProtoWireSummary(t *testing.T) protoWireSummary {
	t.Helper()

	baseTS := timestamppb.New(time.Date(2026, 2, 9, 16, 0, 0, 0, time.UTC))

	fixtures := []protoWireFixture{
		marshalFixture(t, "SpawnAgentRequest", &forgedv1.SpawnAgentRequest{
			AgentId:     "agent-1",
			WorkspaceId: "ws-1",
			Command:     "forge",
			Args:        []string{"run"},
			WorkingDir:  "/tmp/repo",
			SessionName: "sess-1",
			Adapter:     "codex",
		}),
		marshalFixture(t, "SpawnAgent", &forgedv1.SpawnAgentResponse{
			Agent: &forgedv1.Agent{
				Id:             "agent-1",
				WorkspaceId:    "ws-1",
				State:          forgedv1.AgentState_AGENT_STATE_IDLE,
				PaneId:         "sess:0.1",
				Pid:            1234,
				Command:        "forge",
				Adapter:        "codex",
				SpawnedAt:      baseTS,
				LastActivityAt: baseTS,
			},
			PaneId: "sess:0.1",
		}),
		marshalFixture(t, "KillAgentRequest", &forgedv1.KillAgentRequest{
			AgentId: "agent-1",
			Force:   true,
		}),
		marshalFixture(t, "KillAgent", &forgedv1.KillAgentResponse{Success: true}),
		marshalFixture(t, "SendInputRequest", &forgedv1.SendInputRequest{
			AgentId:   "agent-1",
			Text:      "status",
			SendEnter: true,
			Keys:      []string{"C-c"},
		}),
		marshalFixture(t, "SendInput", &forgedv1.SendInputResponse{Success: true}),
		marshalFixture(t, "StartLoopRunnerRequest", &forgedv1.StartLoopRunnerRequest{
			LoopId:      "loop-1",
			ConfigPath:  "/tmp/loop.yaml",
			CommandPath: "forge",
		}),
		marshalFixture(t, "StartLoopRunner", &forgedv1.StartLoopRunnerResponse{
			Runner: &forgedv1.LoopRunner{
				LoopId:     "loop-1",
				InstanceId: "inst-1",
				Pid:        4242,
				State:      forgedv1.LoopRunnerState_LOOP_RUNNER_STATE_RUNNING,
				StartedAt:  baseTS,
			},
		}),
		marshalFixture(t, "StopLoopRunnerRequest", &forgedv1.StopLoopRunnerRequest{
			LoopId: "loop-1",
			Force:  true,
		}),
		marshalFixture(t, "StopLoopRunner", &forgedv1.StopLoopRunnerResponse{
			Success: true,
			Runner: &forgedv1.LoopRunner{
				LoopId:     "loop-1",
				InstanceId: "inst-1",
				Pid:        0,
				State:      forgedv1.LoopRunnerState_LOOP_RUNNER_STATE_STOPPED,
				StartedAt:  baseTS,
				StoppedAt:  timestamppb.New(baseTS.AsTime().Add(10 * time.Minute)),
			},
		}),
		marshalFixture(t, "GetStatusRequest", &forgedv1.GetStatusRequest{}),
		marshalFixture(t, "GetStatus", &forgedv1.GetStatusResponse{
			Status: &forgedv1.DaemonStatus{
				Version:    "v0.0.1",
				Hostname:   "node-a",
				StartedAt:  baseTS,
				Uptime:     durationpb.New(2 * time.Hour),
				AgentCount: 2,
				Health: &forgedv1.HealthStatus{
					Health: forgedv1.Health_HEALTH_HEALTHY,
				},
			},
		}),
		marshalFixture(t, "PingRequest", &forgedv1.PingRequest{}),
		marshalFixture(t, "Ping", &forgedv1.PingResponse{
			Timestamp: baseTS,
			Version:   "v0.0.1",
		}),
	}

	return protoWireSummary{Fixtures: fixtures}
}

func marshalFixture(t *testing.T, rpc string, msg proto.Message) protoWireFixture {
	t.Helper()

	wire, err := proto.MarshalOptions{Deterministic: true}.Marshal(msg)
	if err != nil {
		t.Fatalf("marshal %s: %v", rpc, err)
	}
	jsonBody, err := protojsonMarshal(msg)
	if err != nil {
		t.Fatalf("marshal %s json: %v", rpc, err)
	}

	return protoWireFixture{
		RPC:      rpc,
		Message:  string(msg.ProtoReflect().Descriptor().FullName()),
		WireHex:  hex.EncodeToString(wire),
		JSONForm: jsonBody,
	}
}

func protojsonMarshal(msg proto.Message) (string, error) {
	body, err := (&protojson.MarshalOptions{
		UseProtoNames:   true,
		EmitUnpopulated: true,
		Multiline:       false,
		Indent:          "",
	}).Marshal(msg)
	if err != nil {
		return "", err
	}
	return string(body), nil
}
