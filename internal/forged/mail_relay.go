package forged

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"strings"
	"sync"
	"time"

	"github.com/rs/zerolog"
	"github.com/tOgg1/forge/internal/fmail"
)

type mailRelayPeer struct {
	raw     string
	network string
	addr    string
}

type mailRelayManager struct {
	logger            zerolog.Logger
	server            *mailServer
	peers             []mailRelayPeer
	host              string
	agent             string
	dialTimeout       time.Duration
	reconnectInterval time.Duration

	mu       sync.Mutex
	lastSeen map[string]string
	cancel   context.CancelFunc
	wg       sync.WaitGroup
}

func newMailRelayManager(logger zerolog.Logger, server *mailServer, host string, peers []string, dialTimeout, reconnectInterval time.Duration) *mailRelayManager {
	if dialTimeout <= 0 {
		dialTimeout = 2 * time.Second
	}
	if reconnectInterval <= 0 {
		reconnectInterval = 2 * time.Second
	}
	return &mailRelayManager{
		logger:            logger,
		server:            server,
		peers:             normalizeMailRelayPeers(peers),
		host:              strings.TrimSpace(host),
		agent:             relayAgentName(host),
		dialTimeout:       dialTimeout,
		reconnectInterval: reconnectInterval,
		lastSeen:          make(map[string]string),
	}
}

func (m *mailRelayManager) Start(ctx context.Context, projects []mailProject) error {
	if m == nil || m.server == nil {
		return nil
	}
	if len(m.peers) == 0 || len(projects) == 0 {
		return nil
	}

	m.mu.Lock()
	if m.cancel != nil {
		m.mu.Unlock()
		return errors.New("mail relay already running")
	}
	ctx, cancel := context.WithCancel(ctx)
	m.cancel = cancel
	m.mu.Unlock()

	for _, project := range projects {
		for _, peer := range m.peers {
			project := project
			peer := peer
			m.wg.Add(1)
			go m.runPeer(ctx, peer, project)
		}
	}
	return nil
}

func (m *mailRelayManager) Stop() {
	if m == nil {
		return
	}
	m.mu.Lock()
	cancel := m.cancel
	m.cancel = nil
	m.mu.Unlock()
	if cancel != nil {
		cancel()
	}
	m.wg.Wait()
}

func (m *mailRelayManager) runPeer(ctx context.Context, peer mailRelayPeer, project mailProject) {
	defer m.wg.Done()

	for {
		select {
		case <-ctx.Done():
			return
		default:
		}

		conn, err := net.DialTimeout(peer.network, peer.addr, m.dialTimeout)
		if err != nil {
			m.logger.Warn().Err(err).Str("peer", peer.raw).Msg("mail relay dial failed")
			if !sleepUntil(ctx, m.reconnectInterval) {
				return
			}
			continue
		}

		m.logger.Info().Str("peer", peer.raw).Str("project", project.ID).Msg("mail relay connected")
		err = m.relayProject(ctx, conn, peer, project)
		_ = conn.Close()

		if err != nil && !errors.Is(err, context.Canceled) {
			m.logger.Warn().Err(err).Str("peer", peer.raw).Str("project", project.ID).Msg("mail relay disconnected")
		}

		if !sleepUntil(ctx, m.reconnectInterval) {
			return
		}
	}
}

func (m *mailRelayManager) relayProject(ctx context.Context, conn net.Conn, peer mailRelayPeer, project mailProject) error {
	reader := bufio.NewReader(conn)
	done := make(chan struct{})
	go func() {
		select {
		case <-ctx.Done():
			_ = conn.Close()
		case <-done:
		}
	}()
	defer close(done)

	req := mailRelayRequest{
		mailBaseRequest: mailBaseRequest{
			Cmd:       "relay",
			ProjectID: project.ID,
			Agent:     m.agent,
			Host:      m.host,
			ReqID:     fmt.Sprintf("relay-%d", time.Now().UTC().UnixNano()),
		},
		Since: m.lastSeenID(peer, project.ID),
	}
	if err := writeJSONLine(conn, req); err != nil {
		return err
	}

	line, err := readMailLine(reader)
	if err != nil {
		return err
	}
	var resp mailResponse
	if err := json.Unmarshal(line, &resp); err != nil {
		return err
	}
	if !resp.OK {
		return fmt.Errorf("relay ack failed: %s", formatRelayError(resp.Error))
	}

	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		default:
		}

		line, err := readMailLine(reader)
		if err != nil {
			return err
		}
		var env mailRelayEnvelope
		if err := json.Unmarshal(line, &env); err != nil {
			return err
		}
		if env.Error != nil {
			return fmt.Errorf("relay stream error: %s", formatRelayError(env.Error))
		}
		if env.Msg == nil {
			continue
		}

		if env.Msg.ID != "" {
			m.updateLastSeen(peer, project.ID, env.Msg.ID)
		}

		if err := m.applyMessage(project, env.Msg); err != nil {
			m.logger.Warn().Err(err).Str("peer", peer.raw).Str("project", project.ID).Msg("mail relay apply failed")
		}
	}
}

func (m *mailRelayManager) applyMessage(project mailProject, message *fmail.Message) error {
	if message == nil {
		return errors.New("message is nil")
	}
	if m.server == nil {
		return errors.New("mail server is nil")
	}
	hub, err := m.server.getHub(project)
	if err != nil {
		return err
	}
	_, err = hub.ingestMessage(message)
	return err
}

func (m *mailRelayManager) lastSeenID(peer mailRelayPeer, projectID string) string {
	key := relayLastSeenKey(peer, projectID)
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.lastSeen[key]
}

func (m *mailRelayManager) updateLastSeen(peer mailRelayPeer, projectID, messageID string) {
	if messageID == "" {
		return
	}
	key := relayLastSeenKey(peer, projectID)
	m.mu.Lock()
	defer m.mu.Unlock()
	if prev, ok := m.lastSeen[key]; !ok || messageID > prev {
		m.lastSeen[key] = messageID
	}
}

func relayLastSeenKey(peer mailRelayPeer, projectID string) string {
	return peer.network + "://" + peer.addr + "|" + projectID
}

type mailRelayEnvelope struct {
	OK    *bool          `json:"ok,omitempty"`
	Error *mailErr       `json:"error,omitempty"`
	Msg   *fmail.Message `json:"msg,omitempty"`
	Event string         `json:"event,omitempty"`
	ReqID string         `json:"req_id,omitempty"`
}

func normalizeMailRelayPeers(peers []string) []mailRelayPeer {
	seen := make(map[string]struct{}, len(peers))
	result := make([]mailRelayPeer, 0, len(peers))
	for _, peer := range peers {
		trimmed := strings.TrimSpace(peer)
		if trimmed == "" {
			continue
		}
		network := "tcp"
		addr := trimmed
		if strings.HasPrefix(trimmed, "unix://") {
			network = "unix"
			addr = strings.TrimPrefix(trimmed, "unix://")
		} else if strings.HasPrefix(trimmed, "tcp://") {
			addr = strings.TrimPrefix(trimmed, "tcp://")
		}
		key := network + "://" + addr
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}
		result = append(result, mailRelayPeer{raw: trimmed, network: network, addr: addr})
	}
	return result
}

func relayAgentName(host string) string {
	trimmed := strings.TrimSpace(host)
	if trimmed == "" {
		return "relay"
	}
	lower := strings.ToLower(trimmed)
	var builder strings.Builder
	for _, r := range lower {
		switch {
		case r >= 'a' && r <= 'z':
			builder.WriteRune(r)
		case r >= '0' && r <= '9':
			builder.WriteRune(r)
		case r == '-':
			builder.WriteRune(r)
		default:
			builder.WriteByte('-')
		}
	}
	name := strings.Trim(builder.String(), "-")
	if name == "" {
		return "relay"
	}
	name = "relay-" + name
	if err := fmail.ValidateAgentName(name); err != nil {
		return "relay"
	}
	return name
}

func formatRelayError(err *mailErr) string {
	if err == nil {
		return "unknown error"
	}
	msg := strings.TrimSpace(err.Message)
	if msg == "" {
		msg = err.Code
	}
	if msg == "" {
		return "unknown error"
	}
	if err.Code == "" || strings.Contains(msg, err.Code) {
		return msg
	}
	return fmt.Sprintf("%s (%s)", msg, err.Code)
}

func writeJSONLine(conn net.Conn, payload any) error {
	data, err := json.Marshal(payload)
	if err != nil {
		return err
	}
	data = append(data, '\n')
	_, err = conn.Write(data)
	return err
}

func sleepUntil(ctx context.Context, delay time.Duration) bool {
	if delay <= 0 {
		return true
	}
	timer := time.NewTimer(delay)
	defer timer.Stop()
	select {
	case <-ctx.Done():
		return false
	case <-timer.C:
		return true
	}
}
