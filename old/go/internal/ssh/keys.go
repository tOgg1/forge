package ssh

import (
	"errors"
	"fmt"
	"net"
	"os"

	"golang.org/x/crypto/ssh/agent"
	"golang.org/x/term"

	xssh "golang.org/x/crypto/ssh"
)

// PassphrasePrompt returns the passphrase for the provided key path.
type PassphrasePrompt func(keyPath string) (string, error)

// AgentConnection wraps a live SSH agent connection.
type AgentConnection struct {
	Conn   net.Conn
	Client agent.ExtendedAgent
}

// LoadPrivateKey loads a private key from disk, prompting for a passphrase when required.
func LoadPrivateKey(path string, prompt PassphrasePrompt) (xssh.Signer, error) {
	keyBytes, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read private key: %w", err)
	}

	signer, err := xssh.ParsePrivateKey(keyBytes)
	if err == nil {
		return signer, nil
	}

	var missing *xssh.PassphraseMissingError
	if !errors.As(err, &missing) {
		return nil, fmt.Errorf("parse private key: %w", err)
	}

	if prompt == nil {
		return nil, ErrPassphraseRequired
	}

	passphrase, err := prompt(path)
	if err != nil {
		return nil, fmt.Errorf("passphrase prompt failed: %w", err)
	}
	if passphrase == "" {
		return nil, ErrPassphraseRequired
	}

	signer, err = xssh.ParsePrivateKeyWithPassphrase(keyBytes, []byte(passphrase))
	if err != nil {
		return nil, fmt.Errorf("parse private key with passphrase: %w", err)
	}

	return signer, nil
}

// DefaultPassphrasePrompt reads a passphrase from stdin without echoing input.
func DefaultPassphrasePrompt(path string) (string, error) {
	fd := int(os.Stdin.Fd())
	if !term.IsTerminal(fd) {
		return "", fmt.Errorf("stdin is not a terminal")
	}

	fmt.Fprintf(os.Stderr, "Enter passphrase for %s: ", path)
	passphrase, err := term.ReadPassword(fd)
	fmt.Fprintln(os.Stderr)
	if err != nil {
		return "", err
	}

	return string(passphrase), nil
}

// ConnectAgent opens a connection to the SSH agent referenced by SSH_AUTH_SOCK.
func ConnectAgent() (*AgentConnection, error) {
	sock := os.Getenv("SSH_AUTH_SOCK")
	if sock == "" {
		return nil, ErrSSHAgentUnavailable
	}

	conn, err := net.Dial("unix", sock)
	if err != nil {
		return nil, fmt.Errorf("connect to ssh agent: %w", err)
	}

	return &AgentConnection{
		Conn:   conn,
		Client: agent.NewClient(conn),
	}, nil
}

// Signers returns the agent-backed signers.
func (a *AgentConnection) Signers() ([]xssh.Signer, error) {
	if a == nil || a.Client == nil {
		return nil, ErrSSHAgentUnavailable
	}
	return a.Client.Signers()
}

// AuthMethod returns an AuthMethod backed by the SSH agent.
func (a *AgentConnection) AuthMethod() xssh.AuthMethod {
	if a == nil || a.Client == nil {
		return nil
	}
	return xssh.PublicKeysCallback(a.Client.Signers)
}

// Close closes the underlying SSH agent connection.
func (a *AgentConnection) Close() error {
	if a == nil || a.Conn == nil {
		return nil
	}
	return a.Conn.Close()
}
