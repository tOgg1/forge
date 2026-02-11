package parity

import (
	"context"
	"net"
	"os/exec"
	"testing"
	"time"

	"github.com/rs/zerolog"
	forgedv1 "github.com/tOgg1/forge/gen/forged/v1"
	"github.com/tOgg1/forge/internal/forged"
	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/status"
)

// startTestServer creates a Go gRPC server with the forged service and returns
// a connected client and cleanup function.
func startTestServer(t *testing.T) (forgedv1.ForgedServiceClient, func()) {
	t.Helper()

	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("listen: %v", err)
	}

	server := grpc.NewServer()
	forgedv1.RegisterForgedServiceServer(server, forged.NewServer(zerolog.Nop(), forged.WithVersion("interop-test")))
	go func() { _ = server.Serve(listener) }()

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	//nolint:staticcheck
	conn, err := grpc.DialContext(ctx, listener.Addr().String(),
		grpc.WithTransportCredentials(insecure.NewCredentials()),
		grpc.WithBlock(),
	)
	if err != nil {
		server.Stop()
		t.Fatalf("dial: %v", err)
	}

	client := forgedv1.NewForgedServiceClient(conn)
	cleanup := func() {
		conn.Close()
		server.Stop()
	}
	return client, cleanup
}

func hasTmux() bool {
	_, err := exec.LookPath("tmux")
	return err == nil
}

// TestDaemonInteropPingRoundTrip verifies Ping over gRPC.
func TestDaemonInteropPingRoundTrip(t *testing.T) {
	t.Parallel()
	client, cleanup := startTestServer(t)
	defer cleanup()

	resp, err := client.Ping(context.Background(), &forgedv1.PingRequest{})
	if err != nil {
		t.Fatalf("Ping: %v", err)
	}
	if resp.Version != "interop-test" {
		t.Errorf("version = %q, want %q", resp.Version, "interop-test")
	}
	if resp.Timestamp == nil {
		t.Error("expected timestamp")
	}
}

// TestDaemonInteropGetStatusRoundTrip verifies GetStatus over gRPC.
func TestDaemonInteropGetStatusRoundTrip(t *testing.T) {
	t.Parallel()
	client, cleanup := startTestServer(t)
	defer cleanup()

	resp, err := client.GetStatus(context.Background(), &forgedv1.GetStatusRequest{})
	if err != nil {
		t.Fatalf("GetStatus: %v", err)
	}
	st := resp.Status
	if st == nil {
		t.Fatal("expected status")
	}
	if st.Version != "interop-test" {
		t.Errorf("version = %q, want %q", st.Version, "interop-test")
	}
	if st.StartedAt == nil {
		t.Error("expected started_at")
	}
	if st.Uptime == nil {
		t.Error("expected uptime")
	}
	if st.Health == nil {
		t.Error("expected health")
	}
}

// TestDaemonInteropListAgentsEmpty verifies ListAgents returns empty on fresh server.
func TestDaemonInteropListAgentsEmpty(t *testing.T) {
	t.Parallel()
	client, cleanup := startTestServer(t)
	defer cleanup()

	resp, err := client.ListAgents(context.Background(), &forgedv1.ListAgentsRequest{})
	if err != nil {
		t.Fatalf("ListAgents: %v", err)
	}
	if len(resp.Agents) != 0 {
		t.Errorf("agents = %d, want 0", len(resp.Agents))
	}
}

// TestDaemonInteropGetAgentNotFound verifies NotFound error for missing agent.
func TestDaemonInteropGetAgentNotFound(t *testing.T) {
	t.Parallel()
	client, cleanup := startTestServer(t)
	defer cleanup()

	_, err := client.GetAgent(context.Background(), &forgedv1.GetAgentRequest{AgentId: "nonexistent"})
	if err == nil {
		t.Fatal("expected error for nonexistent agent")
	}
	if st, ok := status.FromError(err); ok {
		if st.Code() != codes.NotFound {
			t.Errorf("code = %v, want NotFound", st.Code())
		}
	}
}

// TestDaemonInteropSpawnAgentMissingID verifies InvalidArgument for empty agent_id.
func TestDaemonInteropSpawnAgentMissingID(t *testing.T) {
	t.Parallel()
	client, cleanup := startTestServer(t)
	defer cleanup()

	_, err := client.SpawnAgent(context.Background(), &forgedv1.SpawnAgentRequest{
		Command: "echo",
	})
	if err == nil {
		t.Fatal("expected error for missing agent_id")
	}
	if st, ok := status.FromError(err); ok {
		if st.Code() != codes.InvalidArgument {
			t.Errorf("code = %v, want InvalidArgument", st.Code())
		}
	}
}

// TestDaemonInteropKillAgentNotFound verifies NotFound for killing missing agent.
func TestDaemonInteropKillAgentNotFound(t *testing.T) {
	t.Parallel()
	client, cleanup := startTestServer(t)
	defer cleanup()

	_, err := client.KillAgent(context.Background(), &forgedv1.KillAgentRequest{
		AgentId: "nonexistent",
	})
	if err == nil {
		t.Fatal("expected error for nonexistent agent")
	}
	if st, ok := status.FromError(err); ok {
		if st.Code() != codes.NotFound {
			t.Errorf("code = %v, want NotFound", st.Code())
		}
	}
}

// TestDaemonInteropLoopRunnerLifecycle verifies Start/Stop/Get/List loop runner RPCs.
func TestDaemonInteropLoopRunnerLifecycle(t *testing.T) {
	t.Parallel()
	client, cleanup := startTestServer(t)
	defer cleanup()

	ctx := context.Background()

	// List (empty)
	listResp, err := client.ListLoopRunners(ctx, &forgedv1.ListLoopRunnersRequest{})
	if err != nil {
		t.Fatalf("ListLoopRunners: %v", err)
	}
	if len(listResp.Runners) != 0 {
		t.Errorf("runners = %d, want 0", len(listResp.Runners))
	}

	// Start
	startResp, err := client.StartLoopRunner(ctx, &forgedv1.StartLoopRunnerRequest{
		LoopId:      "loop-go-1",
		ConfigPath:  "/tmp/loop.yaml",
		CommandPath: "forge",
	})
	if err != nil {
		t.Fatalf("StartLoopRunner: %v", err)
	}
	runner := startResp.Runner
	if runner == nil {
		t.Fatal("expected runner")
	}
	if runner.LoopId != "loop-go-1" {
		t.Errorf("loop_id = %q, want %q", runner.LoopId, "loop-go-1")
	}
	if runner.State != forgedv1.LoopRunnerState_LOOP_RUNNER_STATE_RUNNING {
		t.Errorf("state = %v, want RUNNING", runner.State)
	}

	// Get
	getResp, err := client.GetLoopRunner(ctx, &forgedv1.GetLoopRunnerRequest{LoopId: "loop-go-1"})
	if err != nil {
		t.Fatalf("GetLoopRunner: %v", err)
	}
	if getResp.Runner == nil || getResp.Runner.LoopId != "loop-go-1" {
		t.Error("expected runner with loop_id loop-go-1")
	}

	// List (one)
	listResp, err = client.ListLoopRunners(ctx, &forgedv1.ListLoopRunnersRequest{})
	if err != nil {
		t.Fatalf("ListLoopRunners: %v", err)
	}
	if len(listResp.Runners) != 1 {
		t.Errorf("runners = %d, want 1", len(listResp.Runners))
	}

	// Stop
	stopResp, err := client.StopLoopRunner(ctx, &forgedv1.StopLoopRunnerRequest{
		LoopId: "loop-go-1",
		Force:  true,
	})
	if err != nil {
		t.Fatalf("StopLoopRunner: %v", err)
	}
	if !stopResp.Success {
		t.Error("expected success=true")
	}
	if stopResp.Runner.State != forgedv1.LoopRunnerState_LOOP_RUNNER_STATE_STOPPED {
		t.Errorf("state = %v, want STOPPED", stopResp.Runner.State)
	}
}

// TestDaemonInteropSpawnAndKillAgent requires tmux to spawn real agent panes.
func TestDaemonInteropSpawnAndKillAgent(t *testing.T) {
	if !hasTmux() {
		t.Skip("tmux not available")
	}
	t.Parallel()
	client, cleanup := startTestServer(t)
	defer cleanup()

	ctx := context.Background()

	spawnResp, err := client.SpawnAgent(ctx, &forgedv1.SpawnAgentRequest{
		AgentId:     "agent-go-interop",
		WorkspaceId: "ws-go",
		Command:     "echo",
		Args:        []string{"hello"},
		WorkingDir:  "/tmp",
		SessionName: "sess-go-interop",
		Adapter:     "test",
	})
	if err != nil {
		t.Fatalf("SpawnAgent: %v", err)
	}
	agent := spawnResp.Agent
	if agent == nil {
		t.Fatal("expected agent")
	}
	if agent.Id != "agent-go-interop" {
		t.Errorf("agent.Id = %q, want %q", agent.Id, "agent-go-interop")
	}
	if spawnResp.PaneId == "" {
		t.Error("expected non-empty pane_id")
	}

	killResp, err := client.KillAgent(ctx, &forgedv1.KillAgentRequest{
		AgentId: "agent-go-interop",
		Force:   true,
	})
	if err != nil {
		t.Fatalf("KillAgent: %v", err)
	}
	if !killResp.Success {
		t.Error("expected success=true")
	}
}

// TestDaemonInteropSendInput requires tmux for agent pane interaction.
func TestDaemonInteropSendInput(t *testing.T) {
	if !hasTmux() {
		t.Skip("tmux not available")
	}
	t.Parallel()
	client, cleanup := startTestServer(t)
	defer cleanup()

	ctx := context.Background()

	if _, err := client.SpawnAgent(ctx, &forgedv1.SpawnAgentRequest{
		AgentId:     "agent-input-go",
		WorkspaceId: "ws-input",
		Command:     "echo",
		WorkingDir:  "/tmp",
		SessionName: "sess-input-go",
		Adapter:     "test",
	}); err != nil {
		t.Fatalf("SpawnAgent: %v", err)
	}

	resp, err := client.SendInput(ctx, &forgedv1.SendInputRequest{
		AgentId:   "agent-input-go",
		Text:      "hello",
		SendEnter: true,
		Keys:      []string{"C-c"},
	})
	if err != nil {
		t.Fatalf("SendInput: %v", err)
	}
	if !resp.Success {
		t.Error("expected success=true")
	}
}

// TestDaemonInteropGetTranscript requires tmux for agent spawn.
func TestDaemonInteropGetTranscript(t *testing.T) {
	if !hasTmux() {
		t.Skip("tmux not available")
	}
	t.Parallel()
	client, cleanup := startTestServer(t)
	defer cleanup()

	ctx := context.Background()

	if _, err := client.SpawnAgent(ctx, &forgedv1.SpawnAgentRequest{
		AgentId:     "agent-transcript-go",
		WorkspaceId: "ws-transcript",
		Command:     "echo",
		Args:        []string{"hi"},
		WorkingDir:  "/tmp",
		SessionName: "sess-transcript-go",
		Adapter:     "test",
	}); err != nil {
		t.Fatalf("SpawnAgent: %v", err)
	}

	resp, err := client.GetTranscript(ctx, &forgedv1.GetTranscriptRequest{
		AgentId: "agent-transcript-go",
		Limit:   100,
	})
	if err != nil {
		t.Fatalf("GetTranscript: %v", err)
	}
	if resp.AgentId != "agent-transcript-go" {
		t.Errorf("agent_id = %q, want %q", resp.AgentId, "agent-transcript-go")
	}
	if len(resp.Entries) == 0 {
		t.Error("expected at least one transcript entry from spawn")
	}
}
