package fmail

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"strings"
	"time"

	"github.com/spf13/cobra"
)

type sendResult struct {
	ID      string
	Message *Message
}

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

	result, err := sendViaForged(runtime, message)
	if err == nil {
		return writeSendResult(cmd, result, jsonOutput)
	}

	if errors.Is(err, errForgedUnavailable) || errors.Is(err, errForgedDisconnected) {
		if errors.Is(err, errForgedDisconnected) {
			fmt.Fprintln(cmd.ErrOrStderr(), "Warning: forged connection dropped, falling back to standalone (message may be duplicated)")
		}
		result, err = sendStandalone(runtime, message)
		if err != nil {
			return err
		}
		return writeSendResult(cmd, result, jsonOutput)
	}

	var exitErr *ExitError
	if errors.As(err, &exitErr) {
		return exitErr
	}
	var serverErr *forgedServerError
	if errors.As(err, &serverErr) {
		return Exitf(ExitCodeFailure, "forged: %s", serverErr.Error())
	}
	return Exitf(ExitCodeFailure, "forged: %v", err)
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

func sendViaForged(runtime *Runtime, message *Message) (sendResult, error) {
	if runtime == nil {
		return sendResult{}, Exitf(ExitCodeFailure, "runtime unavailable")
	}
	if message == nil {
		return sendResult{}, Exitf(ExitCodeFailure, "message is required")
	}

	projectID, err := resolveProjectID(runtime.Root)
	if err != nil {
		return sendResult{}, Exitf(ExitCodeFailure, "resolve project id: %v", err)
	}
	body, err := encodeMailBody(message.Body)
	if err != nil {
		return sendResult{}, Exitf(ExitCodeFailure, "encode message: %v", err)
	}

	conn, err := dialForged(runtime.Root)
	if err != nil {
		return sendResult{}, err
	}
	defer conn.Close()

	host, _ := os.Hostname()
	req := mailSendRequest{
		mailBaseRequest: mailBaseRequest{
			Cmd:       "send",
			ProjectID: projectID,
			Agent:     runtime.Agent,
			Host:      host,
			ReqID:     nextReqID(),
		},
		To:       message.To,
		Body:     body,
		ReplyTo:  message.ReplyTo,
		Priority: message.Priority,
	}

	if err := conn.writeJSON(req); err != nil {
		return sendResult{}, errForgedDisconnected
	}

	line, err := conn.readLine()
	if err != nil {
		return sendResult{}, errForgedDisconnected
	}

	var resp mailResponse
	if err := json.Unmarshal(line, &resp); err != nil {
		return sendResult{}, fmt.Errorf("invalid forged response: %w", err)
	}
	if !resp.OK {
		if resp.Error == nil {
			return sendResult{}, &forgedServerError{Message: "unknown error"}
		}
		return sendResult{}, &forgedServerError{
			Code:      resp.Error.Code,
			Message:   resp.Error.Message,
			Retryable: resp.Error.Retryable,
		}
	}
	if strings.TrimSpace(resp.ID) == "" {
		return sendResult{}, fmt.Errorf("forged response missing id")
	}

	result := sendResult{ID: resp.ID}
	result.Message = loadSentMessage(runtime.Root, message.To, resp.ID)
	if result.Message == nil {
		result.Message = copySendMessage(message, resp.ID)
	}
	return result, nil
}

func sendStandalone(runtime *Runtime, message *Message) (sendResult, error) {
	if runtime == nil {
		return sendResult{}, Exitf(ExitCodeFailure, "runtime unavailable")
	}
	if message == nil {
		return sendResult{}, Exitf(ExitCodeFailure, "message is required")
	}

	store, err := NewStore(runtime.Root)
	if err != nil {
		return sendResult{}, Exitf(ExitCodeFailure, "init store: %v", err)
	}

	projectID, err := DeriveProjectID(runtime.Root)
	if err != nil {
		return sendResult{}, Exitf(ExitCodeFailure, "derive project id: %v", err)
	}
	if _, err := store.EnsureProject(projectID); err != nil {
		return sendResult{}, Exitf(ExitCodeFailure, "ensure project: %v", err)
	}

	host, _ := os.Hostname()
	if _, err := store.UpdateAgentRecord(runtime.Agent, host); err != nil {
		return sendResult{}, Exitf(ExitCodeFailure, "update agent registry: %v", err)
	}

	if _, err := store.SaveMessage(message); err != nil {
		if errors.Is(err, ErrMessageTooLarge) {
			return sendResult{}, Exitf(ExitCodeFailure, "message exceeds 1MB limit")
		}
		return sendResult{}, Exitf(ExitCodeFailure, "save message: %v", err)
	}

	return sendResult{ID: message.ID, Message: message}, nil
}

func writeSendResult(cmd *cobra.Command, result sendResult, jsonOutput bool) error {
	if jsonOutput {
		payload := result.Message
		if payload == nil {
			payload = &Message{ID: result.ID, Time: time.Now().UTC()}
		}
		data, err := json.MarshalIndent(payload, "", "  ")
		if err != nil {
			return Exitf(ExitCodeFailure, "encode message: %v", err)
		}
		fmt.Fprintln(cmd.OutOrStdout(), string(data))
		return nil
	}
	fmt.Fprintln(cmd.OutOrStdout(), result.ID)
	return nil
}

func loadSentMessage(root, target, id string) *Message {
	store, err := NewStore(root)
	if err != nil {
		return nil
	}
	path := store.TopicMessagePath(target, id)
	if strings.HasPrefix(target, "@") {
		path = store.DMMessagePath(strings.TrimPrefix(target, "@"), id)
	}
	message, err := store.ReadMessage(path)
	if err != nil {
		return nil
	}
	return message
}

func copySendMessage(message *Message, id string) *Message {
	if message == nil {
		return &Message{ID: id, Time: time.Now().UTC()}
	}
	clone := *message
	clone.ID = id
	if clone.Time.IsZero() {
		clone.Time = time.Now().UTC()
	}
	return &clone
}
