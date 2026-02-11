package ssh

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"math"
	"os/exec"
)

var (
	// ErrMissingHost indicates no host was provided in connection options.
	ErrMissingHost = errors.New("ssh host is required")
)

// ExecError wraps command failures with exit details.
type ExecError struct {
	Command  string
	ExitCode int
	Stdout   []byte
	Stderr   []byte
	Err      error
}

func (e *ExecError) Error() string {
	return fmt.Sprintf("ssh command failed (exit=%d): %s", e.ExitCode, e.Command)
}

// SystemExecutor runs SSH commands using the system ssh binary.
type SystemExecutor struct {
	options ConnectionOptions
	binary  string
}

// NewSystemExecutor creates a new SystemExecutor with the given options.
func NewSystemExecutor(options ConnectionOptions) *SystemExecutor {
	return &SystemExecutor{options: options, binary: "ssh"}
}

// SetBinary overrides the ssh binary path.
func (e *SystemExecutor) SetBinary(path string) {
	if path != "" {
		e.binary = path
	}
}

// Exec runs a command and returns its stdout and stderr output.
func (e *SystemExecutor) Exec(ctx context.Context, cmd string) (stdout, stderr []byte, err error) {
	return e.exec(ctx, cmd, nil)
}

// ExecInteractive runs a command, streaming stdin to the remote process.
func (e *SystemExecutor) ExecInteractive(ctx context.Context, cmd string, stdin io.Reader) error {
	_, _, err := e.exec(ctx, cmd, stdin)
	return err
}

// StartSession opens a long-lived SSH session.
func (e *SystemExecutor) StartSession() (Session, error) {
	return &SystemSession{executor: e}, nil
}

// Close releases any resources held by the executor.
func (e *SystemExecutor) Close() error {
	return nil
}

func (e *SystemExecutor) exec(ctx context.Context, cmd string, stdin io.Reader) ([]byte, []byte, error) {
	if e.options.Host == "" {
		return nil, nil, ErrMissingHost
	}

	args, target := buildSSHArgs(e.options)
	args = append(args, target, cmd)

	command := exec.CommandContext(ctx, e.binary, args...)
	if stdin != nil {
		command.Stdin = stdin
	}

	var stdoutBuf bytes.Buffer
	var stderrBuf bytes.Buffer
	command.Stdout = &stdoutBuf
	command.Stderr = &stderrBuf

	err := command.Run()
	stdout := stdoutBuf.Bytes()
	stderr := stderrBuf.Bytes()
	if err != nil {
		return stdout, stderr, wrapExecError(err, cmd, stdout, stderr)
	}
	return stdout, stderr, nil
}

// SystemSession provides a simple session wrapper for the system ssh executor.
type SystemSession struct {
	executor *SystemExecutor
}

// Exec runs a command and returns its stdout and stderr output.
func (s *SystemSession) Exec(ctx context.Context, cmd string) (stdout, stderr []byte, err error) {
	return s.executor.Exec(ctx, cmd)
}

// ExecInteractive runs a command, streaming stdin to the remote process.
func (s *SystemSession) ExecInteractive(ctx context.Context, cmd string, stdin io.Reader) error {
	return s.executor.ExecInteractive(ctx, cmd, stdin)
}

// Close ends the session.
func (s *SystemSession) Close() error {
	return nil
}

func buildSSHArgs(options ConnectionOptions) ([]string, string) {
	args := []string{}
	if options.Port > 0 {
		args = append(args, "-p", fmt.Sprintf("%d", options.Port))
	}
	if options.KeyPath != "" {
		args = append(args, "-i", options.KeyPath)
	}
	if options.AgentForwarding {
		args = append(args, "-A")
	}
	if options.ProxyJump != "" {
		args = append(args, "-J", options.ProxyJump)
	}
	if options.ControlMaster != "" {
		args = append(args, "-o", fmt.Sprintf("ControlMaster=%s", options.ControlMaster))
	}
	if options.ControlPath != "" {
		args = append(args, "-o", fmt.Sprintf("ControlPath=%s", options.ControlPath))
	}
	if options.ControlPersist != "" {
		args = append(args, "-o", fmt.Sprintf("ControlPersist=%s", options.ControlPersist))
	}
	if options.Timeout > 0 {
		seconds := int(math.Ceil(options.Timeout.Seconds()))
		args = append(args, "-o", fmt.Sprintf("ConnectTimeout=%d", seconds))
	}

	target := options.Host
	if options.User != "" {
		target = fmt.Sprintf("%s@%s", options.User, options.Host)
	}
	return args, target
}

func wrapExecError(err error, cmd string, stdout, stderr []byte) error {
	if exitErr, ok := err.(*exec.ExitError); ok {
		return &ExecError{
			Command:  cmd,
			ExitCode: exitErr.ExitCode(),
			Stdout:   stdout,
			Stderr:   stderr,
			Err:      err,
		}
	}
	return err
}
