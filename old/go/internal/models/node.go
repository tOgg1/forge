// Package models defines the core domain types for Forge.
package models

import (
	"time"
)

// NodeStatus represents the current status of a node.
type NodeStatus string

const (
	NodeStatusOnline  NodeStatus = "online"
	NodeStatusOffline NodeStatus = "offline"
	NodeStatusUnknown NodeStatus = "unknown"
)

// SSHBackend specifies which SSH implementation to use for a node.
type SSHBackend string

const (
	SSHBackendNative SSHBackend = "native" // Go's x/crypto/ssh
	SSHBackendSystem SSHBackend = "system" // System ssh binary
	SSHBackendAuto   SSHBackend = "auto"   // Auto-detect best option
)

// ExecutionMode specifies how commands are executed on a node.
type ExecutionMode string

const (
	// ExecutionModeAuto tries forged first, falls back to SSH.
	ExecutionModeAuto ExecutionMode = "auto"
	// ExecutionModeForged forces use of the forged daemon.
	ExecutionModeForged ExecutionMode = "forged"
	// ExecutionModeSSH forces use of direct SSH execution.
	ExecutionModeSSH ExecutionMode = "ssh"
)

// Node represents a machine that Forge can control via SSH and tmux.
type Node struct {
	// ID is the unique identifier for the node.
	ID string `json:"id"`

	// Name is the human-friendly name for the node.
	Name string `json:"name"`

	// SSHTarget is the SSH connection string (user@host:port).
	SSHTarget string `json:"ssh_target"`

	// SSHBackend specifies which SSH implementation to use.
	SSHBackend SSHBackend `json:"ssh_backend"`

	// SSHKeyPath is the path to the SSH private key (optional).
	SSHKeyPath string `json:"ssh_key_path,omitempty"`

	// SSHAgentForwarding enables SSH agent forwarding.
	SSHAgentForwarding bool `json:"ssh_agent_forwarding,omitempty"`

	// SSHProxyJump specifies a bastion host to reach the target.
	SSHProxyJump string `json:"ssh_proxy_jump,omitempty"`

	// SSHControlMaster configures SSH multiplexing (auto/yes/no).
	SSHControlMaster string `json:"ssh_control_master,omitempty"`

	// SSHControlPath is the socket path for SSH multiplexing.
	SSHControlPath string `json:"ssh_control_path,omitempty"`

	// SSHControlPersist controls how long master connections stay alive.
	SSHControlPersist string `json:"ssh_control_persist,omitempty"`

	// SSHTimeoutSeconds overrides the default connection timeout.
	SSHTimeoutSeconds int `json:"ssh_timeout_seconds,omitempty"`

	// ForgedEnabled indicates if forged daemon is expected on this node.
	ForgedEnabled bool `json:"forged_enabled,omitempty"`

	// ForgedPort is the port where forged listens (default: 50051).
	ForgedPort int `json:"forged_port,omitempty"`

	// ExecutionMode controls how commands are executed on this node.
	// "auto" tries forged first, "forged" forces daemon, "ssh" forces SSH.
	ExecutionMode ExecutionMode `json:"execution_mode,omitempty"`

	// ForgedAvailable indicates if forged was detected as running.
	// This is set during connection tests and updated dynamically.
	ForgedAvailable bool `json:"forged_available,omitempty"`

	// Status is the current connection status.
	Status NodeStatus `json:"status"`

	// IsLocal indicates if this is the local machine (no SSH needed).
	IsLocal bool `json:"is_local"`

	// LastSeen is the timestamp of the last successful connection.
	LastSeen *time.Time `json:"last_seen,omitempty"`

	// AgentCount is the number of agents currently running on this node.
	AgentCount int `json:"agent_count"`

	// Metadata contains additional node information.
	Metadata NodeMetadata `json:"metadata,omitempty"`

	// CreatedAt is when the node was added to Forge.
	CreatedAt time.Time `json:"created_at"`

	// UpdatedAt is when the node was last updated.
	UpdatedAt time.Time `json:"updated_at"`
}

// NodeMetadata contains additional information about a node.
type NodeMetadata struct {
	// TmuxVersion is the installed tmux version.
	TmuxVersion string `json:"tmux_version,omitempty"`

	// Platform is the OS/platform (e.g., "linux", "darwin").
	Platform string `json:"platform,omitempty"`

	// Hostname is the node's hostname.
	Hostname string `json:"hostname,omitempty"`

	// AvailableAdapters lists installed agent CLIs.
	AvailableAdapters []string `json:"available_adapters,omitempty"`

	// ForgedVersion is the forged daemon version if detected.
	ForgedVersion string `json:"forged_version,omitempty"`

	// ForgedStatus indicates the last known forged status ("running", "stopped", "unknown").
	ForgedStatus string `json:"forged_status,omitempty"`
}

// Validate checks if the node configuration is valid.
func (n *Node) Validate() error {
	validation := &ValidationErrors{}
	if n.Name == "" {
		validation.Add("name", ErrInvalidNodeName)
	}
	if !n.IsLocal && n.SSHTarget == "" {
		validation.Add("ssh_target", ErrInvalidSSHTarget)
	}
	return validation.Err()
}
