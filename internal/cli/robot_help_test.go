package cli

import (
	"bytes"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestRobotHelp_PrintsAndExitsZero(t *testing.T) {
	// globals; keep isolated
	prev := robotHelp
	robotHelp = false
	defer func() { robotHelp = prev }()

	var out bytes.Buffer
	rootCmd.SetOut(&out)
	rootCmd.SetErr(&out)
	rootCmd.SetArgs([]string{"--robot-help"})
	defer rootCmd.SetArgs(nil)

	err := rootCmd.Execute()
	var exitErr *ExitError
	require.ErrorAs(t, err, &exitErr)
	require.Equal(t, 0, exitErr.Code)

	s := out.String()
	require.Contains(t, s, "Forge Robot Help")
	require.Contains(t, s, "forge mem")
	require.Contains(t, s, "forge work")
	require.Contains(t, s, "FORGE_LOOP_ID")
}
