package fmailtui

import (
	"github.com/spf13/cobra"
)

func Execute(version string) error {
	return newRootCmd(version).Execute()
}

func newRootCmd(version string) *cobra.Command {
	cfg := Config{}
	cmd := &cobra.Command{
		Use:           "fmail-tui",
		Short:         "fmail terminal UI",
		Long:          "Bubbletea-based terminal UI for fmail.",
		SilenceUsage:  true,
		SilenceErrors: true,
		Version:       version,
		RunE: func(cmd *cobra.Command, args []string) error {
			return Run(cfg)
		},
	}
	cmd.Flags().StringVar(&cfg.ProjectID, "project", "", "fmail project ID override")
	cmd.Flags().StringVar(&cfg.Root, "root", "", "project root containing .fmail")
	cmd.Flags().StringVar(&cfg.ForgedAddr, "forged-addr", "", "forged endpoint (socket path or host:port)")
	cmd.Flags().StringVar(&cfg.Theme, "theme", string(ThemeDefault), "theme: default|high-contrast")
	cmd.Flags().DurationVar(&cfg.PollInterval, "poll-interval", defaultPollInterval, "poll interval for background refresh")
	return cmd
}
