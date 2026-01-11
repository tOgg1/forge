package fmail

import (
	"os"

	"github.com/spf13/cobra"
)

func Execute(version string) error {
	if hasRobotHelpFlag(os.Args[1:]) {
		return writeRobotHelp(os.Stdout, version)
	}
	return newRootCmd(version).Execute()
}

func newRootCmd(version string) *cobra.Command {
	cmd := &cobra.Command{
		Use:           "fmail",
		Short:         "Agent-to-agent messaging via .fmail files",
		Long:          "fmail sends and receives messages via .fmail/ files.",
		SilenceUsage:  true,
		SilenceErrors: true,
		Version:       version,
	}
	cmd.PersistentFlags().Bool("robot-help", false, "Machine-readable help output")

	cmd.AddCommand(
		newSendCmd(),
		newLogCmd(),
		newWatchCmd(),
		newWhoCmd(),
		newStatusCmd(),
		newTopicsCmd(),
		newGCCmd(),
		newInitCmd(),
	)

	return cmd
}
