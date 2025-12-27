package cli

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"strings"
	"time"
)

type mailMCPClient struct {
	url        string
	httpClient *http.Client
}

func newMailMCPClient(cfg mailConfig) *mailMCPClient {
	timeout := cfg.Timeout
	if timeout <= 0 {
		timeout = defaultMailTimeout
	}
	return &mailMCPClient{
		url: cfg.URL,
		httpClient: &http.Client{
			Timeout: timeout,
		},
	}
}

func (c *mailMCPClient) SendMessage(ctx context.Context, req mailSendRequest) error {
	args := map[string]any{
		"project_key": req.Project,
		"sender_name": req.From,
		"to":          req.To,
		"subject":     req.Subject,
		"body_md":     req.Body,
	}
	if req.Priority != "" {
		args["importance"] = req.Priority
	}
	if req.AckRequired {
		args["ack_required"] = true
	}
	return c.callTool(ctx, "send_message", args, nil)
}

func (c *mailMCPClient) FetchInbox(ctx context.Context, req mailInboxRequest) ([]mailMessage, error) {
	return c.fetchInbox(ctx, req.Project, req.Agent, req.Limit, req.Since, false)
}

func (c *mailMCPClient) ReadMessage(ctx context.Context, req mailReadRequest) (mailMessage, error) {
	messages, err := c.fetchInbox(ctx, req.Project, req.Agent, req.Limit, req.Since, true)
	if err != nil {
		return mailMessage{}, err
	}
	for _, msg := range messages {
		if msg.ID == req.MessageID {
			return msg, nil
		}
	}
	return mailMessage{}, fmt.Errorf("message %s not found (try a larger --limit)", formatMailID(req.MessageID))
}

func (c *mailMCPClient) MarkRead(ctx context.Context, req mailStatusRequest) error {
	args := map[string]any{
		"project_key": req.Project,
		"agent_name":  req.Agent,
		"message_id":  req.MessageID,
	}
	return c.callTool(ctx, "mark_message_read", args, nil)
}

func (c *mailMCPClient) Acknowledge(ctx context.Context, req mailStatusRequest) error {
	args := map[string]any{
		"project_key": req.Project,
		"agent_name":  req.Agent,
		"message_id":  req.MessageID,
	}
	return c.callTool(ctx, "acknowledge_message", args, nil)
}

func (c *mailMCPClient) fetchInbox(ctx context.Context, project, agent string, limit int, since *time.Time, includeBodies bool) ([]mailMessage, error) {
	args := map[string]any{
		"project_key":    project,
		"agent_name":     agent,
		"include_bodies": includeBodies,
	}
	if limit > 0 {
		args["limit"] = limit
	}
	if since != nil && !since.IsZero() {
		args["since_ts"] = since.UTC().Format(time.RFC3339Nano)
	}

	raw, err := c.callToolRaw(ctx, "fetch_inbox", args)
	if err != nil {
		return nil, err
	}

	var items []mcpInboxMessage
	if err := decodeToolResult(raw, &items); err != nil {
		var payload struct {
			Messages []mcpInboxMessage `json:"messages"`
		}
		if err := decodeToolResult(raw, &payload); err != nil {
			return nil, err
		}
		items = payload.Messages
	}

	messages := make([]mailMessage, 0, len(items))
	for _, item := range items {
		createdAt := parseMailTime(item.CreatedAt)
		messages = append(messages, mailMessage{
			ID:          item.ID,
			ThreadID:    item.ThreadID,
			From:        item.From,
			Subject:     item.Subject,
			Body:        item.Body,
			CreatedAt:   createdAt,
			Importance:  item.Importance,
			AckRequired: item.AckRequired,
			Backend:     string(mailBackendMCP),
		})
	}

	return messages, nil
}

type mcpRequest struct {
	JSONRPC string `json:"jsonrpc"`
	ID      string `json:"id"`
	Method  string `json:"method"`
	Params  any    `json:"params"`
}

type mcpResponse struct {
	Result json.RawMessage `json:"result"`
	Error  *mcpError       `json:"error"`
}

type mcpError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

type mcpToolCallParams struct {
	Name      string `json:"name"`
	Arguments any    `json:"arguments"`
}

type mcpToolResult struct {
	Content []mcpToolContent `json:"content"`
}

type mcpToolContent struct {
	Type string          `json:"type"`
	Text string          `json:"text,omitempty"`
	JSON json.RawMessage `json:"json,omitempty"`
}

type mcpInboxMessage struct {
	ID          int64  `json:"id"`
	ThreadID    string `json:"thread_id"`
	Subject     string `json:"subject"`
	From        string `json:"from"`
	Body        string `json:"body_md"`
	CreatedAt   string `json:"created_ts"`
	Importance  string `json:"importance"`
	AckRequired bool   `json:"ack_required"`
}

func (c *mailMCPClient) callTool(ctx context.Context, name string, args any, out any) error {
	raw, err := c.callToolRaw(ctx, name, args)
	if err != nil {
		return err
	}
	if out == nil {
		return nil
	}
	return decodeToolResult(raw, out)
}

func (c *mailMCPClient) callToolRaw(ctx context.Context, name string, args any) (json.RawMessage, error) {
	params := mcpToolCallParams{
		Name:      name,
		Arguments: args,
	}
	return c.call(ctx, "tools/call", params)
}

func (c *mailMCPClient) call(ctx context.Context, method string, params any) (json.RawMessage, error) {
	if ctx == nil {
		ctx = context.Background()
	}

	req := mcpRequest{
		JSONRPC: "2.0",
		ID:      fmt.Sprintf("swarm-mail-%d", time.Now().UnixNano()),
		Method:  method,
		Params:  params,
	}

	payload, err := json.Marshal(req)
	if err != nil {
		return nil, fmt.Errorf("encode mcp request: %w", err)
	}

	httpReq, err := http.NewRequestWithContext(ctx, http.MethodPost, c.url, bytes.NewReader(payload))
	if err != nil {
		return nil, fmt.Errorf("build mcp request: %w", err)
	}
	httpReq.Header.Set("Content-Type", "application/json")

	resp, err := c.httpClient.Do(httpReq)
	if err != nil {
		return nil, fmt.Errorf("call mcp server: %w", err)
	}
	defer resp.Body.Close()

	var response mcpResponse
	if err := json.NewDecoder(resp.Body).Decode(&response); err != nil {
		return nil, fmt.Errorf("decode mcp response: %w", err)
	}
	if response.Error != nil {
		return nil, fmt.Errorf("mcp error %d: %s", response.Error.Code, response.Error.Message)
	}
	if len(response.Result) == 0 {
		return nil, errors.New("empty mcp result")
	}

	return response.Result, nil
}

func decodeToolResult(raw json.RawMessage, out any) error {
	if len(raw) == 0 {
		return errors.New("empty mcp result")
	}

	var toolResult mcpToolResult
	if err := json.Unmarshal(raw, &toolResult); err == nil && len(toolResult.Content) > 0 {
		return decodeToolContent(toolResult.Content, out)
	}

	if err := json.Unmarshal(raw, out); err == nil {
		return nil
	}

	var text string
	if err := json.Unmarshal(raw, &text); err == nil {
		return json.Unmarshal([]byte(text), out)
	}

	return fmt.Errorf("unsupported mcp result: %s", string(raw))
}

func decodeToolContent(content []mcpToolContent, out any) error {
	for _, item := range content {
		switch strings.ToLower(item.Type) {
		case "json":
			if len(item.JSON) == 0 {
				continue
			}
			if err := json.Unmarshal(item.JSON, out); err == nil {
				return nil
			}
		case "text":
			if strings.TrimSpace(item.Text) == "" {
				continue
			}
			if err := json.Unmarshal([]byte(item.Text), out); err == nil {
				return nil
			}
		}
	}
	return errors.New("unable to decode mcp tool result")
}

func parseMailTime(value string) time.Time {
	if strings.TrimSpace(value) == "" {
		return time.Time{}
	}
	if t, err := time.Parse(time.RFC3339Nano, value); err == nil {
		return t
	}
	if t, err := time.Parse(time.RFC3339, value); err == nil {
		return t
	}
	if t, err := time.Parse("2006-01-02T15:04:05", value); err == nil {
		return t
	}
	return time.Time{}
}
