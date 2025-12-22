package ssh

import (
	"os"
	"path/filepath"
	"testing"
)

func TestApplySSHConfig_Basic(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("HOME", dir)

	configDir := filepath.Join(dir, ".ssh")
	if err := os.MkdirAll(configDir, 0700); err != nil {
		t.Fatalf("failed to create config dir: %v", err)
	}

	config := `
Host test-host
  HostName example.com
  User deploy
  Port 2222
  IdentityFile ~/.ssh/id_test
  ProxyJump jump.example.com
`
	configPath := filepath.Join(configDir, "config")
	if err := os.WriteFile(configPath, []byte(config), 0600); err != nil {
		t.Fatalf("failed to write config: %v", err)
	}

	opts := ConnectionOptions{Host: "test-host"}
	got, err := ApplySSHConfig(opts)
	if err != nil {
		t.Fatalf("ApplySSHConfig failed: %v", err)
	}

	if got.Host != "example.com" {
		t.Fatalf("expected host example.com, got %q", got.Host)
	}
	if got.User != "deploy" {
		t.Fatalf("expected user deploy, got %q", got.User)
	}
	if got.Port != 2222 {
		t.Fatalf("expected port 2222, got %d", got.Port)
	}

	expectedKey := filepath.Join(dir, ".ssh", "id_test")
	if got.KeyPath != expectedKey {
		t.Fatalf("expected key path %q, got %q", expectedKey, got.KeyPath)
	}
	if got.ProxyJump != "jump.example.com" {
		t.Fatalf("expected proxy jump jump.example.com, got %q", got.ProxyJump)
	}
}

func TestApplySSHConfig_DoesNotOverrideExplicit(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("HOME", dir)

	configDir := filepath.Join(dir, ".ssh")
	if err := os.MkdirAll(configDir, 0700); err != nil {
		t.Fatalf("failed to create config dir: %v", err)
	}

	config := `
Host example
  User configuser
  Port 2222
  IdentityFile ~/.ssh/id_config
  ProxyJump jump.example.com
`
	configPath := filepath.Join(configDir, "config")
	if err := os.WriteFile(configPath, []byte(config), 0600); err != nil {
		t.Fatalf("failed to write config: %v", err)
	}

	opts := ConnectionOptions{
		Host:      "example",
		User:      "explicit",
		Port:      2200,
		KeyPath:   "/tmp/key",
		ProxyJump: "explicit.jump",
	}

	got, err := ApplySSHConfig(opts)
	if err != nil {
		t.Fatalf("ApplySSHConfig failed: %v", err)
	}

	if got.User != "explicit" {
		t.Fatalf("expected user explicit, got %q", got.User)
	}
	if got.Port != 2200 {
		t.Fatalf("expected port 2200, got %d", got.Port)
	}
	if got.KeyPath != "/tmp/key" {
		t.Fatalf("expected key path /tmp/key, got %q", got.KeyPath)
	}
	if got.ProxyJump != "explicit.jump" {
		t.Fatalf("expected proxy jump explicit.jump, got %q", got.ProxyJump)
	}
}

func TestApplySSHConfig_PatternNegation(t *testing.T) {
	dir := t.TempDir()
	t.Setenv("HOME", dir)

	configDir := filepath.Join(dir, ".ssh")
	if err := os.MkdirAll(configDir, 0700); err != nil {
		t.Fatalf("failed to create config dir: %v", err)
	}

	config := `
Host !dev-bad dev-*
  User devuser
`
	configPath := filepath.Join(configDir, "config")
	if err := os.WriteFile(configPath, []byte(config), 0600); err != nil {
		t.Fatalf("failed to write config: %v", err)
	}

	okHost := ConnectionOptions{Host: "dev-box"}
	okGot, err := ApplySSHConfig(okHost)
	if err != nil {
		t.Fatalf("ApplySSHConfig failed: %v", err)
	}
	if okGot.User != "devuser" {
		t.Fatalf("expected user devuser, got %q", okGot.User)
	}

	blockedHost := ConnectionOptions{Host: "dev-bad"}
	blockedGot, err := ApplySSHConfig(blockedHost)
	if err != nil {
		t.Fatalf("ApplySSHConfig failed: %v", err)
	}
	if blockedGot.User != "" {
		t.Fatalf("expected user to remain empty, got %q", blockedGot.User)
	}
}
