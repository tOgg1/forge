package tui

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/url"
	"sort"
	"strings"
	"time"
)

const (
	defaultAgentMailURL          = "http://127.0.0.1:8765/mcp/"
	defaultAgentMailLimit        = 50
	defaultAgentMailPollInterval = 20 * time.Second
	defaultAgentMailTimeout      = 5 * time.Second
)

// AgentMailConfig controls optional Agent Mail integration.
type AgentMailConfig struct {
	URL          string
	Project      string
	Agent        string
	PollInterval time.Duration
	Limit        int
	Timeout      time.Duration
}

func (cfg AgentMailConfig) Enabled() bool {
	return strings.TrimSpace(cfg.Project) != "" && strings.TrimSpace(cfg.Agent) != ""
}

func normalizeAgentMailConfig(cfg AgentMailConfig) AgentMailConfig {
	if strings.TrimSpace(cfg.URL) == "" {
		cfg.URL = defaultAgentMailURL
	}
	if cfg.Limit <= 0 {
		cfg.Limit = defaultAgentMailLimit
	}
	if cfg.PollInterval <= 0 {
		cfg.PollInterval = defaultAgentMailPollInterval
	}
	if cfg.Timeout <= 0 {
		cfg.Timeout = defaultAgentMailTimeout
	}
	return cfg
}

type agentMailClient struct {
	url        string
	project    string
	agent      string
	limit      int
	httpClient *http.Client
}

func newAgentMailClient(cfg AgentMailConfig) *agentMailClient {
	if !cfg.Enabled() {
		return nil
	}
	cfg = normalizeAgentMailConfig(cfg)
	return &agentMailClient{
		url:     cfg.URL,
		project: cfg.Project,
		agent:   cfg.Agent,
		limit:   cfg.Limit,
		httpClient: &http.Client{
			Timeout: cfg.Timeout,
		},
	}
}

type agentMailMessage struct {
	ID        string
	ThreadID  string
	Subject   string
	From      string
	Body      string
	CreatedAt time.Time
}

func (c *agentMailClient) fetchInbox(ctx context.Context) ([]agentMailMessage, error) {
	if c == nil {
		return nil, errors.New("agent mail client not configured")
	}
	if ctx == nil {
		ctx = context.Background()
	}

	uri := inboxResourceURI(c.agent, c.project, c.limit)
	resource, err := c.readResource(ctx, uri)
	if err != nil {
		return nil, err
	}

	var payload agentMailInboxPayload
	if err := json.Unmarshal(resource, &payload); err != nil {
		return nil, fmt.Errorf("parse agent mail inbox: %w", err)
	}

	messages := make([]agentMailMessage, 0, len(payload.Messages))
	for _, msg := range payload.Messages {
		createdAt, _ := parseAgentMailTime(msg.CreatedAt)
		messages = append(messages, agentMailMessage{
			ID:        fmt.Sprintf("%d", msg.ID),
			ThreadID:  msg.ThreadID,
			Subject:   msg.Subject,
			From:      msg.From,
			Body:      msg.Body,
			CreatedAt: createdAt,
		})
	}

	return messages, nil
}

func (c *agentMailClient) readResource(ctx context.Context, uri string) ([]byte, error) {
	request := mcpResourceRequest{
		JSONRPC: "2.0",
		ID:      fmt.Sprintf("tui-mail-%d", time.Now().UnixNano()),
		Method:  "resources/read",
		Params: mcpResourceParams{
			URI: uri,
		},
	}
	payload, err := json.Marshal(request)
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

	var response mcpResourceResponse
	if err := json.NewDecoder(resp.Body).Decode(&response); err != nil {
		return nil, fmt.Errorf("decode mcp response: %w", err)
	}
	if response.Error != nil {
		return nil, fmt.Errorf("mcp error %d: %s", response.Error.Code, response.Error.Message)
	}
	if len(response.Result.Contents) == 0 {
		return nil, errors.New("empty mcp resource response")
	}
	content := response.Result.Contents[0]
	if strings.TrimSpace(content.Text) == "" {
		return nil, errors.New("empty mcp resource content")
	}
	return []byte(content.Text), nil
}

type mcpResourceRequest struct {
	JSONRPC string            `json:"jsonrpc"`
	ID      string            `json:"id"`
	Method  string            `json:"method"`
	Params  mcpResourceParams `json:"params"`
}

type mcpResourceParams struct {
	URI string `json:"uri"`
}

type mcpResourceResponse struct {
	Result mcpResourceResult `json:"result"`
	Error  *mcpResponseError `json:"error"`
}

type mcpResourceResult struct {
	Contents []mcpResourceContent `json:"contents"`
}

type mcpResourceContent struct {
	MimeType string `json:"mimeType"`
	Text     string `json:"text"`
}

type mcpResponseError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

type agentMailInboxPayload struct {
	Messages []agentMailInboxMessage `json:"messages"`
}

type agentMailInboxMessage struct {
	ID        int    `json:"id"`
	ThreadID  string `json:"thread_id"`
	Subject   string `json:"subject"`
	From      string `json:"from"`
	Body      string `json:"body_md"`
	CreatedAt string `json:"created_ts"`
}

func inboxResourceURI(agent, project string, limit int) string {
	agent = strings.TrimSpace(agent)
	project = strings.TrimSpace(project)
	if limit <= 0 {
		limit = defaultAgentMailLimit
	}
	return fmt.Sprintf(
		"resource://inbox/%s?project=%s&limit=%d&include_bodies=true",
		url.PathEscape(agent),
		url.QueryEscape(project),
		limit,
	)
}

func parseAgentMailTime(value string) (time.Time, error) {
	if strings.TrimSpace(value) == "" {
		return time.Time{}, nil
	}
	if parsed, err := time.Parse(time.RFC3339Nano, value); err == nil {
		return parsed, nil
	}
	return time.Parse(time.RFC3339, value)
}

func buildMailThreads(messages []agentMailMessage, readCache map[string]bool) []mailThread {
	threadsByID := make(map[string]*mailThread)
	for _, msg := range messages {
		threadID := strings.TrimSpace(msg.ThreadID)
		if threadID == "" {
			threadID = fmt.Sprintf("msg-%s", msg.ID)
		}
		thread := threadsByID[threadID]
		if thread == nil {
			thread = &mailThread{ID: threadID, Subject: msg.Subject}
			threadsByID[threadID] = thread
		}
		if thread.Subject == "" && msg.Subject != "" {
			thread.Subject = msg.Subject
		}
		read := false
		if readCache != nil {
			read = readCache[msg.ID]
		}
		thread.Messages = append(thread.Messages, mailMessage{
			ID:        msg.ID,
			From:      msg.From,
			Body:      msg.Body,
			CreatedAt: msg.CreatedAt,
			Read:      read,
		})
	}

	threads := make([]mailThread, 0, len(threadsByID))
	for _, thread := range threadsByID {
		sort.Slice(thread.Messages, func(i, j int) bool {
			left := thread.Messages[i]
			right := thread.Messages[j]
			if left.CreatedAt.Equal(right.CreatedAt) {
				return left.ID < right.ID
			}
			return left.CreatedAt.Before(right.CreatedAt)
		})
		threads = append(threads, *thread)
	}

	sort.Slice(threads, func(i, j int) bool {
		left := mailThreadLastMessage(threads[i])
		right := mailThreadLastMessage(threads[j])
		if left == nil && right == nil {
			return threads[i].ID < threads[j].ID
		}
		if left == nil {
			return false
		}
		if right == nil {
			return true
		}
		if left.CreatedAt.Equal(right.CreatedAt) {
			return threads[i].ID < threads[j].ID
		}
		return left.CreatedAt.After(right.CreatedAt)
	})

	return threads
}
