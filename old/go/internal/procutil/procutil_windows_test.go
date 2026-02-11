//go:build windows

package procutil

import (
	"os/exec"
	"testing"
)

func TestConfigureDetachedCreationFlags(t *testing.T) {
	cmd := exec.Command("cmd", "/c", "echo", "ok")
	ConfigureDetached(cmd)
	if cmd.SysProcAttr == nil {
		t.Fatalf("SysProcAttr is nil")
	}
	if cmd.SysProcAttr.CreationFlags == 0 {
		t.Fatalf("expected CreationFlags set")
	}
}
