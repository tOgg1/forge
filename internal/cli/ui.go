// Package cli provides TUI launch commands.
package cli

import (
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/opencode-ai/swarm/internal/tui"
	"github.com/spf13/cobra"
	"golang.org/x/term"
)

func init() {
	rootCmd.AddCommand(uiCmd)
}

var uiCmd = &cobra.Command{
	Use:   "ui",
	Short: "Launch the Swarm TUI",
	Long:  "Launch the Swarm terminal user interface (TUI).",
	RunE: func(cmd *cobra.Command, args []string) error {
		return runTUI()
	},
}

func runTUI() error {
	if IsNonInteractive() {
		return &PreflightError{
			Message:  "TUI requires an interactive terminal",
			Hint:     "Run without --non-interactive and with a TTY, or use CLI subcommands",
			NextStep: "swarm --help",
		}
	}

	// Build TUI config from app config
	tuiConfig := tui.Config{}
	if cfg := GetConfig(); cfg != nil {
		tuiConfig.Theme = cfg.TUI.Theme
	}
	tuiConfig.AgentMail = agentMailConfigFromEnv()

	return tui.RunWithConfig(tuiConfig)
}

func hasTTY() bool {
	return term.IsTerminal(int(os.Stdin.Fd())) && term.IsTerminal(int(os.Stdout.Fd()))
}

func agentMailConfigFromEnv() tui.AgentMailConfig {
	cfg := tui.AgentMailConfig{
		URL:     strings.TrimSpace(os.Getenv("SWARM_AGENT_MAIL_URL")),
		Project: strings.TrimSpace(os.Getenv("SWARM_AGENT_MAIL_PROJECT")),
		Agent:   strings.TrimSpace(os.Getenv("SWARM_AGENT_MAIL_AGENT")),
	}

	if value := strings.TrimSpace(os.Getenv("SWARM_AGENT_MAIL_LIMIT")); value != "" {
		if limit, err := strconv.Atoi(value); err == nil && limit > 0 {
			cfg.Limit = limit
		}
	}

	if value := strings.TrimSpace(os.Getenv("SWARM_AGENT_MAIL_POLL_INTERVAL")); value != "" {
		if parsed, ok := parseEnvDuration(value); ok {
			cfg.PollInterval = parsed
		}
	}

	if value := strings.TrimSpace(os.Getenv("SWARM_AGENT_MAIL_TIMEOUT")); value != "" {
		if parsed, ok := parseEnvDuration(value); ok {
			cfg.Timeout = parsed
		}
	}

	return cfg
}

func parseEnvDuration(value string) (time.Duration, bool) {
	if value == "" {
		return 0, false
	}
	if parsed, err := time.ParseDuration(value); err == nil {
		return parsed, true
	}
	if seconds, err := strconv.Atoi(value); err == nil {
		return time.Duration(seconds) * time.Second, true
	}
	return 0, false
}
