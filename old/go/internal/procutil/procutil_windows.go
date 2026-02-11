//go:build windows

package procutil

import (
	"os/exec"
	"syscall"
)

const (
	_DETACHED_PROCESS                  = 0x00000008
	_PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
)

func configureDetached(cmd *exec.Cmd) {
	if cmd == nil {
		return
	}
	cmd.SysProcAttr = &syscall.SysProcAttr{
		CreationFlags: syscall.CREATE_NEW_PROCESS_GROUP | _DETACHED_PROCESS,
	}
}

func isProcessAlive(pid int) bool {
	if pid <= 0 {
		return false
	}

	handle, err := syscall.OpenProcess(_PROCESS_QUERY_LIMITED_INFORMATION, false, uint32(pid))
	if err != nil {
		return false
	}
	defer syscall.CloseHandle(handle)
	return true
}
