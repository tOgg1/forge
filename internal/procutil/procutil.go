package procutil

import "os/exec"

// ConfigureDetached configures a command to run detached from the current session/process group.
func ConfigureDetached(cmd *exec.Cmd) {
	configureDetached(cmd)
}

// IsProcessAlive reports whether a process with the given PID appears alive.
func IsProcessAlive(pid int) bool {
	return isProcessAlive(pid)
}
