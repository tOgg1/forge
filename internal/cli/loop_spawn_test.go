package cli

import (
	"bytes"
	"errors"
	"strings"
	"testing"
)

func TestParseLoopSpawnOwner(t *testing.T) {
	cases := []struct {
		input   string
		want    loopSpawnOwner
		wantErr bool
	}{
		{input: "", want: loopSpawnOwnerAuto},
		{input: "auto", want: loopSpawnOwnerAuto},
		{input: "local", want: loopSpawnOwnerLocal},
		{input: "daemon", want: loopSpawnOwnerDaemon},
		{input: "DaEmOn", want: loopSpawnOwnerDaemon},
		{input: "invalid", wantErr: true},
	}

	for _, tc := range cases {
		got, err := parseLoopSpawnOwner(tc.input)
		if tc.wantErr {
			if err == nil {
				t.Fatalf("parseLoopSpawnOwner(%q): expected error", tc.input)
			}
			continue
		}
		if err != nil {
			t.Fatalf("parseLoopSpawnOwner(%q): %v", tc.input, err)
		}
		if got != tc.want {
			t.Fatalf("parseLoopSpawnOwner(%q): got %q, want %q", tc.input, got, tc.want)
		}
	}
}

func TestStartLoopRunnerAutoFallsBackToLocal(t *testing.T) {
	originalDaemon := startLoopProcessDaemonFunc
	originalLocal := startLoopProcessLocalFunc
	originalWarn := spawnWarningWriter
	originalQuiet := quiet
	originalJSON := jsonOutput
	originalJSONL := jsonlOutput
	defer func() {
		startLoopProcessDaemonFunc = originalDaemon
		startLoopProcessLocalFunc = originalLocal
		spawnWarningWriter = originalWarn
		quiet = originalQuiet
		jsonOutput = originalJSON
		jsonlOutput = originalJSONL
	}()

	quiet = false
	jsonOutput = false
	jsonlOutput = false

	startLoopProcessDaemonFunc = func(string, string) (string, error) {
		return "", errors.New("daemon down")
	}

	localCalls := 0
	startLoopProcessLocalFunc = func(string, string) error {
		localCalls++
		return nil
	}

	var warn bytes.Buffer
	spawnWarningWriter = &warn

	got, err := startLoopRunner("loop-1", "", loopSpawnOwnerAuto)
	if err != nil {
		t.Fatalf("startLoopRunner(auto): %v", err)
	}
	if got.Owner != loopSpawnOwnerLocal {
		t.Fatalf("owner = %q, want %q", got.Owner, loopSpawnOwnerLocal)
	}
	if localCalls != 1 {
		t.Fatalf("local fallback calls = %d, want 1", localCalls)
	}
	if !strings.Contains(warn.String(), "falling back to local spawn") {
		t.Fatalf("warning output missing fallback message: %q", warn.String())
	}
}

func TestStartLoopRunnerDaemonDoesNotFallback(t *testing.T) {
	originalDaemon := startLoopProcessDaemonFunc
	originalLocal := startLoopProcessLocalFunc
	defer func() {
		startLoopProcessDaemonFunc = originalDaemon
		startLoopProcessLocalFunc = originalLocal
	}()

	startLoopProcessDaemonFunc = func(string, string) (string, error) {
		return "", errors.New("daemon unavailable")
	}

	localCalled := false
	startLoopProcessLocalFunc = func(string, string) error {
		localCalled = true
		return nil
	}

	_, err := startLoopRunner("loop-2", "", loopSpawnOwnerDaemon)
	if err == nil {
		t.Fatalf("expected error for daemon owner when daemon unavailable")
	}
	if localCalled {
		t.Fatalf("local fallback should not run for daemon owner")
	}
}
