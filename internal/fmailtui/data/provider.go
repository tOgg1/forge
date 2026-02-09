package data

import (
	"fmt"
	"path/filepath"
	"strings"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
)

const (
	defaultCacheTTL            = 500 * time.Millisecond
	defaultMetadataTTL         = 5 * time.Second
	defaultPollInterval        = 100 * time.Millisecond
	defaultPollMax             = 2 * time.Second
	defaultReconnectInterval   = 2 * time.Second
	defaultSubscribeBufferSize = 256
	defaultMessageCacheSize    = 2048
	defaultForgedAgent         = "tui-viewer"
)

// MessageProvider abstracts message access for the fmail TUI.
type MessageProvider interface {
	// Topics lists topics with metadata.
	Topics() ([]TopicInfo, error)
	// Messages lists messages in a topic with optional filters.
	Messages(topic string, opts MessageFilter) ([]fmail.Message, error)
	// DMConversations lists DM conversations for a given agent.
	DMConversations(agent string) ([]DMConversation, error)
	// DMs lists DMs with a specific agent.
	DMs(agent string, opts MessageFilter) ([]fmail.Message, error)
	// Agents lists all known agents.
	Agents() ([]fmail.AgentRecord, error)
	// Search searches across messages.
	Search(query SearchQuery) ([]SearchResult, error)
	// Subscribe streams new messages and returns a cancel function.
	Subscribe(filter SubscriptionFilter) (<-chan fmail.Message, func())
}

type TopicInfo struct {
	Name         string
	MessageCount int
	LastActivity time.Time
	Participants []string
	LastMessage  *fmail.Message
}

type DMConversation struct {
	Agent        string
	MessageCount int
	LastActivity time.Time
	UnreadCount  int
}

type MessageFilter struct {
	Since    time.Time
	Until    time.Time
	From     string
	Priority string
	Tags     []string
	Limit    int
	To       string
}

type SubscriptionFilter struct {
	Topic     string
	Agent     string
	Since     time.Time
	SinceID   string
	From      string
	Priority  string
	Tags      []string
	IncludeDM bool
}

type SearchQuery struct {
	Text     string
	From     string
	To       string
	In       string // scope search to a single topic or "@agent"
	Priority string
	Tags     []string
	Since    time.Time
	Until    time.Time
	HasReply bool
	// View-level filters (provider may ignore them).
	HasBookmark   bool
	HasAnnotation bool
	IsUnread      bool
}

type SearchResult struct {
	Message     fmail.Message
	Topic       string
	MatchOffset int
	MatchLength int
	Prev        *fmail.Message // optional context message before
	Next        *fmail.Message // optional context message after
}

type SendRequest struct {
	From     string
	To       string
	Body     string
	ReplyTo  string
	Priority string
	Tags     []string
	Time     time.Time
}

type FileProviderConfig struct {
	Root     string
	CacheTTL time.Duration
	// MetadataTTL controls topic/DM metadata index refresh frequency.
	MetadataTTL time.Duration
	// PollInterval controls the minimum poll cadence used by Subscribe().
	PollInterval time.Duration
	// PollMax controls the maximum poll cadence used by Subscribe() backoff.
	PollMax         time.Duration
	CacheCapacity   int
	SubscribeBuffer int
	SelfAgent       string
}

type ForgedProviderConfig struct {
	Root              string
	Addr              string
	Agent             string
	DialTimeout       time.Duration
	ReconnectInterval time.Duration
	Fallback          *FileProvider
	SubscribeBuffer   int
}

type HybridProviderConfig struct {
	Root              string
	ReconnectInterval time.Duration
	SubscribeBuffer   int
	FileProvider      *FileProvider
	ForgedProvider    *ForgedProvider
}

func normalizeRoot(root string) (string, error) {
	trimmed := strings.TrimSpace(root)
	if trimmed == "" {
		return "", fmt.Errorf("root required")
	}
	return filepath.Abs(trimmed)
}
