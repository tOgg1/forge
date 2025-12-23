package swarmd

import (
	"context"
	"testing"

	swarmdv1 "github.com/opencode-ai/swarm/gen/swarmd/v1"
	"github.com/rs/zerolog"
)

func TestServerPing(t *testing.T) {
	server := NewServer(zerolog.Nop(), WithVersion("test-version"))

	resp, err := server.Ping(context.Background(), &swarmdv1.PingRequest{})
	if err != nil {
		t.Fatalf("Ping() error = %v", err)
	}

	if resp.Version != "test-version" {
		t.Errorf("Version = %q, want %q", resp.Version, "test-version")
	}
	if resp.Timestamp == nil {
		t.Error("Timestamp should not be nil")
	}
}

func TestServerGetStatus(t *testing.T) {
	server := NewServer(zerolog.Nop(), WithVersion("test-version"))

	resp, err := server.GetStatus(context.Background(), &swarmdv1.GetStatusRequest{})
	if err != nil {
		t.Fatalf("GetStatus() error = %v", err)
	}

	if resp.Status == nil {
		t.Fatal("Status should not be nil")
	}
	if resp.Status.Version != "test-version" {
		t.Errorf("Version = %q, want %q", resp.Status.Version, "test-version")
	}
	if resp.Status.AgentCount != 0 {
		t.Errorf("AgentCount = %d, want 0", resp.Status.AgentCount)
	}
}

func TestServerListAgentsEmpty(t *testing.T) {
	server := NewServer(zerolog.Nop())

	resp, err := server.ListAgents(context.Background(), &swarmdv1.ListAgentsRequest{})
	if err != nil {
		t.Fatalf("ListAgents() error = %v", err)
	}

	if len(resp.Agents) != 0 {
		t.Errorf("Agents count = %d, want 0", len(resp.Agents))
	}
}

func TestServerGetAgentNotFound(t *testing.T) {
	server := NewServer(zerolog.Nop())

	_, err := server.GetAgent(context.Background(), &swarmdv1.GetAgentRequest{
		AgentId: "nonexistent",
	})
	if err == nil {
		t.Error("GetAgent() should return error for nonexistent agent")
	}
}

func TestServerSpawnAgentValidation(t *testing.T) {
	server := NewServer(zerolog.Nop())

	tests := []struct {
		name    string
		req     *swarmdv1.SpawnAgentRequest
		wantErr bool
	}{
		{
			name:    "empty agent_id",
			req:     &swarmdv1.SpawnAgentRequest{Command: "echo"},
			wantErr: true,
		},
		{
			name:    "empty command",
			req:     &swarmdv1.SpawnAgentRequest{AgentId: "test-agent"},
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := server.SpawnAgent(context.Background(), tt.req)
			if (err != nil) != tt.wantErr {
				t.Errorf("SpawnAgent() error = %v, wantErr %v", err, tt.wantErr)
			}
		})
	}
}

func TestServerKillAgentNotFound(t *testing.T) {
	server := NewServer(zerolog.Nop())

	_, err := server.KillAgent(context.Background(), &swarmdv1.KillAgentRequest{
		AgentId: "nonexistent",
	})
	if err == nil {
		t.Error("KillAgent() should return error for nonexistent agent")
	}
}

func TestServerSendInputNotFound(t *testing.T) {
	server := NewServer(zerolog.Nop())

	_, err := server.SendInput(context.Background(), &swarmdv1.SendInputRequest{
		AgentId: "nonexistent",
		Text:    "hello",
	})
	if err == nil {
		t.Error("SendInput() should return error for nonexistent agent")
	}
}

func TestServerCapturePaneNotFound(t *testing.T) {
	server := NewServer(zerolog.Nop())

	_, err := server.CapturePane(context.Background(), &swarmdv1.CapturePaneRequest{
		AgentId: "nonexistent",
	})
	if err == nil {
		t.Error("CapturePane() should return error for nonexistent agent")
	}
}

func TestDetectAgentState(t *testing.T) {
	server := NewServer(zerolog.Nop())

	tests := []struct {
		name    string
		content string
		want    swarmdv1.AgentState
	}{
		{
			name:    "waiting for approval with y/n",
			content: "Do you want to proceed? [y/n]",
			want:    swarmdv1.AgentState_AGENT_STATE_WAITING_APPROVAL,
		},
		{
			name:    "waiting for approval with confirm",
			content: "Please confirm this action",
			want:    swarmdv1.AgentState_AGENT_STATE_WAITING_APPROVAL,
		},
		{
			name:    "idle with prompt",
			content: "some output\n$",
			want:    swarmdv1.AgentState_AGENT_STATE_IDLE,
		},
		{
			name:    "idle with arrow prompt",
			content: "done\n❯",
			want:    swarmdv1.AgentState_AGENT_STATE_IDLE,
		},
		{
			name:    "running with spinner",
			content: "⠋ Processing...",
			want:    swarmdv1.AgentState_AGENT_STATE_RUNNING,
		},
		{
			name:    "running with thinking",
			content: "Thinking...",
			want:    swarmdv1.AgentState_AGENT_STATE_RUNNING,
		},
		{
			name:    "failed with error",
			content: "error: something went wrong",
			want:    swarmdv1.AgentState_AGENT_STATE_FAILED,
		},
		{
			name:    "failed with panic",
			content: "panic: runtime error",
			want:    swarmdv1.AgentState_AGENT_STATE_FAILED,
		},
		{
			name:    "default to running",
			content: "some random output",
			want:    swarmdv1.AgentState_AGENT_STATE_RUNNING,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := server.detectAgentState(tt.content, "")
			if got != tt.want {
				t.Errorf("detectAgentState() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestContainsAny(t *testing.T) {
	tests := []struct {
		name    string
		s       string
		substrs []string
		want    bool
	}{
		{
			name:    "contains first",
			s:       "hello world",
			substrs: []string{"hello", "foo"},
			want:    true,
		},
		{
			name:    "contains second",
			s:       "hello world",
			substrs: []string{"foo", "world"},
			want:    true,
		},
		{
			name:    "contains none",
			s:       "hello world",
			substrs: []string{"foo", "bar"},
			want:    false,
		},
		{
			name:    "empty string",
			s:       "",
			substrs: []string{"foo"},
			want:    false,
		},
		{
			name:    "empty substrs",
			s:       "hello",
			substrs: []string{},
			want:    false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := containsAny(tt.s, tt.substrs...)
			if got != tt.want {
				t.Errorf("containsAny() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestSplitLines(t *testing.T) {
	tests := []struct {
		name    string
		content string
		want    []string
	}{
		{
			name:    "single line",
			content: "hello",
			want:    []string{"hello"},
		},
		{
			name:    "multiple lines",
			content: "line1\nline2\nline3",
			want:    []string{"line1", "line2", "line3"},
		},
		{
			name:    "trailing newline",
			content: "line1\nline2\n",
			want:    []string{"line1", "line2"},
		},
		{
			name:    "empty string",
			content: "",
			want:    nil,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := splitLines(tt.content)
			if len(got) != len(tt.want) {
				t.Errorf("splitLines() len = %d, want %d", len(got), len(tt.want))
				return
			}
			for i := range got {
				if got[i] != tt.want[i] {
					t.Errorf("splitLines()[%d] = %q, want %q", i, got[i], tt.want[i])
				}
			}
		})
	}
}

// =============================================================================
// Transcript Tests
// =============================================================================

func TestServerGetTranscriptNotFound(t *testing.T) {
	server := NewServer(zerolog.Nop())

	_, err := server.GetTranscript(context.Background(), &swarmdv1.GetTranscriptRequest{
		AgentId: "nonexistent",
	})
	if err == nil {
		t.Error("GetTranscript() should return error for nonexistent agent")
	}
}

func TestServerGetTranscriptValidation(t *testing.T) {
	server := NewServer(zerolog.Nop())

	_, err := server.GetTranscript(context.Background(), &swarmdv1.GetTranscriptRequest{})
	if err == nil {
		t.Error("GetTranscript() should return error for empty agent_id")
	}
}

func TestAddTranscriptEntry(t *testing.T) {
	server := NewServer(zerolog.Nop())

	// Manually add an agent (bypass tmux)
	server.mu.Lock()
	server.agents["test-agent"] = &agentInfo{
		id:         "test-agent",
		transcript: make([]transcriptEntry, 0),
	}
	server.mu.Unlock()

	// Add a transcript entry
	server.addTranscriptEntry("test-agent", swarmdv1.TranscriptEntryType_TRANSCRIPT_ENTRY_TYPE_COMMAND, "echo hello", map[string]string{"key": "value"})

	// Verify the entry was added
	resp, err := server.GetTranscript(context.Background(), &swarmdv1.GetTranscriptRequest{
		AgentId: "test-agent",
	})
	if err != nil {
		t.Fatalf("GetTranscript() error = %v", err)
	}

	if len(resp.Entries) != 1 {
		t.Fatalf("Expected 1 entry, got %d", len(resp.Entries))
	}

	entry := resp.Entries[0]
	if entry.Content != "echo hello" {
		t.Errorf("Content = %q, want %q", entry.Content, "echo hello")
	}
	if entry.Type != swarmdv1.TranscriptEntryType_TRANSCRIPT_ENTRY_TYPE_COMMAND {
		t.Errorf("Type = %v, want %v", entry.Type, swarmdv1.TranscriptEntryType_TRANSCRIPT_ENTRY_TYPE_COMMAND)
	}
	if entry.Metadata["key"] != "value" {
		t.Errorf("Metadata[key] = %q, want %q", entry.Metadata["key"], "value")
	}
}

func TestGetTranscriptWithLimit(t *testing.T) {
	server := NewServer(zerolog.Nop())

	// Manually add an agent with multiple transcript entries
	server.mu.Lock()
	server.agents["test-agent"] = &agentInfo{
		id:         "test-agent",
		transcript: make([]transcriptEntry, 0),
	}
	server.mu.Unlock()

	// Add 10 entries
	for i := 0; i < 10; i++ {
		server.addTranscriptEntry("test-agent", swarmdv1.TranscriptEntryType_TRANSCRIPT_ENTRY_TYPE_OUTPUT, "output", nil)
	}

	// Get with limit of 5
	resp, err := server.GetTranscript(context.Background(), &swarmdv1.GetTranscriptRequest{
		AgentId: "test-agent",
		Limit:   5,
	})
	if err != nil {
		t.Fatalf("GetTranscript() error = %v", err)
	}

	if len(resp.Entries) != 5 {
		t.Errorf("Expected 5 entries, got %d", len(resp.Entries))
	}
	if !resp.HasMore {
		t.Error("Expected HasMore to be true")
	}
	if resp.NextCursor == "" {
		t.Error("Expected NextCursor to be set")
	}
}

func TestParseInt64(t *testing.T) {
	tests := []struct {
		name    string
		input   string
		want    int64
		wantErr bool
	}{
		{
			name:    "valid number",
			input:   "123",
			want:    123,
			wantErr: false,
		},
		{
			name:    "zero",
			input:   "0",
			want:    0,
			wantErr: false,
		},
		{
			name:    "large number",
			input:   "9999999999",
			want:    9999999999,
			wantErr: false,
		},
		{
			name:    "invalid character",
			input:   "123abc",
			want:    0,
			wantErr: true,
		},
		{
			name:    "negative sign",
			input:   "-123",
			want:    0,
			wantErr: true,
		},
		{
			name:    "empty string",
			input:   "",
			want:    0,
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := parseInt64(tt.input)
			if (err != nil) != tt.wantErr {
				t.Errorf("parseInt64() error = %v, wantErr %v", err, tt.wantErr)
				return
			}
			if !tt.wantErr && got != tt.want {
				t.Errorf("parseInt64() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestTranscriptEntryToProto(t *testing.T) {
	server := NewServer(zerolog.Nop())

	entry := &transcriptEntry{
		id:        42,
		entryType: swarmdv1.TranscriptEntryType_TRANSCRIPT_ENTRY_TYPE_USER_INPUT,
		content:   "user input text",
		metadata:  map[string]string{"source": "keyboard"},
	}

	proto := server.transcriptEntryToProto(entry)

	if proto.Content != "user input text" {
		t.Errorf("Content = %q, want %q", proto.Content, "user input text")
	}
	if proto.Type != swarmdv1.TranscriptEntryType_TRANSCRIPT_ENTRY_TYPE_USER_INPUT {
		t.Errorf("Type = %v, want %v", proto.Type, swarmdv1.TranscriptEntryType_TRANSCRIPT_ENTRY_TYPE_USER_INPUT)
	}
	if proto.Metadata["source"] != "keyboard" {
		t.Errorf("Metadata[source] = %q, want %q", proto.Metadata["source"], "keyboard")
	}
	if proto.Timestamp == nil {
		t.Error("Timestamp should not be nil")
	}
}
