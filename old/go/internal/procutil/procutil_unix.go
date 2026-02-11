//go:build !windows

package procutil

import (
	"errors"
	"os"
	"os/exec"
	"syscall"
)

func configureDetached(cmd *exec.Cmd) {
	if cmd == nil {
		return
	}
	cmd.SysProcAttr = &syscall.SysProcAttr{
		Setsid: true,
	}
}

func isProcessAlive(pid int) bool {
	if pid <= 0 {
		return false
	}

	process, err := os.FindProcess(pid)
	if err != nil {
		return false
	}

	err = process.Signal(syscall.Signal(0))
	if err == nil {
		return true
	}
	// EPERM means process exists but we lack permission to signal it.
	return !errors.Is(err, syscall.ESRCH)
}
