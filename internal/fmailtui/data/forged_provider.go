package data

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
)

const forgedTCPAddr = "127.0.0.1:7463"

type ForgedProvider struct {
	root              string
	addr              string
	agent             string
	dialTimeout       time.Duration
	reconnectInterval time.Duration
	subscribeBuffer   int
	fallback          *FileProvider
}

type forgedWatchRequest struct {
	Cmd   string `json:"cmd"`
	Topic string `json:"topic,omitempty"`
	Agent string `json:"agent"`
	Since string `json:"since,omitempty"`
}

type forgedError struct {
	Code      string `json:"code"`
	Message   string `json:"message"`
	Retryable bool   `json:"retryable,omitempty"`
}

type forgedWatchAck struct {
	OK    bool         `json:"ok"`
	Error *forgedError `json:"error,omitempty"`
}

type forgedWatchEnvelope struct {
	OK    *bool          `json:"ok,omitempty"`
	Error *forgedError   `json:"error,omitempty"`
	Msg   *fmail.Message `json:"msg,omitempty"`
}

func NewForgedProvider(cfg ForgedProviderConfig) (*ForgedProvider, error) {
	root, err := normalizeRoot(cfg.Root)
	if err != nil {
		return nil, err
	}

	fallback := cfg.Fallback
	if fallback == nil {
		fallback, err = NewFileProvider(FileProviderConfig{Root: root})
		if err != nil {
			return nil, err
		}
	}

	agent := strings.TrimSpace(cfg.Agent)
	if agent == "" {
		agent = defaultForgedAgent
	}
	normalized, err := fmail.NormalizeAgentName(agent)
	if err != nil {
		return nil, fmt.Errorf("normalize agent: %w", err)
	}
	agent = normalized

	dialTimeout := cfg.DialTimeout
	if dialTimeout <= 0 {
		dialTimeout = 200 * time.Millisecond
	}
	reconnectInterval := cfg.ReconnectInterval
	if reconnectInterval <= 0 {
		reconnectInterval = defaultReconnectInterval
	}
	subscribeBuffer := cfg.SubscribeBuffer
	if subscribeBuffer <= 0 {
		subscribeBuffer = defaultSubscribeBufferSize
	}

	return &ForgedProvider{
		root:              root,
		addr:              strings.TrimSpace(cfg.Addr),
		agent:             agent,
		dialTimeout:       dialTimeout,
		reconnectInterval: reconnectInterval,
		subscribeBuffer:   subscribeBuffer,
		fallback:          fallback,
	}, nil
}

func (p *ForgedProvider) Topics() ([]TopicInfo, error) {
	return p.fallback.Topics()
}

func (p *ForgedProvider) Messages(topic string, opts MessageFilter) ([]fmail.Message, error) {
	return p.fallback.Messages(topic, opts)
}

func (p *ForgedProvider) DMConversations(agent string) ([]DMConversation, error) {
	return p.fallback.DMConversations(agent)
}

func (p *ForgedProvider) DMs(agent string, opts MessageFilter) ([]fmail.Message, error) {
	return p.fallback.DMs(agent, opts)
}

func (p *ForgedProvider) Agents() ([]fmail.AgentRecord, error) {
	return p.fallback.Agents()
}

func (p *ForgedProvider) Search(query SearchQuery) ([]SearchResult, error) {
	return p.fallback.Search(query)
}

func (p *ForgedProvider) Subscribe(filter SubscriptionFilter) (<-chan fmail.Message, func()) {
	ctx, cancel := context.WithCancel(context.Background())
	out := make(chan fmail.Message, p.subscribeBuffer)
	go p.subscribeLoop(ctx, out, filter)
	return out, cancel
}

func (p *ForgedProvider) subscribeLoop(ctx context.Context, out chan<- fmail.Message, filter SubscriptionFilter) {
	defer close(out)
	lastSeenID := strings.TrimSpace(filter.SinceID)

	for {
		if ctx.Err() != nil {
			return
		}

		if err := p.streamWatch(ctx, filter, &lastSeenID, out); err == nil || ctx.Err() != nil {
			return
		}

		fallbackFilter := filter
		fallbackFilter.SinceID = lastSeenID
		fallbackCh, fallbackCancel := p.fallback.Subscribe(fallbackFilter)
		reconnectTicker := time.NewTicker(p.reconnectInterval)

	fallbackLoop:
		for {
			select {
			case <-ctx.Done():
				reconnectTicker.Stop()
				fallbackCancel()
				return
			case msg, ok := <-fallbackCh:
				if !ok {
					break fallbackLoop
				}
				if msg.ID != "" && msg.ID > lastSeenID {
					lastSeenID = msg.ID
				}
				select {
				case <-ctx.Done():
					reconnectTicker.Stop()
					fallbackCancel()
					return
				case out <- cloneMessage(msg):
				}
			case <-reconnectTicker.C:
				if p.canConnect(ctx) {
					reconnectTicker.Stop()
					fallbackCancel()
					break fallbackLoop
				}
			}
		}
	}
}

func (p *ForgedProvider) streamWatch(ctx context.Context, filter SubscriptionFilter, lastSeenID *string, out chan<- fmail.Message) error {
	conn, err := p.dial(ctx)
	if err != nil {
		return err
	}
	defer conn.Close()

	done := make(chan struct{})
	defer close(done)
	go func() {
		select {
		case <-ctx.Done():
			_ = conn.Close()
		case <-done:
		}
	}()

	reader := bufio.NewReader(conn)
	writer := bufio.NewWriter(conn)
	req := forgedWatchRequest{
		Cmd:   "watch",
		Topic: watchTopic(filter),
		Agent: p.agent,
		Since: watchSince(*lastSeenID, filter.Since),
	}
	if err := writeJSONLine(writer, req); err != nil {
		return err
	}

	ackLine, err := readForgedLine(reader)
	if err != nil {
		return err
	}
	var ack forgedWatchAck
	if err := json.Unmarshal(ackLine, &ack); err != nil {
		return fmt.Errorf("invalid forged ack: %w", err)
	}
	if !ack.OK {
		return fmt.Errorf("forged watch rejected: %s", formatForgedErr(ack.Error))
	}

	for {
		line, err := readForgedLine(reader)
		if err != nil {
			return err
		}
		if len(line) == 0 {
			continue
		}

		var env forgedWatchEnvelope
		if err := json.Unmarshal(line, &env); err != nil {
			return fmt.Errorf("invalid forged stream data: %w", err)
		}
		if env.OK != nil && !*env.OK {
			return fmt.Errorf("forged stream error: %s", formatForgedErr(env.Error))
		}
		if env.Msg == nil {
			continue
		}

		message := cloneMessage(*env.Msg)
		if !messageMatchesSubscription(message, filter) {
			continue
		}
		if message.ID != "" && message.ID > *lastSeenID {
			*lastSeenID = message.ID
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case out <- message:
		}
	}
}

func watchTopic(filter SubscriptionFilter) string {
	topic := strings.TrimSpace(filter.Topic)
	if topic == "" {
		return "*"
	}
	return topic
}

func watchSince(lastSeenID string, since time.Time) string {
	id := strings.TrimSpace(lastSeenID)
	if id != "" {
		return id
	}
	if since.IsZero() {
		return ""
	}
	return since.UTC().Format(time.RFC3339Nano)
}

func formatForgedErr(err *forgedError) string {
	if err == nil {
		return "unknown error"
	}
	message := strings.TrimSpace(err.Message)
	if message == "" {
		message = strings.TrimSpace(err.Code)
	}
	if message == "" {
		message = "unknown error"
	}
	if strings.TrimSpace(err.Code) == "" || strings.Contains(message, err.Code) {
		return message
	}
	return fmt.Sprintf("%s (%s)", message, err.Code)
}

func writeJSONLine(writer *bufio.Writer, payload any) error {
	data, err := json.Marshal(payload)
	if err != nil {
		return err
	}
	if _, err := writer.Write(data); err != nil {
		return err
	}
	if err := writer.WriteByte('\n'); err != nil {
		return err
	}
	return writer.Flush()
}

func readForgedLine(reader *bufio.Reader) ([]byte, error) {
	line, err := reader.ReadBytes('\n')
	if err != nil {
		if errors.Is(err, io.EOF) && len(line) > 0 {
			return bytes.TrimSpace(line), nil
		}
		return nil, err
	}
	if len(line) > fmail.MaxMessageSize+64*1024 {
		return nil, fmt.Errorf("forged line too long")
	}
	return bytes.TrimSpace(line), nil
}

func (p *ForgedProvider) canConnect(parent context.Context) bool {
	ctx, cancel := context.WithTimeout(parent, p.dialTimeout)
	defer cancel()
	conn, err := p.dial(ctx)
	if err != nil {
		return false
	}
	_ = conn.Close()
	return true
}

func (p *ForgedProvider) dial(ctx context.Context) (net.Conn, error) {
	dialer := &net.Dialer{Timeout: p.dialTimeout}
	addr := strings.TrimSpace(p.addr)

	if addr != "" {
		network := "tcp"
		target := addr
		if looksLikeUnixSocket(addr) {
			network = "unix"
			target = addr
		}
		return dialer.DialContext(ctx, network, target)
	}

	socketPath := filepath.Join(p.root, ".fmail", "forged.sock")
	if conn, err := dialer.DialContext(ctx, "unix", socketPath); err == nil {
		return conn, nil
	}
	return dialer.DialContext(ctx, "tcp", forgedTCPAddr)
}

func looksLikeUnixSocket(addr string) bool {
	if strings.HasPrefix(addr, "/") {
		return true
	}
	if strings.HasPrefix(addr, "./") {
		return true
	}
	return strings.HasSuffix(addr, ".sock") && !strings.Contains(addr, ":")
}

func forgedSocketExists(root string) bool {
	socketPath := filepath.Join(root, ".fmail", "forged.sock")
	info, err := os.Stat(socketPath)
	if err != nil {
		return false
	}
	return !info.IsDir()
}
