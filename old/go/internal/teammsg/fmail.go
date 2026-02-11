package teammsg

import (
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
)

type Messenger interface {
	// SendTask sends a direct message to an agent (DM).
	SendTask(toAgent string, body string) error
	// SendTopic sends a topic message (broadcast).
	SendTopic(topic string, body string) error
}

type noopMessenger struct{}

func (noopMessenger) SendTask(string, string) error  { return nil }
func (noopMessenger) SendTopic(string, string) error { return nil }

type storeSaver interface {
	SaveMessage(message *fmail.Message) (string, error)
}

type fmailMessenger struct {
	from string
	save storeSaver
}

func (m *fmailMessenger) SendTask(toAgent string, body string) error {
	agent := strings.TrimSpace(toAgent)
	if agent == "" {
		return fmt.Errorf("missing to agent")
	}
	if !strings.HasPrefix(agent, "@") {
		agent = "@" + agent
	}
	return m.send(agent, body)
}

func (m *fmailMessenger) SendTopic(topic string, body string) error {
	topic = strings.TrimSpace(topic)
	if topic == "" {
		return fmt.Errorf("missing topic")
	}
	if strings.HasPrefix(topic, "@") {
		return fmt.Errorf("topic cannot start with @: %s", topic)
	}
	return m.send(topic, body)
}

func (m *fmailMessenger) send(target string, body string) error {
	if m == nil || m.save == nil {
		return fmt.Errorf("not configured")
	}
	if strings.TrimSpace(m.from) == "" {
		return fmt.Errorf("missing from")
	}
	body = strings.TrimSpace(body)
	if body == "" {
		return fmt.Errorf("empty body")
	}
	normalizedTarget, _, err := fmail.NormalizeTarget(target)
	if err != nil {
		return err
	}
	msg := &fmail.Message{
		From: m.from,
		To:   normalizedTarget,
		Body: body,
		Time: time.Now().UTC(),
	}
	_, err = m.save.SaveMessage(msg)
	return err
}

// NewFromEnv returns a messenger using the local fmail store when configured.
// Configuration:
// - FMAIL_AGENT required to enable.
// - FMAIL_ROOT optional (fmail DiscoverProjectRoot fallback).
func NewFromEnv(startDir string) (Messenger, error) {
	from := strings.TrimSpace(os.Getenv("FMAIL_AGENT"))
	if from == "" {
		return noopMessenger{}, nil
	}

	root, err := fmail.DiscoverProjectRoot(startDir)
	if err != nil {
		return noopMessenger{}, err
	}
	store, err := fmail.NewStore(root)
	if err != nil {
		return noopMessenger{}, err
	}
	if err := store.EnsureRoot(); err != nil {
		return noopMessenger{}, err
	}
	return &fmailMessenger{from: from, save: store}, nil
}
