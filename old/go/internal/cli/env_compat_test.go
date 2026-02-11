package cli

import (
	"os"
	"testing"
)

func TestMailConfigFromEnv_IgnoresLegacySwarmVars(t *testing.T) {
	t.Setenv("FORGE_AGENT_MAIL_URL", "")
	t.Setenv("FORGE_AGENT_MAIL_PROJECT", "")
	t.Setenv("FORGE_AGENT_MAIL_AGENT", "")
	t.Setenv("FORGE_AGENT_MAIL_LIMIT", "")
	t.Setenv("FORGE_AGENT_MAIL_TIMEOUT", "")

	t.Setenv("SWARM_AGENT_MAIL_URL", "http://legacy-mail.invalid")
	t.Setenv("SWARM_AGENT_MAIL_PROJECT", "legacy-project")
	t.Setenv("SWARM_AGENT_MAIL_AGENT", "legacy-agent")
	t.Setenv("SWARM_AGENT_MAIL_LIMIT", "99")
	t.Setenv("SWARM_AGENT_MAIL_TIMEOUT", "15s")

	cfg := mailConfigFromEnv()
	if cfg.URL != "" || cfg.Project != "" || cfg.Agent != "" || cfg.Limit != 0 || cfg.Timeout != 0 {
		t.Fatalf("expected legacy SWARM_* vars to be ignored, got %+v", cfg)
	}
}

func TestAgentMailConfigFromEnv_IgnoresLegacySwarmVars(t *testing.T) {
	t.Setenv("FORGE_AGENT_MAIL_URL", "")
	t.Setenv("FORGE_AGENT_MAIL_PROJECT", "")
	t.Setenv("FORGE_AGENT_MAIL_AGENT", "")
	t.Setenv("FORGE_AGENT_MAIL_TIMEOUT", "")

	t.Setenv("SWARM_AGENT_MAIL_URL", "http://legacy-mail.invalid")
	t.Setenv("SWARM_AGENT_MAIL_PROJECT", "legacy-project")
	t.Setenv("SWARM_AGENT_MAIL_AGENT", "legacy-agent")
	t.Setenv("SWARM_AGENT_MAIL_TIMEOUT", "15s")

	cfg := agentMailConfigFromEnv()
	if cfg.URL != "" || cfg.Project != "" || cfg.Agent != "" {
		t.Fatalf("expected legacy SWARM_* vars to be ignored, got %+v", cfg)
	}
	if cfg.Timeout != defaultAgentMailTimeout {
		t.Fatalf("expected default timeout %s, got %s", defaultAgentMailTimeout, cfg.Timeout)
	}
}

func TestProgressEnabled_IgnoresLegacySwarmNoProgress(t *testing.T) {
	restoreNoProgress := noProgress
	restoreJSON := jsonOutput
	restoreJSONL := jsonlOutput
	noProgress = false
	jsonOutput = false
	jsonlOutput = false
	t.Cleanup(func() {
		noProgress = restoreNoProgress
		jsonOutput = restoreJSON
		jsonlOutput = restoreJSONL
	})

	unsetEnv(t, "FORGE_NO_PROGRESS")
	unsetEnv(t, "NO_PROGRESS")
	t.Setenv("SWARM_NO_PROGRESS", "1")

	if !progressEnabled() {
		t.Fatal("expected progress to stay enabled when only SWARM_NO_PROGRESS is set")
	}
}

func unsetEnv(t *testing.T, key string) {
	t.Helper()
	prev, had := os.LookupEnv(key)
	if err := os.Unsetenv(key); err != nil {
		t.Fatalf("unset %s: %v", key, err)
	}
	t.Cleanup(func() {
		if had {
			_ = os.Setenv(key, prev)
			return
		}
		_ = os.Unsetenv(key)
	})
}
