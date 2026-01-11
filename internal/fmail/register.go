package fmail

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"strings"

	"github.com/spf13/cobra"
)

const registerMaxAttempts = 10

func newRegisterCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "register [name]",
		Short: "Request a unique agent name",
		Args:  argsMax(1),
		RunE:  runRegister,
	}
	cmd.Flags().Bool("json", false, "Output as JSON")
	return cmd
}

func runRegister(cmd *cobra.Command, args []string) error {
	root, err := DiscoverProjectRoot("")
	if err != nil {
		return Exitf(ExitCodeFailure, "resolve project root: %v", err)
	}

	store, err := NewStore(root)
	if err != nil {
		return Exitf(ExitCodeFailure, "init store: %v", err)
	}

	jsonOutput, _ := cmd.Flags().GetBool("json")
	host, _ := os.Hostname()

	if len(args) > 0 {
		normalized, err := NormalizeAgentName(args[0])
		if err != nil {
			return Exitf(ExitCodeFailure, "invalid agent name: %v", err)
		}
		record, err := store.RegisterAgentRecord(normalized, host)
		if err != nil {
			if errors.Is(err, ErrAgentExists) {
				return Exitf(ExitCodeFailure, "agent name already registered: %s", normalized)
			}
			return Exitf(ExitCodeFailure, "register agent: %v", err)
		}
		return writeRegisterResult(cmd, record, jsonOutput)
	}

	record, err := registerGeneratedAgent(store, host)
	if err != nil {
		return Exitf(ExitCodeFailure, "register agent: %v", err)
	}
	return writeRegisterResult(cmd, record, jsonOutput)
}

func registerGeneratedAgent(store *Store, host string) (*AgentRecord, error) {
	for attempt := 0; attempt < registerMaxAttempts; attempt++ {
		candidate := fmt.Sprintf("agent-%s", NewMessageID())
		record, err := store.RegisterAgentRecord(candidate, host)
		if err == nil {
			return record, nil
		}
		if errors.Is(err, ErrAgentExists) {
			continue
		}
		return nil, err
	}
	return nil, fmt.Errorf("unable to allocate unique agent name")
}

func writeRegisterResult(cmd *cobra.Command, record *AgentRecord, jsonOutput bool) error {
	if jsonOutput {
		payload, err := json.MarshalIndent(record, "", "  ")
		if err != nil {
			return Exitf(ExitCodeFailure, "encode agent: %v", err)
		}
		fmt.Fprintln(cmd.OutOrStdout(), string(payload))
		return nil
	}
	fmt.Fprintln(cmd.OutOrStdout(), strings.TrimSpace(record.Name))
	return nil
}
