//go:build !windows

package procutil

import (
	"os/exec"
	"testing"
)

func TestConfigureDetachedSetsid(t *testing.T) {
	cmd := exec.Command("true")
	ConfigureDetached(cmd)
	if cmd.SysProcAttr == nil {
		t.Fatalf("SysProcAttr is nil")
	}
	if !cmd.SysProcAttr.Setsid {
		t.Fatalf("expected Setsid=true")
	}
}
