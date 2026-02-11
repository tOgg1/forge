package cli

import "strings"

func agentMailConfigFromEnv() agentMailConfig {
	cfg := agentMailConfig{
		URL:     getEnvTrim("FORGE_AGENT_MAIL_URL"),
		Project: getEnvTrim("FORGE_AGENT_MAIL_PROJECT"),
		Agent:   getEnvTrim("FORGE_AGENT_MAIL_AGENT"),
	}

	if value := getEnvTrim("FORGE_AGENT_MAIL_TIMEOUT"); value != "" {
		if parsed, ok := parseEnvDuration(value); ok {
			cfg.Timeout = parsed
		}
	}

	cfg.Project = strings.TrimSpace(cfg.Project)
	cfg.Agent = strings.TrimSpace(cfg.Agent)
	cfg.URL = strings.TrimSpace(cfg.URL)
	if cfg.Timeout <= 0 {
		cfg.Timeout = defaultAgentMailTimeout
	}

	return cfg
}
