package parity

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestSchemaFingerprintBaseline(t *testing.T) {
	t.Parallel()

	fingerprint, err := ComputeSchemaFingerprint(context.Background())
	if err != nil {
		t.Fatalf("compute schema fingerprint: %v", err)
	}

	expectedDump := readFile(t, filepath.Join("testdata", "schema", "schema-fingerprint.txt"))
	expectedHash := strings.TrimSpace(readFile(t, filepath.Join("testdata", "schema", "schema-fingerprint.sha256")))

	if string(normalize([]byte(fingerprint.Dump))) != string(normalize([]byte(expectedDump))) {
		t.Fatalf("schema fingerprint dump drift; regenerate with: go run ./cmd/schema-fingerprint --out-dir internal/parity/testdata/schema")
	}
	if fingerprint.SHA256 != expectedHash {
		t.Fatalf("schema fingerprint hash drift: got %s want %s", fingerprint.SHA256, expectedHash)
	}
}

func TestSchemaFingerprintDeterministic(t *testing.T) {
	t.Parallel()

	first, err := ComputeSchemaFingerprint(context.Background())
	if err != nil {
		t.Fatalf("first compute: %v", err)
	}
	second, err := ComputeSchemaFingerprint(context.Background())
	if err != nil {
		t.Fatalf("second compute: %v", err)
	}

	if first.Dump != second.Dump {
		t.Fatal("schema fingerprint dump is not deterministic")
	}
	if first.SHA256 != second.SHA256 {
		t.Fatal("schema fingerprint hash is not deterministic")
	}
}

func readFile(t *testing.T, path string) string {
	t.Helper()
	body, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	return string(body)
}
