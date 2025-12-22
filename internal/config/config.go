// Package config handles Swarm configuration loading and validation.
package config

import (
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/opencode-ai/swarm/internal/models"
)

// Config is the root configuration structure for Swarm.
type Config struct {
	// Global settings
	Global GlobalConfig `yaml:"global" mapstructure:"global"`

	// Database settings
	Database DatabaseConfig `yaml:"database" mapstructure:"database"`

	// Logging settings
	Logging LoggingConfig `yaml:"logging" mapstructure:"logging"`

	// Accounts contains configured provider profiles.
	Accounts []AccountConfig `yaml:"accounts" mapstructure:"accounts"`

	// Default settings for nodes
	NodeDefaults NodeConfig `yaml:"node_defaults" mapstructure:"node_defaults"`

	// Default settings for workspaces
	WorkspaceDefaults WorkspaceConfig `yaml:"workspace_defaults" mapstructure:"workspace_defaults"`

	// Default settings for agents
	AgentDefaults AgentConfig `yaml:"agent_defaults" mapstructure:"agent_defaults"`

	// Scheduler settings
	Scheduler SchedulerConfig `yaml:"scheduler" mapstructure:"scheduler"`

	// TUI settings
	TUI TUIConfig `yaml:"tui" mapstructure:"tui"`
}

// GlobalConfig contains global Swarm settings.
type GlobalConfig struct {
	// DataDir is where Swarm stores its data (default: ~/.local/share/swarm).
	DataDir string `yaml:"data_dir" mapstructure:"data_dir"`

	// ConfigDir is where config files are stored (default: ~/.config/swarm).
	ConfigDir string `yaml:"config_dir" mapstructure:"config_dir"`

	// AutoRegisterLocalNode automatically registers the local machine as a node.
	AutoRegisterLocalNode bool `yaml:"auto_register_local_node" mapstructure:"auto_register_local_node"`
}

// DatabaseConfig contains database settings.
type DatabaseConfig struct {
	// Path is the SQLite database file path.
	Path string `yaml:"path" mapstructure:"path"`

	// MaxConnections is the maximum number of database connections.
	MaxConnections int `yaml:"max_connections" mapstructure:"max_connections"`

	// BusyTimeout is how long to wait for a locked database (milliseconds).
	BusyTimeoutMs int `yaml:"busy_timeout_ms" mapstructure:"busy_timeout_ms"`
}

// LoggingConfig contains logging settings.
type LoggingConfig struct {
	// Level is the minimum log level (debug, info, warn, error).
	Level string `yaml:"level" mapstructure:"level"`

	// Format is the output format (json, console).
	Format string `yaml:"format" mapstructure:"format"`

	// File is an optional log file path.
	File string `yaml:"file" mapstructure:"file"`

	// EnableCaller adds caller information to logs.
	EnableCaller bool `yaml:"enable_caller" mapstructure:"enable_caller"`
}

// AccountConfig contains provider credentials and profile settings.
type AccountConfig struct {
	// Provider identifies the AI provider.
	Provider models.Provider `yaml:"provider" mapstructure:"provider"`

	// ProfileName is the human-friendly name for this account.
	ProfileName string `yaml:"profile_name" mapstructure:"profile_name"`

	// CredentialRef is a reference to the credential (env var, file path, or vault key).
	CredentialRef string `yaml:"credential_ref" mapstructure:"credential_ref"`

	// IsActive indicates if this account is enabled for use.
	IsActive bool `yaml:"is_active" mapstructure:"is_active"`
}

// NodeConfig contains default settings for nodes.
type NodeConfig struct {
	// SSHBackend is the default SSH backend (native, system, auto).
	SSHBackend models.SSHBackend `yaml:"ssh_backend" mapstructure:"ssh_backend"`

	// SSHTimeout is the connection timeout for SSH.
	SSHTimeout time.Duration `yaml:"ssh_timeout" mapstructure:"ssh_timeout"`

	// SSHKeyPath is the default SSH private key path.
	SSHKeyPath string `yaml:"ssh_key_path" mapstructure:"ssh_key_path"`

	// HealthCheckInterval is how often to check node health.
	HealthCheckInterval time.Duration `yaml:"health_check_interval" mapstructure:"health_check_interval"`
}

// WorkspaceConfig contains default settings for workspaces.
type WorkspaceConfig struct {
	// TmuxPrefix is the prefix for generated tmux session names.
	TmuxPrefix string `yaml:"tmux_prefix" mapstructure:"tmux_prefix"`

	// DefaultAgentType is the default agent type to spawn.
	DefaultAgentType models.AgentType `yaml:"default_agent_type" mapstructure:"default_agent_type"`

	// AutoImportExisting automatically imports existing tmux sessions.
	AutoImportExisting bool `yaml:"auto_import_existing" mapstructure:"auto_import_existing"`
}

// AgentConfig contains default settings for agents.
type AgentConfig struct {
	// DefaultType is the default agent type.
	DefaultType models.AgentType `yaml:"default_type" mapstructure:"default_type"`

	// StatePollingInterval is how often to poll agent state.
	StatePollingInterval time.Duration `yaml:"state_polling_interval" mapstructure:"state_polling_interval"`

	// IdleTimeout is how long of no activity before considering agent idle.
	IdleTimeout time.Duration `yaml:"idle_timeout" mapstructure:"idle_timeout"`

	// TranscriptBufferSize is the max lines to keep in transcript buffer.
	TranscriptBufferSize int `yaml:"transcript_buffer_size" mapstructure:"transcript_buffer_size"`

	// ApprovalPolicy is the default approval policy (strict, permissive).
	ApprovalPolicy string `yaml:"approval_policy" mapstructure:"approval_policy"`
}

// SchedulerConfig contains scheduler settings.
type SchedulerConfig struct {
	// DispatchInterval is how often the scheduler runs.
	DispatchInterval time.Duration `yaml:"dispatch_interval" mapstructure:"dispatch_interval"`

	// MaxRetries is the maximum dispatch retry count.
	MaxRetries int `yaml:"max_retries" mapstructure:"max_retries"`

	// RetryBackoff is the base backoff duration for retries.
	RetryBackoff time.Duration `yaml:"retry_backoff" mapstructure:"retry_backoff"`

	// DefaultCooldownDuration is the default cooldown after rate limiting.
	DefaultCooldownDuration time.Duration `yaml:"default_cooldown_duration" mapstructure:"default_cooldown_duration"`

	// AutoRotateOnRateLimit automatically rotates accounts on rate limit.
	AutoRotateOnRateLimit bool `yaml:"auto_rotate_on_rate_limit" mapstructure:"auto_rotate_on_rate_limit"`
}

// TUIConfig contains TUI settings.
type TUIConfig struct {
	// RefreshInterval is how often to refresh the display.
	RefreshInterval time.Duration `yaml:"refresh_interval" mapstructure:"refresh_interval"`

	// Theme is the color theme (default, dark, light).
	Theme string `yaml:"theme" mapstructure:"theme"`

	// ShowTimestamps shows timestamps in the UI.
	ShowTimestamps bool `yaml:"show_timestamps" mapstructure:"show_timestamps"`

	// CompactMode uses a more compact layout.
	CompactMode bool `yaml:"compact_mode" mapstructure:"compact_mode"`
}

// DefaultConfig returns the default configuration.
func DefaultConfig() *Config {
	homeDir, _ := os.UserHomeDir()

	return &Config{
		Global: GlobalConfig{
			DataDir:               filepath.Join(homeDir, ".local", "share", "swarm"),
			ConfigDir:             filepath.Join(homeDir, ".config", "swarm"),
			AutoRegisterLocalNode: true,
		},
		Database: DatabaseConfig{
			Path:           "", // Will be set to DataDir/swarm.db
			MaxConnections: 10,
			BusyTimeoutMs:  5000,
		},
		Logging: LoggingConfig{
			Level:        "info",
			Format:       "console",
			EnableCaller: false,
		},
		Accounts: []AccountConfig{},
		NodeDefaults: NodeConfig{
			SSHBackend:          models.SSHBackendAuto,
			SSHTimeout:          30 * time.Second,
			HealthCheckInterval: 60 * time.Second,
		},
		WorkspaceDefaults: WorkspaceConfig{
			TmuxPrefix:         "swarm",
			DefaultAgentType:   models.AgentTypeOpenCode,
			AutoImportExisting: false,
		},
		AgentDefaults: AgentConfig{
			DefaultType:          models.AgentTypeOpenCode,
			StatePollingInterval: 2 * time.Second,
			IdleTimeout:          10 * time.Second,
			TranscriptBufferSize: 10000,
			ApprovalPolicy:       "strict",
		},
		Scheduler: SchedulerConfig{
			DispatchInterval:        1 * time.Second,
			MaxRetries:              3,
			RetryBackoff:            5 * time.Second,
			DefaultCooldownDuration: 5 * time.Minute,
			AutoRotateOnRateLimit:   true,
		},
		TUI: TUIConfig{
			RefreshInterval: 500 * time.Millisecond,
			Theme:           "default",
			ShowTimestamps:  true,
			CompactMode:     false,
		},
	}
}

// Validate checks if the configuration is valid.
func (c *Config) Validate() error {
	if c.Database.MaxConnections < 1 {
		return fmt.Errorf("database.max_connections must be at least 1")
	}

	if c.AgentDefaults.StatePollingInterval < 100*time.Millisecond {
		return fmt.Errorf("agent_defaults.state_polling_interval must be at least 100ms")
	}

	if c.Scheduler.DispatchInterval < 100*time.Millisecond {
		return fmt.Errorf("scheduler.dispatch_interval must be at least 100ms")
	}

	for i, account := range c.Accounts {
		if account.Provider == "" {
			return fmt.Errorf("accounts[%d].provider is required", i)
		}
		if account.ProfileName == "" {
			return fmt.Errorf("accounts[%d].profile_name is required", i)
		}
		if account.CredentialRef == "" {
			return fmt.Errorf("accounts[%d].credential_ref is required", i)
		}
		switch account.Provider {
		case models.ProviderAnthropic, models.ProviderOpenAI, models.ProviderGoogle, models.ProviderCustom:
			// ok
		default:
			return fmt.Errorf("accounts[%d].provider must be one of anthropic, openai, google, custom", i)
		}
	}

	return nil
}

// EnsureDirectories creates required directories.
func (c *Config) EnsureDirectories() error {
	dirs := []string{
		c.Global.DataDir,
		c.Global.ConfigDir,
	}

	for _, dir := range dirs {
		if err := os.MkdirAll(dir, 0755); err != nil {
			return fmt.Errorf("failed to create directory %s: %w", dir, err)
		}
	}

	return nil
}

// DatabasePath returns the full database path.
func (c *Config) DatabasePath() string {
	if c.Database.Path != "" {
		return c.Database.Path
	}
	return filepath.Join(c.Global.DataDir, "swarm.db")
}
