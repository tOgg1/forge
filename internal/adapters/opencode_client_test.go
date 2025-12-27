package adapters

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestOpenCodeClientSendMessage(t *testing.T) {
	t.Parallel()

	type call struct {
		path string
		body string
	}
	var calls []call

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, _ := io.ReadAll(r.Body)
		calls = append(calls, call{path: r.URL.Path, body: strings.TrimSpace(string(body))})
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(`{}`))
	}))
	defer server.Close()

	client := NewOpenCodeClient(server.URL)
	if err := client.SendMessage(context.Background(), "hello"); err != nil {
		t.Fatalf("SendMessage error: %v", err)
	}

	if len(calls) != 2 {
		t.Fatalf("expected 2 calls, got %d", len(calls))
	}
	if calls[0].path != "/tui/append-prompt" {
		t.Fatalf("expected append-prompt first, got %s", calls[0].path)
	}
	var payload map[string]any
	if err := json.Unmarshal([]byte(calls[0].body), &payload); err != nil {
		t.Fatalf("append-prompt payload invalid: %v", err)
	}
	if payload["text"] != "hello" {
		t.Fatalf("expected text payload to be hello, got %v", payload["text"])
	}
	if calls[1].path != "/tui/submit-prompt" {
		t.Fatalf("expected submit-prompt second, got %s", calls[1].path)
	}
}

func TestOpenCodeClientGetStatus(t *testing.T) {
	t.Parallel()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/session" {
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
		_, _ = w.Write([]byte(`{"state":"idle","session_id":"abc123"}`))
	}))
	defer server.Close()

	client := NewOpenCodeClient(server.URL)
	status, err := client.GetStatus(context.Background())
	if err != nil {
		t.Fatalf("GetStatus error: %v", err)
	}
	if status == nil || status.Data == nil {
		t.Fatalf("expected status data")
	}
	if status.Data["state"] != "idle" {
		t.Fatalf("expected state idle, got %v", status.Data["state"])
	}
}
