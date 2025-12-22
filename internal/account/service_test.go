package account

import (
	"os"
	"path/filepath"
	"testing"
)

func TestResolveCredential_EnvPrefix(t *testing.T) {
	t.Setenv("TEST_KEY", "value")

	got, err := ResolveCredential("env:TEST_KEY")
	if err != nil {
		t.Fatalf("expected no error, got %v", err)
	}
	if got != "value" {
		t.Fatalf("expected value, got %q", got)
	}
}

func TestResolveCredential_DollarVar(t *testing.T) {
	t.Setenv("TEST_KEY", "value")

	got, err := ResolveCredential("$TEST_KEY")
	if err != nil {
		t.Fatalf("expected no error, got %v", err)
	}
	if got != "value" {
		t.Fatalf("expected value, got %q", got)
	}
}

func TestResolveCredential_DollarVarBraced(t *testing.T) {
	t.Setenv("TEST_KEY", "value")

	got, err := ResolveCredential("${TEST_KEY}")
	if err != nil {
		t.Fatalf("expected no error, got %v", err)
	}
	if got != "value" {
		t.Fatalf("expected value, got %q", got)
	}
}

func TestResolveCredential_File(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "token.txt")
	if err := os.WriteFile(path, []byte("secret\n"), 0600); err != nil {
		t.Fatalf("failed to write temp file: %v", err)
	}

	got, err := ResolveCredential("file:" + path)
	if err != nil {
		t.Fatalf("expected no error, got %v", err)
	}
	if got != "secret" {
		t.Fatalf("expected secret, got %q", got)
	}
}

func TestResolveCredential_Literal(t *testing.T) {
	got, err := ResolveCredential("literal")
	if err != nil {
		t.Fatalf("expected no error, got %v", err)
	}
	if got != "literal" {
		t.Fatalf("expected literal, got %q", got)
	}
}
