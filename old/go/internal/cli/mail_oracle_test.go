package cli

import (
	"context"
	"database/sql"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"runtime"
	"testing"
	"time"

	_ "modernc.org/sqlite"
)

func TestMailBackendSelectionFixture(t *testing.T) {
	t.Setenv("FORGE_AGENT_MAIL_URL", "")
	t.Setenv("FORGE_AGENT_MAIL_PROJECT", "")
	t.Setenv("FORGE_AGENT_MAIL_AGENT", "")
	t.Setenv("SWARM_AGENT_MAIL_URL", "")
	t.Setenv("SWARM_AGENT_MAIL_PROJECT", "")
	t.Setenv("SWARM_AGENT_MAIL_AGENT", "")

	restore := snapshotMailGlobals()
	defer restore()

	tmp := t.TempDir()
	withWorkingDir(t, tmp, func() {
		summary := []map[string]any{}

		resetForgeMailFlags()
		mailURL = "http://mail.test/mcp/"
		mailProject = "proj-flag"
		mailAgent = "agent-flag"
		cfg, backend, err := resolveMailConfig()
		if err != nil {
			t.Fatalf("resolve explicit flags: %v", err)
		}
		summary = append(summary, map[string]any{
			"scenario":          "explicit_flags",
			"backend":           string(backend),
			"url":               cfg.URL,
			"agent":             cfg.Agent,
			"project_non_empty": cfg.Project != "",
		})

		resetForgeMailFlags()
		t.Setenv("FORGE_AGENT_MAIL_URL", "http://env-mail.test/mcp/")
		t.Setenv("FORGE_AGENT_MAIL_PROJECT", "proj-env")
		t.Setenv("FORGE_AGENT_MAIL_AGENT", "agent-env")

		cfg, backend, err = resolveMailConfig()
		if err != nil {
			t.Fatalf("resolve env config: %v", err)
		}
		summary = append(summary, map[string]any{
			"scenario":          "env_config",
			"backend":           string(backend),
			"url":               cfg.URL,
			"agent":             cfg.Agent,
			"project_non_empty": cfg.Project != "",
		})

		resetForgeMailFlags()
		t.Setenv("FORGE_AGENT_MAIL_URL", "")
		t.Setenv("FORGE_AGENT_MAIL_PROJECT", "")
		t.Setenv("FORGE_AGENT_MAIL_AGENT", "")

		cfg, backend, err = resolveMailConfig()
		if err != nil {
			t.Fatalf("resolve local fallback: %v", err)
		}
		summary = append(summary, map[string]any{
			"scenario":          "local_fallback",
			"backend":           string(backend),
			"url":               cfg.URL,
			"agent":             cfg.Agent,
			"project_non_empty": cfg.Project != "",
		})

		assertMailFixture(t, "mail_backend_selection.json", summary)
	})
}

func TestMailLocalStoreIntegrationFixture(t *testing.T) {
	dbConn, err := sql.Open("sqlite", ":memory:")
	if err != nil {
		t.Fatalf("open sqlite: %v", err)
	}
	defer dbConn.Close()

	store := &mailStore{db: dbConn}
	if err := store.ensureSchema(context.Background()); err != nil {
		t.Fatalf("ensure schema: %v", err)
	}

	req := mailSendRequest{
		Project:     "proj-local",
		From:        "sender-a",
		To:          []string{"agent-1", "agent-2"},
		Subject:     "handoff",
		Body:        "please review",
		Priority:    "high",
		AckRequired: true,
	}
	ids, err := store.SendLocal(context.Background(), req)
	if err != nil {
		t.Fatalf("send local: %v", err)
	}

	inboxOne, err := store.ListLocal(context.Background(), "proj-local", "agent-1", nil, false, 10)
	if err != nil {
		t.Fatalf("list local agent-1: %v", err)
	}
	inboxTwo, err := store.ListLocal(context.Background(), "proj-local", "agent-2", nil, false, 10)
	if err != nil {
		t.Fatalf("list local agent-2: %v", err)
	}

	first, err := store.GetLocal(context.Background(), "proj-local", "agent-1", ids[0])
	if err != nil {
		t.Fatalf("get local: %v", err)
	}

	now := time.Date(2026, 2, 9, 12, 0, 0, 0, time.UTC)
	if err := store.MarkRead(context.Background(), "proj-local", "agent-1", ids[0], now); err != nil {
		t.Fatalf("mark read: %v", err)
	}
	if err := store.MarkAck(context.Background(), "proj-local", "agent-1", ids[0], now.Add(5*time.Minute)); err != nil {
		t.Fatalf("mark ack: %v", err)
	}
	statuses, err := store.LoadStatus(context.Background(), "proj-local", "agent-1", []int64{ids[0]})
	if err != nil {
		t.Fatalf("load status: %v", err)
	}

	summary := map[string]any{
		"sent_ids_count": len(ids),
		"inbox_counts": map[string]any{
			"agent_1": len(inboxOne),
			"agent_2": len(inboxTwo),
		},
		"first_message": map[string]any{
			"subject":      first.Subject,
			"from":         first.From,
			"ack_required": first.AckRequired,
			"importance":   first.Importance,
		},
		"status_marked": map[string]any{
			"read": statuses[ids[0]].ReadAt != nil,
			"ack":  statuses[ids[0]].AckedAt != nil,
		},
	}

	assertMailFixture(t, "mail_local_flow.json", summary)
}

func TestMailMCPClientIntegrationFixture(t *testing.T) {
	var calls []string

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		defer r.Body.Close()

		var req mcpRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			t.Fatalf("decode request: %v", err)
		}

		var params mcpToolCallParams
		raw, err := json.Marshal(req.Params)
		if err != nil {
			t.Fatalf("marshal params: %v", err)
		}
		if err := json.Unmarshal(raw, &params); err != nil {
			t.Fatalf("decode tool params: %v", err)
		}
		calls = append(calls, params.Name)

		writeResult := func(payload any) {
			res := map[string]any{
				"result": map[string]any{
					"content": []map[string]any{
						{"type": "json", "json": payload},
					},
				},
			}
			if err := json.NewEncoder(w).Encode(res); err != nil {
				t.Fatalf("encode response: %v", err)
			}
		}

		switch params.Name {
		case "send_message":
			writeResult(map[string]any{"ok": true})
		case "fetch_inbox":
			writeResult([]map[string]any{
				{
					"id":           101,
					"thread_id":    "t-1",
					"subject":      "hello",
					"from":         "agent-a",
					"body_md":      "payload",
					"created_ts":   "2026-02-09T10:00:00Z",
					"importance":   "high",
					"ack_required": true,
				},
			})
		case "mark_message_read", "acknowledge_message":
			writeResult(map[string]any{"ok": true})
		default:
			t.Fatalf("unexpected tool call: %s", params.Name)
		}
	}))
	defer server.Close()

	client := newMailMCPClient(mailConfig{
		URL:     server.URL,
		Project: "proj-mcp",
		Agent:   "agent-target",
		Limit:   10,
		Timeout: 2 * time.Second,
	})

	if err := client.SendMessage(context.Background(), mailSendRequest{
		Project:  "proj-mcp",
		From:     "sender-a",
		To:       []string{"agent-target"},
		Subject:  "subj",
		Body:     "body",
		Priority: "normal",
	}); err != nil {
		t.Fatalf("send mcp: %v", err)
	}

	inbox, err := client.FetchInbox(context.Background(), mailInboxRequest{
		Project: "proj-mcp",
		Agent:   "agent-target",
		Limit:   10,
	})
	if err != nil {
		t.Fatalf("fetch inbox: %v", err)
	}

	read, err := client.ReadMessage(context.Background(), mailReadRequest{
		Project:   "proj-mcp",
		Agent:     "agent-target",
		MessageID: 101,
		Limit:     10,
	})
	if err != nil {
		t.Fatalf("read message: %v", err)
	}

	if err := client.MarkRead(context.Background(), mailStatusRequest{
		Project:   "proj-mcp",
		Agent:     "agent-target",
		MessageID: 101,
	}); err != nil {
		t.Fatalf("mark read: %v", err)
	}
	if err := client.Acknowledge(context.Background(), mailStatusRequest{
		Project:   "proj-mcp",
		Agent:     "agent-target",
		MessageID: 101,
	}); err != nil {
		t.Fatalf("ack: %v", err)
	}

	summary := map[string]any{
		"call_sequence": calls,
		"inbox_count":   len(inbox),
		"read_message": map[string]any{
			"id":      read.ID,
			"subject": read.Subject,
			"from":    read.From,
			"backend": read.Backend,
		},
	}

	assertMailFixture(t, "mail_mcp_flow.json", summary)
}

func assertMailFixture(t *testing.T, name string, got any) {
	t.Helper()

	want := readMailFixture(t, name)
	gotJSON, err := json.MarshalIndent(got, "", "  ")
	if err != nil {
		t.Fatalf("marshal got: %v", err)
	}
	gotText := string(gotJSON)
	if normalizeGolden(want) != normalizeGolden(gotText) {
		t.Fatalf("mail fixture drift (%s)\nwant:\n%s\ngot:\n%s", name, want, gotText)
	}
}

func readMailFixture(t *testing.T, name string) string {
	t.Helper()

	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatalf("resolve test path")
	}
	path := filepath.Join(filepath.Dir(file), "testdata", "oracle", name)
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read fixture %s: %v", name, err)
	}
	return string(data)
}

func snapshotMailGlobals() func() {
	prev := struct {
		mailURL     string
		mailProject string
		mailAgent   string
		mailLimit   int
		mailTimeout time.Duration
	}{
		mailURL:     mailURL,
		mailProject: mailProject,
		mailAgent:   mailAgent,
		mailLimit:   mailLimit,
		mailTimeout: mailTimeout,
	}

	return func() {
		mailURL = prev.mailURL
		mailProject = prev.mailProject
		mailAgent = prev.mailAgent
		mailLimit = prev.mailLimit
		mailTimeout = prev.mailTimeout
	}
}
