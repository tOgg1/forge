package cli

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"testing"
	"time"
)

type oracleForgeMailReport struct {
	Steps []oracleForgeMailStep `json:"steps"`
}

type oracleForgeMailStep struct {
	Name   string `json:"name"`
	Stdout string `json:"stdout,omitempty"`
	Stderr string `json:"stderr,omitempty"`
	Error  string `json:"error,omitempty"`
}

func TestOracleForgeMailFixtures(t *testing.T) {
	if testing.Short() {
		t.Skip("oracle fixtures are integration-style; skip in -short")
	}

	repo := t.TempDir()
	cleanupConfig := withTempConfig(t, repo)
	defer cleanupConfig()

	withWorkingDir(t, repo, func() {
		restoreGlobals := snapshotCLIFlags()
		defer restoreGlobals()
		restoreMail := snapshotForgeMailGlobals()
		defer restoreMail()

		// Keep deterministic; avoid env-driven backend selection.
		t.Setenv("FORGE_AGENT_MAIL_URL", "")
		t.Setenv("FORGE_AGENT_MAIL_PROJECT", "")
		t.Setenv("FORGE_AGENT_MAIL_AGENT", "")
		t.Setenv("SWARM_AGENT_MAIL_URL", "")
		t.Setenv("SWARM_AGENT_MAIL_PROJECT", "")
		t.Setenv("SWARM_AGENT_MAIL_AGENT", "")

		jsonOutput = true
		jsonlOutput = false
		noColor = true
		quiet = true
		yesFlag = true
		nonInteractive = true

		var report oracleForgeMailReport

		// 1) local backend send (no MCP config).
		resetForgeMailFlags()
		mailFrom = "oracle-sender"
		mailTo = []string{"oracle-recipient"}
		mailSubject = "oracle subject"
		mailBody = "oracle body"
		mailPriority = "normal"
		stdout, stderr, runErr := captureStdoutStderr(func() error { return mailSendCmd.RunE(mailSendCmd, nil) })
		if runErr != nil {
			t.Fatalf("mail send local: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleForgeMailStep{
			Name:   "local send",
			Stdout: normalizeForgeMailJSONText(t, stdout, repo),
			Stderr: strings.TrimSpace(stderr),
		})

		// 2) backend selection fixture: --agent (or env) implies MCP backend.
		resetForgeMailFlags()
		mailAgent = "oracle-recipient"
		cfg, backend, err := resolveMailConfig()
		if err != nil {
			t.Fatalf("resolve mail config: %v", err)
		}
		selection := map[string]any{
			"backend":  backend,
			"url":      cfg.URL,
			"project":  cfg.Project,
			"agent":    cfg.Agent,
			"limit":    cfg.Limit,
			"timeout":  cfg.Timeout.String(),
			"has_git":  true, // document: project key detection uses git root when present
		}
		selectionJSON, _ := json.Marshal(selection)
		report.Steps = append(report.Steps, oracleForgeMailStep{
			Name:   "backend selection",
			Stdout: normalizeForgeMailJSONText(t, string(selectionJSON), repo),
		})

		// 3) MCP backend fixtures with mocked server.
		server := newOracleMCPServer()
		httpServer := httptest.NewServer(server)
		t.Cleanup(httpServer.Close)

		// 3a) mcp send
		resetForgeMailFlags()
		mailURL = httpServer.URL
		mailProject = "oracle-project"
		mailFrom = "oracle-sender"
		mailTo = []string{"oracle-recipient"}
		mailSubject = "mcp subject"
		mailBody = "mcp body"
		mailPriority = "high"
		mailAckRequired = true
		stdout, stderr, runErr = captureStdoutStderr(func() error { return mailSendCmd.RunE(mailSendCmd, nil) })
		if runErr != nil {
			t.Fatalf("mail send mcp: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleForgeMailStep{
			Name:   "mcp send",
			Stdout: normalizeForgeMailJSONText(t, stdout, repo),
			Stderr: strings.TrimSpace(stderr),
		})

		// 3b) mcp inbox
		resetForgeMailFlags()
		mailURL = httpServer.URL
		mailProject = "oracle-project"
		mailAgent = "oracle-recipient"
		stdout, stderr, runErr = captureStdoutStderr(func() error { return mailInboxCmd.RunE(mailInboxCmd, nil) })
		if runErr != nil {
			t.Fatalf("mail inbox mcp: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleForgeMailStep{
			Name:   "mcp inbox",
			Stdout: normalizeForgeMailJSONText(t, stdout, repo),
			Stderr: strings.TrimSpace(stderr),
		})

		// 3c) mcp read (marks read + persists status).
		resetForgeMailFlags()
		mailURL = httpServer.URL
		mailProject = "oracle-project"
		mailAgent = "oracle-recipient"
		stdout, stderr, runErr = captureStdoutStderr(func() error { return mailReadCmd.RunE(mailReadCmd, []string{"m-1"}) })
		if runErr != nil {
			t.Fatalf("mail read mcp: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleForgeMailStep{
			Name:   "mcp read",
			Stdout: normalizeForgeMailJSONText(t, stdout, repo),
			Stderr: strings.TrimSpace(stderr),
		})

		// 3d) mcp ack
		resetForgeMailFlags()
		mailURL = httpServer.URL
		mailProject = "oracle-project"
		mailAgent = "oracle-recipient"
		stdout, stderr, runErr = captureStdoutStderr(func() error { return mailAckCmd.RunE(mailAckCmd, []string{"m-1"}) })
		if runErr != nil {
			t.Fatalf("mail ack mcp: %v\nstderr:\n%s", runErr, stderr)
		}
		report.Steps = append(report.Steps, oracleForgeMailStep{
			Name:   "mcp ack",
			Stdout: normalizeForgeMailJSONText(t, stdout, repo),
			Stderr: strings.TrimSpace(stderr),
		})

		got := mustMarshalJSON(t, report)
		goldenPath := oracleForgeMailGoldenPath(t)

		if os.Getenv("FORGE_UPDATE_GOLDENS") == "1" {
			if err := os.MkdirAll(filepath.Dir(goldenPath), 0o755); err != nil {
				t.Fatalf("mkdir golden dir: %v", err)
			}
			if err := os.WriteFile(goldenPath, []byte(got), 0o644); err != nil {
				t.Fatalf("write golden: %v", err)
			}
			return
		}

		wantBytes, err := os.ReadFile(goldenPath)
		if err != nil {
			t.Fatalf("read golden: %v (set FORGE_UPDATE_GOLDENS=1 to generate)", err)
		}
		want := string(wantBytes)
		if normalizeGolden(want) != normalizeGolden(got) {
			t.Fatalf("oracle fixture drift: %s (set FORGE_UPDATE_GOLDENS=1 to regenerate)\n--- want\n%s\n--- got\n%s", goldenPath, want, got)
		}
	})
}

func resetForgeMailFlags() {
	mailURL = ""
	mailProject = ""
	mailAgent = ""
	mailLimit = 0
	mailTimeout = 0

	mailTo = nil
	mailSubject = ""
	mailBody = ""
	mailFile = ""
	mailStdin = false
	mailPriority = "normal"
	mailAckRequired = false
	mailFrom = ""

	mailUnread = false
	sinceDur = ""
}

func snapshotForgeMailGlobals() func() {
	prev := struct {
		mailURL         string
		mailProject     string
		mailAgent       string
		mailLimit       int
		mailTimeout     time.Duration
		mailTo          []string
		mailSubject     string
		mailBody        string
		mailFile        string
		mailStdin       bool
		mailPriority    string
		mailAckRequired bool
		mailFrom        string
		mailUnread      bool
		sinceDur        string
	}{
		mailURL:         mailURL,
		mailProject:     mailProject,
		mailAgent:       mailAgent,
		mailLimit:       mailLimit,
		mailTimeout:     mailTimeout,
		mailTo:          append([]string(nil), mailTo...),
		mailSubject:     mailSubject,
		mailBody:        mailBody,
		mailFile:        mailFile,
		mailStdin:       mailStdin,
		mailPriority:    mailPriority,
		mailAckRequired: mailAckRequired,
		mailFrom:        mailFrom,
		mailUnread:      mailUnread,
		sinceDur:        sinceDur,
	}

	return func() {
		mailURL = prev.mailURL
		mailProject = prev.mailProject
		mailAgent = prev.mailAgent
		mailLimit = prev.mailLimit
		mailTimeout = prev.mailTimeout
		mailTo = append([]string(nil), prev.mailTo...)
		mailSubject = prev.mailSubject
		mailBody = prev.mailBody
		mailFile = prev.mailFile
		mailStdin = prev.mailStdin
		mailPriority = prev.mailPriority
		mailAckRequired = prev.mailAckRequired
		mailFrom = prev.mailFrom
		mailUnread = prev.mailUnread
		sinceDur = prev.sinceDur
	}
}

func normalizeForgeMailJSONText(t *testing.T, raw string, repo string) string {
	t.Helper()
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return ""
	}

	var v any
	if err := json.Unmarshal([]byte(raw), &v); err != nil {
		t.Fatalf("unmarshal json: %v\nraw:\n%s", err, raw)
	}
	v = normalizeForgeMailJSONValue(v, repo)
	out, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		t.Fatalf("marshal json: %v", err)
	}
	return string(out) + "\n"
}

func normalizeForgeMailJSONValue(v any, repo string) any {
	switch vv := v.(type) {
	case map[string]any:
		out := make(map[string]any, len(vv))
		for k, val := range vv {
			switch k {
			case "created_at", "read_at", "acked_at", "since_ts":
				out[k] = "<TIME>"
			case "project":
				if s, ok := val.(string); ok && (strings.Contains(s, string(os.PathSeparator)) || strings.HasPrefix(s, "/")) {
					// Local backend uses absolute paths for project key; normalize.
					out[k] = "<PROJECT>"
					continue
				}
				out[k] = normalizeForgeMailJSONValue(val, repo)
			default:
				out[k] = normalizeForgeMailJSONValue(val, repo)
			}
		}
		return out
	case []any:
		out := make([]any, 0, len(vv))
		for _, item := range vv {
			out = append(out, normalizeForgeMailJSONValue(item, repo))
		}
		return out
	case string:
		// Normalize any temp repo absolute paths embedded in output.
		if repo != "" && strings.Contains(vv, repo) {
			return strings.ReplaceAll(vv, repo, "<PROJECT>")
		}
		return vv
	default:
		return v
	}
}

func oracleForgeMailGoldenPath(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatalf("resolve test file path")
	}
	base := filepath.Dir(file)
	return filepath.Join(base, "testdata", "oracle", "forge_mail.json")
}

type oracleMCPServer struct {
	mu       sync.Mutex
	nextID   int64
	messages map[string]map[string][]mcpInboxMessage // project -> agent -> messages
}

func newOracleMCPServer() *oracleMCPServer {
	return &oracleMCPServer{
		nextID:   1,
		messages: map[string]map[string][]mcpInboxMessage{},
	}
}

func (s *oracleMCPServer) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	var req mcpRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	var params mcpToolCallParams
	if err := decodeToolCallParams(req.Params, &params); err != nil {
		writeMCPError(w, req.ID, -32602, err.Error())
		return
	}

	switch params.Name {
	case "send_message":
		var args struct {
			ProjectKey   string   `json:"project_key"`
			SenderName   string   `json:"sender_name"`
			To           []string `json:"to"`
			Subject      string   `json:"subject"`
			BodyMD       string   `json:"body_md"`
			Importance   string   `json:"importance"`
			AckRequired  bool     `json:"ack_required"`
			IncludeBodies bool    `json:"include_bodies"`
		}
		_ = jsonMarshalInto(params.Arguments, &args)

		s.mu.Lock()
		defer s.mu.Unlock()
		if _, ok := s.messages[args.ProjectKey]; !ok {
			s.messages[args.ProjectKey] = map[string][]mcpInboxMessage{}
		}

		created := time.Date(2026, 2, 9, 0, 0, 0, 0, time.UTC).Format(time.RFC3339Nano)
		for _, agent := range args.To {
			msg := mcpInboxMessage{
				ID:          s.nextID,
				ThreadID:    "",
				Subject:     args.Subject,
				From:        args.SenderName,
				Body:        args.BodyMD,
				CreatedAt:   created,
				Importance:  args.Importance,
				AckRequired: args.AckRequired,
			}
			s.nextID++
			s.messages[args.ProjectKey][agent] = append(s.messages[args.ProjectKey][agent], msg)
		}

		writeMCPResult(w, req.ID, map[string]any{"ok": true})
	case "fetch_inbox":
		var args struct {
			ProjectKey     string `json:"project_key"`
			AgentName      string `json:"agent_name"`
			IncludeBodies  bool   `json:"include_bodies"`
			Limit          int    `json:"limit"`
			SinceTS        string `json:"since_ts"`
		}
		_ = jsonMarshalInto(params.Arguments, &args)

		s.mu.Lock()
		items := append([]mcpInboxMessage(nil), s.messages[args.ProjectKey][args.AgentName]...)
		s.mu.Unlock()

		if !args.IncludeBodies {
			for i := range items {
				items[i].Body = ""
			}
		}

		writeMCPResult(w, req.ID, items)
	case "mark_message_read", "acknowledge_message":
		writeMCPResult(w, req.ID, map[string]any{"ok": true})
	default:
		writeMCPError(w, req.ID, -32601, fmt.Sprintf("unknown tool: %s", params.Name))
	}
}

func decodeToolCallParams(raw any, out *mcpToolCallParams) error {
	data, err := json.Marshal(raw)
	if err != nil {
		return err
	}
	return json.Unmarshal(data, out)
}

func jsonMarshalInto(raw any, out any) error {
	data, err := json.Marshal(raw)
	if err != nil {
		return err
	}
	return json.Unmarshal(data, out)
}

func writeMCPResult(w http.ResponseWriter, id string, payload any) {
	w.Header().Set("Content-Type", "application/json")
	result := mcpToolResult{
		Content: []mcpToolContent{
			{
				Type: "json",
				JSON: mustJSON(payload),
			},
		},
	}
	resp := map[string]any{
		"jsonrpc": "2.0",
		"id":      id,
		"result":  result,
	}
	_ = json.NewEncoder(w).Encode(resp)
}

func writeMCPError(w http.ResponseWriter, id string, code int, message string) {
	w.Header().Set("Content-Type", "application/json")
	resp := map[string]any{
		"jsonrpc": "2.0",
		"id":      id,
		"error": map[string]any{
			"code":    code,
			"message": message,
		},
	}
	_ = json.NewEncoder(w).Encode(resp)
}

func mustJSON(v any) json.RawMessage {
	data, _ := json.Marshal(v)
	return data
}
