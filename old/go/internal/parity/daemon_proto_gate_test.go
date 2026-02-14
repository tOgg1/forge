package parity

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestDaemonProtoGateProtoSurfaceLocked(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	protoPath := filepath.Join(root, "proto/forged/v1/forged.proto")
	pbPath := filepath.Join(root, "gen/forged/v1/forged.pb.go")
	grpcPath := filepath.Join(root, "gen/forged/v1/forged_grpc.pb.go")

	protoBody := mustReadFile(t, protoPath)
	pbBody := mustReadFile(t, pbPath)
	grpcBody := mustReadFile(t, grpcPath)

	if !strings.Contains(protoBody, "service ForgedService") {
		t.Fatalf("proto gate drift: %s missing ForgedService service declaration", protoPath)
	}
	if !strings.Contains(pbBody, "package forgedv1") {
		t.Fatalf("proto gate drift: %s missing forgedv1 package", pbPath)
	}

	for _, rpc := range []string{
		"SpawnAgent",
		"KillAgent",
		"SendInput",
		"ListAgents",
		"GetAgent",
		"StartLoopRunner",
		"StopLoopRunner",
		"GetLoopRunner",
		"ListLoopRunners",
		"CapturePane",
		"StreamPaneUpdates",
		"StreamEvents",
		"GetTranscript",
		"StreamTranscript",
		"GetStatus",
		"Ping",
	} {
		if !strings.Contains(protoBody, "rpc "+rpc+"(") {
			t.Fatalf("proto gate drift: missing rpc %s in %s", rpc, protoPath)
		}
		if !strings.Contains(grpcBody, rpc+"(") {
			t.Fatalf("proto gate drift: missing generated grpc method %s in %s", rpc, grpcPath)
		}
	}
}

func mustReadFile(t *testing.T, path string) string {
	t.Helper()
	body, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	return string(body)
}

func repoRoot(t *testing.T) string {
	t.Helper()

	wd, err := os.Getwd()
	if err != nil {
		t.Fatalf("getwd: %v", err)
	}

	cur := wd
	for {
		if _, err := os.Stat(filepath.Join(cur, "go.mod")); err == nil {
			return cur
		}
		next := filepath.Dir(cur)
		if next == cur {
			t.Fatal("repo root with go.mod not found")
		}
		cur = next
	}
}

func workspaceRoot(t *testing.T) string {
	t.Helper()
	root := filepath.Clean(filepath.Join(repoRoot(t), "..", ".."))
	if _, err := os.Stat(filepath.Join(root, "docs")); err != nil {
		t.Fatalf("workspace root missing docs/: %v", err)
	}
	return root
}
