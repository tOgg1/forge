package tmux

import (
	"context"
	"errors"
	"fmt"
	"strconv"
	"strings"
	"testing"
)

type fakeExecutor struct {
	stdout      []byte
	stderr      []byte
	err         error
	stdoutQueue [][]byte
	stderrQueue [][]byte
	errQueue    []error
	lastCmd     string
	commands    []string
}

func (f *fakeExecutor) Exec(ctx context.Context, cmd string) ([]byte, []byte, error) {
	f.lastCmd = cmd
	f.commands = append(f.commands, cmd)

	stdout := f.stdout
	stderr := f.stderr
	err := f.err

	if len(f.stdoutQueue) > 0 {
		stdout = f.stdoutQueue[0]
		f.stdoutQueue = f.stdoutQueue[1:]
	}
	if len(f.stderrQueue) > 0 {
		stderr = f.stderrQueue[0]
		f.stderrQueue = f.stderrQueue[1:]
	}
	if len(f.errQueue) > 0 {
		err = f.errQueue[0]
		f.errQueue = f.errQueue[1:]
	}

	return stdout, stderr, err
}

func TestListSessions(t *testing.T) {
	exec := &fakeExecutor{stdout: []byte("alpha|2\nbeta|1\n")}
	client := NewClient(exec)

	sessions, err := client.ListSessions(context.Background())
	if err != nil {
		t.Fatalf("ListSessions failed: %v", err)
	}
	if exec.lastCmd == "" {
		t.Fatalf("expected command to be executed")
	}
	if len(sessions) != 2 {
		t.Fatalf("expected 2 sessions, got %d", len(sessions))
	}
	if sessions[0].Name != "alpha" || sessions[0].WindowCount != 2 {
		t.Fatalf("unexpected first session: %+v", sessions[0])
	}
}

func TestListSessions_NoServer(t *testing.T) {
	exec := &fakeExecutor{
		err:    errors.New("exit status 1"),
		stderr: []byte("no server running on /tmp/tmux-1000/default"),
	}
	client := NewClient(exec)

	sessions, err := client.ListSessions(context.Background())
	if err != nil {
		t.Fatalf("expected no error, got %v", err)
	}
	if len(sessions) != 0 {
		t.Fatalf("expected no sessions, got %d", len(sessions))
	}
}

func TestListSessions_InvalidOutput(t *testing.T) {
	exec := &fakeExecutor{stdout: []byte("bad-output")}
	client := NewClient(exec)

	_, err := client.ListSessions(context.Background())
	if err == nil {
		t.Fatalf("expected error for invalid output")
	}
}

func TestListPanePaths(t *testing.T) {
	exec := &fakeExecutor{stdout: []byte("%1|/repo/one\n%2|/repo/two\n%3|/repo/one\n")}
	client := NewClient(exec)

	paths, err := client.ListPanePaths(context.Background(), "session-1")
	if err != nil {
		t.Fatalf("ListPanePaths failed: %v", err)
	}
	if len(paths) != 2 {
		t.Fatalf("expected 2 unique paths, got %d", len(paths))
	}
	if paths[0] != "/repo/one" || paths[1] != "/repo/two" {
		t.Fatalf("unexpected paths order: %#v", paths)
	}
}

func TestListPanePaths_NoServer(t *testing.T) {
	exec := &fakeExecutor{
		err:    errors.New("exit status 1"),
		stderr: []byte("no server running on /tmp/tmux-1000/default"),
	}
	client := NewClient(exec)

	paths, err := client.ListPanePaths(context.Background(), "session-1")
	if err != nil {
		t.Fatalf("expected no error, got %v", err)
	}
	if len(paths) != 0 {
		t.Fatalf("expected no paths, got %d", len(paths))
	}
}

func TestNewTmuxClient(t *testing.T) {
	exec := &fakeExecutor{stdout: []byte("alpha|1\n")}
	client := NewTmuxClient(exec)

	sessions, err := client.ListSessions(context.Background())
	if err != nil {
		t.Fatalf("ListSessions failed: %v", err)
	}
	if len(sessions) != 1 || sessions[0].Name != "alpha" {
		t.Fatalf("unexpected sessions: %#v", sessions)
	}
}

func TestHasSession(t *testing.T) {
	tests := []struct {
		name    string
		stdout  []byte
		stderr  []byte
		err     error
		want    bool
		wantErr bool
	}{
		{
			name:   "session exists",
			stdout: []byte{},
			want:   true,
		},
		{
			name:   "session not found",
			stderr: []byte("session not found"),
			err:    errors.New("exit status 1"),
			want:   false,
		},
		{
			name:   "no server running",
			stderr: []byte("no server running"),
			err:    errors.New("exit status 1"),
			want:   false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			exec := &fakeExecutor{stdout: tt.stdout, stderr: tt.stderr, err: tt.err}
			client := NewClient(exec)

			got, err := client.HasSession(context.Background(), "test-session")
			if tt.wantErr && err == nil {
				t.Fatal("expected error")
			}
			if !tt.wantErr && err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if got != tt.want {
				t.Errorf("got %v, want %v", got, tt.want)
			}
		})
	}
}

func TestHasSession_EmptyName(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	_, err := client.HasSession(context.Background(), "")
	if err == nil {
		t.Fatal("expected error for empty session name")
	}
}

func TestNewSession(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	err := client.NewSession(context.Background(), "my-session", "/home/user/project")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if exec.lastCmd == "" {
		t.Fatal("expected command to be executed")
	}
	if !containsAll(exec.lastCmd, "new-session", "-d", "-s", "my-session") {
		t.Errorf("unexpected command: %s", exec.lastCmd)
	}
}

func TestNewSession_NoWorkDir(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	err := client.NewSession(context.Background(), "my-session", "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if !containsAll(exec.lastCmd, "new-session", "-d", "-s") {
		t.Errorf("unexpected command: %s", exec.lastCmd)
	}
}

func TestNewSession_Duplicate(t *testing.T) {
	exec := &fakeExecutor{
		stderr: []byte("duplicate session: my-session"),
		err:    errors.New("exit status 1"),
	}
	client := NewClient(exec)

	err := client.NewSession(context.Background(), "my-session", "")
	if err != ErrSessionExists {
		t.Errorf("expected ErrSessionExists, got %v", err)
	}
}

func TestNewSession_EmptyName(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	err := client.NewSession(context.Background(), "", "/some/path")
	if err == nil {
		t.Fatal("expected error for empty session name")
	}
}

func TestNewWindow(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	err := client.NewWindow(context.Background(), "my-session", "agents", "/home/user/project")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if exec.lastCmd == "" {
		t.Fatal("expected command to be executed")
	}
	if !containsAll(exec.lastCmd, "new-window", "-t", "my-session", "-n", "agents") {
		t.Errorf("unexpected command: %s", exec.lastCmd)
	}
}

func TestNewWindow_EmptySession(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	err := client.NewWindow(context.Background(), "", "agents", "/some/path")
	if err == nil {
		t.Fatal("expected error for empty session name")
	}
}

func TestSelectWindow(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	err := client.SelectWindow(context.Background(), "my-session:agents")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if !containsAll(exec.lastCmd, "select-window", "-t") {
		t.Errorf("unexpected command: %s", exec.lastCmd)
	}
}

func TestSelectWindow_EmptyTarget(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	err := client.SelectWindow(context.Background(), "")
	if err == nil {
		t.Fatal("expected error for empty target")
	}
}

func TestSelectLayout(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	err := client.SelectLayout(context.Background(), "my-session:agents", "tiled")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if !containsAll(exec.lastCmd, "select-layout", "-t", "my-session:agents", "tiled") {
		t.Errorf("unexpected command: %s", exec.lastCmd)
	}
}

func TestSelectLayout_EmptyTarget(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	err := client.SelectLayout(context.Background(), "", "tiled")
	if err == nil {
		t.Fatal("expected error for empty target")
	}
}

func TestSelectLayout_EmptyLayout(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	err := client.SelectLayout(context.Background(), "my-session:agents", "")
	if err == nil {
		t.Fatal("expected error for empty layout")
	}
}

func TestKillSession(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	err := client.KillSession(context.Background(), "my-session")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if !containsAll(exec.lastCmd, "kill-session", "-t", "my-session") {
		t.Errorf("unexpected command: %s", exec.lastCmd)
	}
}

func TestKillSession_NotFound(t *testing.T) {
	exec := &fakeExecutor{
		stderr: []byte("can't find session: my-session"),
		err:    errors.New("exit status 1"),
	}
	client := NewClient(exec)

	err := client.KillSession(context.Background(), "my-session")
	if err != ErrSessionNotFound {
		t.Errorf("expected ErrSessionNotFound, got %v", err)
	}
}

func TestListPanes(t *testing.T) {
	exec := &fakeExecutor{stdout: []byte("%1|0|0|/home/user/project|1|bash\n%2|0|1|/home/user/project|0|opencode\n")}
	client := NewClient(exec)

	panes, err := client.ListPanes(context.Background(), "my-session")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(panes) != 2 {
		t.Fatalf("expected 2 panes, got %d", len(panes))
	}
	if panes[0].ID != "%1" || panes[0].WindowIndex != 0 || panes[0].Index != 0 || !panes[0].Active {
		t.Errorf("unexpected first pane: %+v", panes[0])
	}
	if panes[0].Command != "bash" {
		t.Errorf("expected first pane command bash, got %q", panes[0].Command)
	}
	if panes[1].ID != "%2" || panes[1].WindowIndex != 0 || panes[1].Index != 1 || panes[1].Active {
		t.Errorf("unexpected second pane: %+v", panes[1])
	}
	if panes[1].Command != "opencode" {
		t.Errorf("expected second pane command opencode, got %q", panes[1].Command)
	}
}

func TestSplitWindow(t *testing.T) {
	exec := &fakeExecutor{stdout: []byte("%3\n")}
	client := NewClient(exec)

	paneID, err := client.SplitWindow(context.Background(), "my-session:0", true, "/home/user/project")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if paneID != "%3" {
		t.Errorf("expected pane ID '%%3', got %q", paneID)
	}
	if !containsAll(exec.lastCmd, "split-window", "-h", "-t", "-P", "-F") {
		t.Errorf("unexpected command: %s", exec.lastCmd)
	}
}

func TestSplitWindow_Vertical(t *testing.T) {
	exec := &fakeExecutor{stdout: []byte("%4\n")}
	client := NewClient(exec)

	_, err := client.SplitWindow(context.Background(), "my-session:0", false, "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !containsAll(exec.lastCmd, "-v") {
		t.Errorf("expected -v flag for vertical split: %s", exec.lastCmd)
	}
}

func TestSendKeys(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	err := client.SendKeys(context.Background(), "%1", "echo hello", true, false)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !containsAll(exec.lastCmd, "send-keys", "-t", "-l") {
		t.Errorf("unexpected command: %s", exec.lastCmd)
	}
}

func TestSendKeys_WithEnter(t *testing.T) {
	callCount := 0
	client := NewClient(&multiCallExecutor{calls: &callCount})

	err := client.SendKeys(context.Background(), "%1", "echo hello", false, true)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if callCount != 2 {
		t.Errorf("expected 2 command calls (send-keys + Enter), got %d", callCount)
	}
}

func TestSendInterrupt(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	err := client.SendInterrupt(context.Background(), "%1")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !containsAll(exec.lastCmd, "send-keys", "-t", "C-c") {
		t.Errorf("unexpected command: %s", exec.lastCmd)
	}
}

type multiCallExecutor struct {
	calls *int
}

func (e *multiCallExecutor) Exec(ctx context.Context, cmd string) ([]byte, []byte, error) {
	*e.calls++
	return nil, nil, nil
}

type sequenceExecutor struct {
	outputs [][]byte
	index   int
	lastCmd string
}

func (e *sequenceExecutor) Exec(ctx context.Context, cmd string) ([]byte, []byte, error) {
	e.lastCmd = cmd
	if e.index >= len(e.outputs) {
		return nil, nil, nil
	}
	out := e.outputs[e.index]
	e.index++
	return out, nil, nil
}

func TestSendAndWait(t *testing.T) {
	exec := &sequenceExecutor{
		outputs: [][]byte{
			[]byte("busy\n"),
			[]byte("still busy\n"),
			[]byte("done\n"),
			[]byte("done\n"),
			[]byte("done\n"),
		},
	}
	client := NewClient(exec)

	content, err := client.SendAndWait(context.Background(), "%1", "echo hello", true, true, 2)
	if err != nil {
		t.Fatalf("SendAndWait failed: %v", err)
	}
	if content != "done\n" {
		t.Fatalf("unexpected content: %q", content)
	}
}

func TestCapturePane(t *testing.T) {
	exec := &fakeExecutor{stdout: []byte("line 1\nline 2\nline 3\n")}
	client := NewClient(exec)

	content, err := client.CapturePane(context.Background(), "%1", false)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if content != "line 1\nline 2\nline 3\n" {
		t.Errorf("unexpected content: %q", content)
	}
	if containsAll(exec.lastCmd, "-S -") {
		t.Errorf("should not have history flag: %s", exec.lastCmd)
	}
}

func TestCapturePane_WithHistory(t *testing.T) {
	exec := &fakeExecutor{
		stdoutQueue: [][]byte{
			[]byte("10"),
			[]byte("history content"),
		},
	}
	client := NewClient(exec)

	_, err := client.CapturePane(context.Background(), "%1", true)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(exec.commands) < 2 {
		t.Fatalf("expected multiple tmux commands, got %d", len(exec.commands))
	}
	if !containsAll(exec.commands[0], "display-message", "history_size") {
		t.Errorf("expected history size lookup, got: %s", exec.commands[0])
	}
	if !containsAll(exec.commands[1], "-S -") {
		t.Errorf("expected history flag -S -: %s", exec.commands[1])
	}
}

func TestCapturePane_WithHistoryChunked(t *testing.T) {
	historySize := historyChunkLines + 250
	exec := &fakeExecutor{
		stdoutQueue: [][]byte{
			[]byte(strconv.Itoa(historySize)),
			[]byte("chunk1\n"),
			[]byte("chunk2\n"),
			[]byte("visible\n"),
		},
	}
	client := NewClient(exec)

	content, err := client.CapturePane(context.Background(), "%1", true)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if content != "chunk1\nchunk2\nvisible\n" {
		t.Fatalf("unexpected content: %q", content)
	}
	if len(exec.commands) != 4 {
		t.Fatalf("expected 4 commands, got %d", len(exec.commands))
	}

	start := -historySize
	firstEnd := start + historyChunkLines - 1
	secondStart := start + historyChunkLines

	if !containsAll(exec.commands[1], fmt.Sprintf("-S %d", start), fmt.Sprintf("-E %d", firstEnd)) {
		t.Errorf("unexpected first history chunk: %s", exec.commands[1])
	}
	if !containsAll(exec.commands[2], fmt.Sprintf("-S %d", secondStart), "-E -1") {
		t.Errorf("unexpected second history chunk: %s", exec.commands[2])
	}
	if !containsAll(exec.commands[3], "-S 0", "-E -") {
		t.Errorf("unexpected visible chunk: %s", exec.commands[3])
	}
}

func TestCapturePane_WithHistoryFallbackLimit(t *testing.T) {
	exec := &fakeExecutor{
		stdoutQueue: [][]byte{
			[]byte(""),
			[]byte("history content"),
		},
		errQueue: []error{
			errors.New("history size error"),
			nil,
		},
	}
	client := NewClient(exec)

	content, err := client.CapturePane(context.Background(), "%1", true)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if content != "history content" {
		t.Fatalf("unexpected content: %q", content)
	}
	if len(exec.commands) != 2 {
		t.Fatalf("expected 2 commands, got %d", len(exec.commands))
	}
	if !containsAll(exec.commands[1], fmt.Sprintf("-S -%d", historyMaxLines)) {
		t.Errorf("unexpected history fallback command: %s", exec.commands[1])
	}
}

func TestKillPane(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	err := client.KillPane(context.Background(), "%1")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !containsAll(exec.lastCmd, "kill-pane", "-t") {
		t.Errorf("unexpected command: %s", exec.lastCmd)
	}
}

func TestSelectPane(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	err := client.SelectPane(context.Background(), "%1")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !containsAll(exec.lastCmd, "select-pane", "-t") {
		t.Errorf("unexpected command: %s", exec.lastCmd)
	}
}

func TestEscapeArg(t *testing.T) {
	tests := []struct {
		input string
		want  string
	}{
		{"simple", "'simple'"},
		{"with spaces", "'with spaces'"},
		{"with'quote", "'with'\\''quote'"},
		{"$variable", "'$variable'"},
	}

	for _, tt := range tests {
		t.Run(tt.input, func(t *testing.T) {
			got := escapeArg(tt.input)
			if got != tt.want {
				t.Errorf("escapeArg(%q) = %q, want %q", tt.input, got, tt.want)
			}
		})
	}
}

// -------------------------
// HistorySize tests
// -------------------------

func TestHistorySize(t *testing.T) {
	exec := &fakeExecutor{stdout: []byte("150\n")}
	client := NewClient(exec)

	size, err := client.HistorySize(context.Background(), "%1")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if size != 150 {
		t.Errorf("expected 150, got %d", size)
	}
	if !containsAll(exec.lastCmd, "display-message", "-p", "history_size") {
		t.Errorf("unexpected command: %s", exec.lastCmd)
	}
}

func TestHistorySize_EmptyTarget(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	_, err := client.HistorySize(context.Background(), "")
	if err == nil {
		t.Fatal("expected error for empty target")
	}
}

func TestHistorySize_EmptyOutput(t *testing.T) {
	exec := &fakeExecutor{stdout: []byte("")}
	client := NewClient(exec)

	_, err := client.HistorySize(context.Background(), "%1")
	if err == nil {
		t.Fatal("expected error for empty output")
	}
}

func TestHistorySize_InvalidOutput(t *testing.T) {
	exec := &fakeExecutor{stdout: []byte("not-a-number\n")}
	client := NewClient(exec)

	_, err := client.HistorySize(context.Background(), "%1")
	if err == nil {
		t.Fatal("expected error for invalid output")
	}
}

func TestHistorySize_NegativeClampedToZero(t *testing.T) {
	exec := &fakeExecutor{stdout: []byte("-5\n")}
	client := NewClient(exec)

	size, err := client.HistorySize(context.Background(), "%1")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if size != 0 {
		t.Errorf("expected 0 (clamped from negative), got %d", size)
	}
}

func TestHistorySize_ExecError(t *testing.T) {
	exec := &fakeExecutor{err: errors.New("tmux failed")}
	client := NewClient(exec)

	_, err := client.HistorySize(context.Background(), "%1")
	if err == nil {
		t.Fatal("expected error from exec failure")
	}
}

// -------------------------
// escapeSessionName tests
// -------------------------

func TestEscapeSessionName(t *testing.T) {
	tests := []struct {
		input string
		want  string
	}{
		{"simple", "simple"},
		{"with-dash", "with-dash"},
		{"with_underscore", "with_underscore"},
		{"with spaces", "'with spaces'"},
		{"with'quote", "'with'\\''quote'"},
		{"with\ttab", "'with\ttab'"},
		{"with\nnewline", "'with\nnewline'"},
		{"with\"doublequote", "'with\"doublequote'"},
		{"with\\backslash", "'with\\backslash'"},
		{"with$dollar", "'with$dollar'"},
		{"with`backtick", "'with`backtick'"},
		{"with!bang", "'with!bang'"},
	}

	for _, tt := range tests {
		t.Run(tt.input, func(t *testing.T) {
			got := escapeSessionName(tt.input)
			if got != tt.want {
				t.Errorf("escapeSessionName(%q) = %q, want %q", tt.input, got, tt.want)
			}
		})
	}
}

// -------------------------
// Edge case tests for empty targets and error handling
// -------------------------

func TestKillSession_EmptySession(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	err := client.KillSession(context.Background(), "")
	if err == nil {
		t.Fatal("expected error for empty session name")
	}
}

func TestListPanes_EmptySession(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	_, err := client.ListPanes(context.Background(), "")
	if err == nil {
		t.Fatal("expected error for empty session name")
	}
}

func TestListPanes_NoServer(t *testing.T) {
	exec := &fakeExecutor{
		err:    errors.New("exit status 1"),
		stderr: []byte("no server running on /tmp/tmux-1000/default"),
	}
	client := NewClient(exec)

	panes, err := client.ListPanes(context.Background(), "my-session")
	if err != nil {
		t.Fatalf("expected no error, got %v", err)
	}
	if len(panes) != 0 {
		t.Fatalf("expected no panes, got %d", len(panes))
	}
}

func TestListPanes_InvalidOutput(t *testing.T) {
	exec := &fakeExecutor{stdout: []byte("invalid-format")}
	client := NewClient(exec)

	_, err := client.ListPanes(context.Background(), "my-session")
	if err == nil {
		t.Fatal("expected error for invalid output")
	}
}

func TestSplitWindow_EmptyTarget(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	_, err := client.SplitWindow(context.Background(), "", true, "")
	if err == nil {
		t.Fatal("expected error for empty target")
	}
}

func TestSendKeys_EmptyTarget(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	err := client.SendKeys(context.Background(), "", "echo hello", true, false)
	if err == nil {
		t.Fatal("expected error for empty target")
	}
}

func TestSendInterrupt_EmptyTarget(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	err := client.SendInterrupt(context.Background(), "")
	if err == nil {
		t.Fatal("expected error for empty target")
	}
}

func TestCapturePane_EmptyTarget(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	_, err := client.CapturePane(context.Background(), "", false)
	if err == nil {
		t.Fatal("expected error for empty target")
	}
}

func TestKillPane_EmptyTarget(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	err := client.KillPane(context.Background(), "")
	if err == nil {
		t.Fatal("expected error for empty target")
	}
}

func TestSelectPane_EmptyTarget(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	err := client.SelectPane(context.Background(), "")
	if err == nil {
		t.Fatal("expected error for empty target")
	}
}

func TestListPanePaths_EmptySession(t *testing.T) {
	exec := &fakeExecutor{}
	client := NewClient(exec)

	_, err := client.ListPanePaths(context.Background(), "")
	if err == nil {
		t.Fatal("expected error for empty session name")
	}
}

func TestListPanePaths_InvalidOutput(t *testing.T) {
	exec := &fakeExecutor{stdout: []byte("invalid-format-no-pipe")}
	client := NewClient(exec)

	_, err := client.ListPanePaths(context.Background(), "my-session")
	if err == nil {
		t.Fatal("expected error for invalid output")
	}
}

// -------------------------
// Golden output tests for parsing
// -------------------------

func TestListSessions_GoldenOutput(t *testing.T) {
	// Tests various realistic tmux output formats
	tests := []struct {
		name     string
		stdout   string
		expected []Session
		wantErr  bool
	}{
		{
			name:     "empty output",
			stdout:   "",
			expected: []Session{},
		},
		{
			name:     "single session",
			stdout:   "mysession|3\n",
			expected: []Session{{Name: "mysession", WindowCount: 3}},
		},
		{
			name:   "multiple sessions",
			stdout: "dev|2\nprod|1\ntest|5\n",
			expected: []Session{
				{Name: "dev", WindowCount: 2},
				{Name: "prod", WindowCount: 1},
				{Name: "test", WindowCount: 5},
			},
		},
		{
			name:     "session with special characters in name",
			stdout:   "my-session_v2|1\n",
			expected: []Session{{Name: "my-session_v2", WindowCount: 1}},
		},
		{
			name:    "malformed output - missing delimiter",
			stdout:  "noseparator\n",
			wantErr: true,
		},
		{
			name:    "malformed output - non-numeric window count",
			stdout:  "session|abc\n",
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			exec := &fakeExecutor{stdout: []byte(tt.stdout)}
			client := NewClient(exec)

			sessions, err := client.ListSessions(context.Background())
			if tt.wantErr {
				if err == nil {
					t.Fatal("expected error")
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if len(sessions) != len(tt.expected) {
				t.Fatalf("expected %d sessions, got %d", len(tt.expected), len(sessions))
			}
			for i, exp := range tt.expected {
				if sessions[i].Name != exp.Name || sessions[i].WindowCount != exp.WindowCount {
					t.Errorf("session[%d] = %+v, want %+v", i, sessions[i], exp)
				}
			}
		})
	}
}

func TestListPanes_GoldenOutput(t *testing.T) {
	tests := []struct {
		name     string
		stdout   string
		expected []Pane
		wantErr  bool
	}{
		{
			name:     "empty output",
			stdout:   "",
			expected: []Pane{},
		},
		{
			name:   "single pane",
			stdout: "%0|0|0|/home/user|1|zsh\n",
			expected: []Pane{
				{ID: "%0", WindowIndex: 0, Index: 0, CurrentDir: "/home/user", Active: true, Command: "zsh"},
			},
		},
		{
			name:   "multiple panes across windows",
			stdout: "%0|0|0|/home/user|1|zsh\n%1|0|1|/home/user/code|0|vim\n%2|1|0|/tmp|0|bash\n",
			expected: []Pane{
				{ID: "%0", WindowIndex: 0, Index: 0, CurrentDir: "/home/user", Active: true, Command: "zsh"},
				{ID: "%1", WindowIndex: 0, Index: 1, CurrentDir: "/home/user/code", Active: false, Command: "vim"},
				{ID: "%2", WindowIndex: 1, Index: 0, CurrentDir: "/tmp", Active: false, Command: "bash"},
			},
		},
		{
			name:   "path with spaces",
			stdout: "%0|0|0|/home/user/My Documents|0|bash\n",
			expected: []Pane{
				{ID: "%0", WindowIndex: 0, Index: 0, CurrentDir: "/home/user/My Documents", Active: false, Command: "bash"},
			},
		},
		{
			name:    "malformed output - missing fields",
			stdout:  "%0|0|0\n",
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			exec := &fakeExecutor{stdout: []byte(tt.stdout)}
			client := NewClient(exec)

			panes, err := client.ListPanes(context.Background(), "my-session")
			if tt.wantErr {
				if err == nil {
					t.Fatal("expected error")
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if len(panes) != len(tt.expected) {
				t.Fatalf("expected %d panes, got %d", len(tt.expected), len(panes))
			}
			for i, exp := range tt.expected {
				if panes[i].ID != exp.ID || panes[i].WindowIndex != exp.WindowIndex ||
					panes[i].Index != exp.Index || panes[i].CurrentDir != exp.CurrentDir ||
					panes[i].Active != exp.Active || panes[i].Command != exp.Command {
					t.Errorf("pane[%d] = %+v, want %+v", i, panes[i], exp)
				}
			}
		})
	}
}

func TestListPanePaths_GoldenOutput(t *testing.T) {
	tests := []struct {
		name     string
		stdout   string
		expected []string
	}{
		{
			name:     "empty output",
			stdout:   "",
			expected: []string{},
		},
		{
			name:     "single path",
			stdout:   "%0|/home/user\n",
			expected: []string{"/home/user"},
		},
		{
			name:     "multiple unique paths",
			stdout:   "%0|/home/user\n%1|/tmp\n%2|/var/log\n",
			expected: []string{"/home/user", "/tmp", "/var/log"},
		},
		{
			name:     "duplicate paths deduplicated",
			stdout:   "%0|/home/user\n%1|/home/user\n%2|/tmp\n",
			expected: []string{"/home/user", "/tmp"},
		},
		{
			name:     "empty path skipped",
			stdout:   "%0|/home/user\n%1|\n%2|/tmp\n",
			expected: []string{"/home/user", "/tmp"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			exec := &fakeExecutor{stdout: []byte(tt.stdout)}
			client := NewClient(exec)

			paths, err := client.ListPanePaths(context.Background(), "my-session")
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if len(paths) != len(tt.expected) {
				t.Fatalf("expected %d paths, got %d: %v", len(tt.expected), len(paths), paths)
			}
			for i, exp := range tt.expected {
				if paths[i] != exp {
					t.Errorf("path[%d] = %q, want %q", i, paths[i], exp)
				}
			}
		})
	}
}

// -------------------------
// NewLocalClient test
// -------------------------

func TestNewLocalClient(t *testing.T) {
	client := NewLocalClient()
	if client == nil {
		t.Fatal("NewLocalClient returned nil")
	}
	if client.exec == nil {
		t.Fatal("expected exec to be set")
	}
	_, ok := client.exec.(*LocalExecutor)
	if !ok {
		t.Fatal("expected LocalExecutor type")
	}
}

// -------------------------
// KillSession_NoServer test
// -------------------------

func TestKillSession_NoServer(t *testing.T) {
	exec := &fakeExecutor{
		err:    errors.New("exit status 1"),
		stderr: []byte("no server running on /tmp/tmux-1000/default"),
	}
	client := NewClient(exec)

	err := client.KillSession(context.Background(), "my-session")
	if err != ErrSessionNotFound {
		t.Errorf("expected ErrSessionNotFound, got %v", err)
	}
}

// -------------------------
// Additional SplitWindow tests
// -------------------------

func TestSplitWindow_ExecError(t *testing.T) {
	exec := &fakeExecutor{err: errors.New("split failed")}
	client := NewClient(exec)

	_, err := client.SplitWindow(context.Background(), "%1", true, "")
	if err == nil {
		t.Fatal("expected error")
	}
}

// -------------------------
// SendKeys with error tests
// -------------------------

func TestSendKeys_ExecError(t *testing.T) {
	exec := &fakeExecutor{err: errors.New("send failed")}
	client := NewClient(exec)

	err := client.SendKeys(context.Background(), "%1", "echo hello", true, false)
	if err == nil {
		t.Fatal("expected error")
	}
}

func TestSendKeys_EnterExecError(t *testing.T) {
	exec := &fakeExecutor{
		errQueue: []error{nil, errors.New("enter failed")},
	}
	client := NewClient(exec)

	err := client.SendKeys(context.Background(), "%1", "echo hello", false, true)
	if err == nil {
		t.Fatal("expected error on Enter key send")
	}
}

// -------------------------
// SendInterrupt error test
// -------------------------

func TestSendInterrupt_ExecError(t *testing.T) {
	exec := &fakeExecutor{err: errors.New("interrupt failed")}
	client := NewClient(exec)

	err := client.SendInterrupt(context.Background(), "%1")
	if err == nil {
		t.Fatal("expected error")
	}
}

// -------------------------
// KillPane error test
// -------------------------

func TestKillPane_ExecError(t *testing.T) {
	exec := &fakeExecutor{err: errors.New("kill failed")}
	client := NewClient(exec)

	err := client.KillPane(context.Background(), "%1")
	if err == nil {
		t.Fatal("expected error")
	}
}

// -------------------------
// SelectPane error test
// -------------------------

func TestSelectPane_ExecError(t *testing.T) {
	exec := &fakeExecutor{err: errors.New("select failed")}
	client := NewClient(exec)

	err := client.SelectPane(context.Background(), "%1")
	if err == nil {
		t.Fatal("expected error")
	}
}

// -------------------------
// NewWindow error test
// -------------------------

func TestNewWindow_ExecError(t *testing.T) {
	exec := &fakeExecutor{err: errors.New("new window failed")}
	client := NewClient(exec)

	err := client.NewWindow(context.Background(), "session", "name", "/path")
	if err == nil {
		t.Fatal("expected error")
	}
}

// -------------------------
// SelectWindow error test
// -------------------------

func TestSelectWindow_ExecError(t *testing.T) {
	exec := &fakeExecutor{err: errors.New("select window failed")}
	client := NewClient(exec)

	err := client.SelectWindow(context.Background(), "target")
	if err == nil {
		t.Fatal("expected error")
	}
}

// -------------------------
// SelectLayout error test
// -------------------------

func TestSelectLayout_ExecError(t *testing.T) {
	exec := &fakeExecutor{err: errors.New("select layout failed")}
	client := NewClient(exec)

	err := client.SelectLayout(context.Background(), "target", "tiled")
	if err == nil {
		t.Fatal("expected error")
	}
}

// -------------------------
// CapturePane error tests
// -------------------------

func TestCapturePane_ExecError(t *testing.T) {
	exec := &fakeExecutor{err: errors.New("capture failed")}
	client := NewClient(exec)

	_, err := client.CapturePane(context.Background(), "%1", false)
	if err == nil {
		t.Fatal("expected error")
	}
}

// -------------------------
// ListSessions error test
// -------------------------

func TestListSessions_ExecError(t *testing.T) {
	exec := &fakeExecutor{
		err:    errors.New("exit status 1"),
		stderr: []byte("some other error"),
	}
	client := NewClient(exec)

	_, err := client.ListSessions(context.Background())
	if err == nil {
		t.Fatal("expected error")
	}
}

// -------------------------
// ListPanePaths error test
// -------------------------

func TestListPanePaths_ExecError(t *testing.T) {
	exec := &fakeExecutor{
		err:    errors.New("exit status 1"),
		stderr: []byte("some other error"),
	}
	client := NewClient(exec)

	_, err := client.ListPanePaths(context.Background(), "session")
	if err == nil {
		t.Fatal("expected error")
	}
}

// -------------------------
// ListPanes error test
// -------------------------

func TestListPanes_ExecError(t *testing.T) {
	exec := &fakeExecutor{
		err:    errors.New("exit status 1"),
		stderr: []byte("some other error"),
	}
	client := NewClient(exec)

	_, err := client.ListPanes(context.Background(), "session")
	if err == nil {
		t.Fatal("expected error")
	}
}

// -------------------------
// HasSession error test
// -------------------------

func TestHasSession_OtherError(t *testing.T) {
	exec := &fakeExecutor{
		err:    errors.New("exit status 1"),
		stderr: []byte("some unexpected error"),
	}
	client := NewClient(exec)

	_, err := client.HasSession(context.Background(), "test-session")
	if err == nil {
		t.Fatal("expected error")
	}
}

// -------------------------
// NewSession error test
// -------------------------

func TestNewSession_OtherError(t *testing.T) {
	exec := &fakeExecutor{
		err:    errors.New("exit status 1"),
		stderr: []byte("some unexpected error"),
	}
	client := NewClient(exec)

	err := client.NewSession(context.Background(), "session", "")
	if err == nil {
		t.Fatal("expected error")
	}
	if errors.Is(err, ErrSessionExists) {
		t.Fatal("should not be ErrSessionExists")
	}
}

// -------------------------
// KillSession other error test
// -------------------------

func TestKillSession_OtherError(t *testing.T) {
	exec := &fakeExecutor{
		err:    errors.New("exit status 1"),
		stderr: []byte("some unexpected error"),
	}
	client := NewClient(exec)

	err := client.KillSession(context.Background(), "session")
	if err == nil {
		t.Fatal("expected error")
	}
	if errors.Is(err, ErrSessionNotFound) {
		t.Fatal("should not be ErrSessionNotFound")
	}
}

// Helper function for tests
func containsAll(s string, substrings ...string) bool {
	for _, sub := range substrings {
		if !strings.Contains(s, sub) {
			return false
		}
	}
	return true
}
