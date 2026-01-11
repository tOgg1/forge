package fmail

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/spf13/cobra"
)

func runSend(cmd *cobra.Command, args []string) error {
	runtime, err := EnsureRuntime(cmd)
	if err != nil {
		return err
	}

	target := strings.TrimSpace(args[0])
	bodyArg := ""
	if len(args) > 1 {
		bodyArg = args[1]
	}

	filePath, _ := cmd.Flags().GetString("file")
	replyTo, _ := cmd.Flags().GetString("reply-to")
	priority, _ := cmd.Flags().GetString("priority")
	jsonOutput, _ := cmd.Flags().GetBool("json")

	normalizedTarget, _, err := NormalizeTarget(target)
	if err != nil {
		return Exitf(ExitCodeFailure, "invalid target %q: %v", target, err)
	}

	body, err := resolveSendBody(cmd, bodyArg, filePath)
	if err != nil {
		return err
	}

	priority = strings.ToLower(strings.TrimSpace(priority))
	if priority == "" {
		priority = PriorityNormal
	}
	if err := ValidatePriority(priority); err != nil {
		return Exitf(ExitCodeFailure, "invalid priority: %s", priority)
	}

	store, err := NewStore(runtime.Root)
	if err != nil {
		return Exitf(ExitCodeFailure, "init store: %v", err)
	}

	projectID, err := DeriveProjectID(runtime.Root)
	if err != nil {
		return Exitf(ExitCodeFailure, "derive project id: %v", err)
	}
	if _, err := store.EnsureProject(projectID); err != nil {
		return Exitf(ExitCodeFailure, "ensure project: %v", err)
	}

	host, _ := os.Hostname()
	if _, err := store.UpdateAgentRecord(runtime.Agent, host); err != nil {
		return Exitf(ExitCodeFailure, "update agent registry: %v", err)
	}

	message := &Message{
		From: runtime.Agent,
		To:   normalizedTarget,
		Body: body,
	}

	if strings.TrimSpace(replyTo) != "" {
		message.ReplyTo = strings.TrimSpace(replyTo)
	}
	if cmd.Flags().Changed("priority") {
		message.Priority = priority
	}

	if _, err := store.SaveMessage(message); err != nil {
		if errors.Is(err, ErrMessageTooLarge) {
			return Exitf(ExitCodeFailure, "message exceeds 1MB limit")
		}
		return Exitf(ExitCodeFailure, "save message: %v", err)
	}

	if jsonOutput {
		payload, err := json.MarshalIndent(message, "", "  ")
		if err != nil {
			return Exitf(ExitCodeFailure, "encode message: %v", err)
		}
		fmt.Fprintln(cmd.OutOrStdout(), string(payload))
		return nil
	}

	fmt.Fprintln(cmd.OutOrStdout(), message.ID)
	return nil
}

func resolveSendBody(cmd *cobra.Command, bodyArg, filePath string) (any, error) {
	bodyArgTrim := strings.TrimSpace(bodyArg)
	filePath = strings.TrimSpace(filePath)

	if filePath != "" && bodyArgTrim != "" {
		return nil, usageError(cmd, "provide either a message argument or --file, not both")
	}

	var raw string
	switch {
	case filePath != "":
		data, err := os.ReadFile(filePath)
		if err != nil {
			return nil, Exitf(ExitCodeFailure, "read file: %v", err)
		}
		raw = string(data)
	case bodyArgTrim != "":
		raw = bodyArg
	default:
		data, err := readStdinIfPiped()
		if err != nil {
			return nil, Exitf(ExitCodeFailure, "read stdin: %v", err)
		}
		raw = data
	}

	if strings.TrimSpace(raw) == "" {
		return nil, usageError(cmd, "message body is required")
	}
	return parseMessageBody(raw)
}

func readStdinIfPiped() (string, error) {
	info, err := os.Stdin.Stat()
	if err != nil {
		return "", err
	}
	if info.Mode()&os.ModeCharDevice != 0 {
		return "", nil
	}
	data, err := io.ReadAll(os.Stdin)
	if err != nil {
		return "", err
	}
	return string(data), nil
}

func parseMessageBody(raw string) (any, error) {
	trimmed := strings.TrimSpace(raw)
	if trimmed == "" {
		return nil, fmt.Errorf("empty message body")
	}
	var value any
	if err := json.Unmarshal([]byte(trimmed), &value); err == nil {
		if value == nil {
			return json.RawMessage("null"), nil
		}
		return value, nil
	}
	return raw, nil
}
