// Package ssh provides abstractions for executing commands on remote nodes.
package ssh

import (
	"context"
	"io"
	"time"
)

// Executor defines a common interface for running commands over SSH.
type Executor interface {
	// Exec runs a command and returns its stdout and stderr output.
	Exec(ctx context.Context, cmd string) (stdout, stderr []byte, err error)

	// ExecInteractive runs a command, streaming stdin to the remote process.
	ExecInteractive(ctx context.Context, cmd string, stdin io.Reader) error

	// StartSession opens a long-lived SSH session for multiple commands.
	StartSession() (Session, error)

	// Close releases any resources held by the executor.
	Close() error
}

// Session represents a long-lived SSH session.
type Session interface {
	// Exec runs a command and returns its stdout and stderr output.
	Exec(ctx context.Context, cmd string) (stdout, stderr []byte, err error)

	// ExecInteractive runs a command, streaming stdin to the remote process.
	ExecInteractive(ctx context.Context, cmd string, stdin io.Reader) error

	// Close ends the session.
	Close() error
}

// ConnectionOptions configures how an SSH connection is established.
type ConnectionOptions struct {
	// Host is the target host name or IP.
	Host string

	// Port is the SSH port (defaults to 22 when unset).
	Port int

	// User is the SSH username.
	User string

	// KeyPath is an optional path to the private key.
	KeyPath string

	// AgentForwarding enables SSH agent forwarding when supported.
	AgentForwarding bool

	// ProxyJump specifies a bastion host to reach the target (user@host:port).
	ProxyJump string

	// Timeout controls how long to wait when establishing connections.
	Timeout time.Duration
}
