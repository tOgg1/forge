package ssh

import "testing"

func TestBuildSSHArgs_ControlMasterOptions(t *testing.T) {
	options := ConnectionOptions{
		Host:           "example.com",
		User:           "deploy",
		ControlMaster:  "auto",
		ControlPath:    "/tmp/ssh-%r@%h:%p",
		ControlPersist: "10m",
	}

	args, target := buildSSHArgs(options)
	if target != "deploy@example.com" {
		t.Fatalf("expected target deploy@example.com, got %q", target)
	}

	assertFlagValue(t, args, "-o", "ControlMaster=auto")
	assertFlagValue(t, args, "-o", "ControlPath=/tmp/ssh-%r@%h:%p")
	assertFlagValue(t, args, "-o", "ControlPersist=10m")
}

func TestBuildSSHArgs_OmitsEmptyControlOptions(t *testing.T) {
	options := ConnectionOptions{
		Host: "example.com",
	}

	args, _ := buildSSHArgs(options)
	if hasFlagValue(args, "-o", "ControlMaster=") || hasFlagValue(args, "-o", "ControlPath=") || hasFlagValue(args, "-o", "ControlPersist=") {
		t.Fatalf("expected control options to be omitted, got args: %#v", args)
	}
}

func TestBuildSSHArgs_ProxyJump(t *testing.T) {
	options := ConnectionOptions{
		Host:      "example.com",
		ProxyJump: "jump.example.com",
	}

	args, _ := buildSSHArgs(options)
	assertFlagValue(t, args, "-J", "jump.example.com")
}

func assertFlagValue(t *testing.T, args []string, flag, value string) {
	t.Helper()
	if !hasFlagValue(args, flag, value) {
		t.Fatalf("expected %s %q in args: %#v", flag, value, args)
	}
}

func hasFlagValue(args []string, flag, value string) bool {
	for i := 0; i < len(args)-1; i++ {
		if args[i] == flag && args[i+1] == value {
			return true
		}
	}
	return false
}
