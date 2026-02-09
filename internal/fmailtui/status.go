package fmailtui

import (
	"fmt"
	"io/fs"
	"net"
	"os"
	"path/filepath"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
	"github.com/tOgg1/forge/internal/fmailtui/styles"
)

type statusTickMsg struct{}

func statusTickCmd() tea.Cmd {
	return tea.Tick(1*time.Second, func(time.Time) tea.Msg { return statusTickMsg{} })
}

type statusIncomingMsg struct {
	msg fmail.Message
}

type statusMetricsMsg struct {
	agentsRecent int
	unread       int
}

type statusProbeMsg struct {
	configured bool
	connected  bool
	method     string
}

type statusConnState int

const (
	connPolling statusConnState = iota
	connConnected
	connDisconnected
	connReconnecting
)

type statusConn struct {
	state       statusConnState
	method      string
	attempt     int
	maxAttempts int
	nextProbe   time.Time
	lastProbe   time.Time
}

type statusState struct {
	now time.Time

	msgTimes      []time.Time
	agentLastSeen map[string]time.Time

	agentsRecent int
	unread       int

	conn statusConn

	diskBytes   int64
	diskChecked time.Time

	lastMetrics time.Time
}

func (s *statusState) record(msg fmail.Message, now time.Time) {
	s.now = now
	if s.agentLastSeen == nil {
		s.agentLastSeen = make(map[string]time.Time)
	}
	s.msgTimes = append(s.msgTimes, now)
	if from := strings.TrimSpace(msg.From); from != "" {
		s.agentLastSeen[from] = now
	}
	s.prune(now)
}

func (s *statusState) onTick(now time.Time) (needProbe bool, needMetrics bool) {
	s.now = now
	s.prune(now)

	if s.conn.nextProbe.IsZero() || !now.Before(s.conn.nextProbe) {
		needProbe = true
	}

	// Metrics: avoid heavy provider scans each second.
	if s.lastMetrics.IsZero() || now.Sub(s.lastMetrics) >= 5*time.Second {
		s.lastMetrics = now
		needMetrics = true
	}
	return needProbe, needMetrics
}

func (s *statusState) prune(now time.Time) {
	cutoff := now.Add(-10 * time.Minute)
	if len(s.msgTimes) > 0 {
		keep := 0
		for _, ts := range s.msgTimes {
			if ts.After(cutoff) {
				break
			}
			keep++
		}
		if keep > 0 {
			s.msgTimes = append([]time.Time(nil), s.msgTimes[keep:]...)
		}
	}
	if len(s.agentLastSeen) > 0 {
		cut := now.Add(-10 * time.Minute)
		for agent, ts := range s.agentLastSeen {
			if ts.Before(cut) {
				delete(s.agentLastSeen, agent)
			}
		}
	}
}

func (s *statusState) msgPerMin(now time.Time) int {
	cutoff := now.Add(-1 * time.Minute)
	n := 0
	for i := len(s.msgTimes) - 1; i >= 0; i-- {
		if s.msgTimes[i].Before(cutoff) {
			break
		}
		n++
	}
	return n
}

func (s *statusState) agentsActive(now time.Time) int {
	cut := now.Add(-10 * time.Minute)
	n := 0
	for _, ts := range s.agentLastSeen {
		if ts.After(cut) {
			n++
		}
	}
	return n
}

func (s *statusState) spark10m(now time.Time) string {
	counts := make([]int, 10)
	for _, ts := range s.msgTimes {
		age := now.Sub(ts)
		if age < 0 || age > 10*time.Minute {
			continue
		}
		idx := int(age / time.Minute)
		if idx < 0 || idx >= 10 {
			continue
		}
		counts[9-idx]++
	}
	return renderSpark(counts)
}

func (s *statusState) diskUsage(root string, now time.Time) int64 {
	if root == "" {
		return 0
	}
	if !s.diskChecked.IsZero() && now.Sub(s.diskChecked) < 60*time.Second {
		return s.diskBytes
	}
	dir := filepath.Join(root, ".fmail")
	var total int64
	_ = filepath.WalkDir(dir, func(path string, d fs.DirEntry, err error) error {
		if err != nil || d == nil || d.IsDir() {
			return nil
		}
		info, err := d.Info()
		if err != nil {
			return nil
		}
		total += info.Size()
		return nil
	})
	s.diskBytes = total
	s.diskChecked = now
	return total
}

func (m *Model) statusInitCmd() tea.Cmd {
	m.startStatusSubscription()
	return tea.Batch(statusTickCmd(), m.waitForStatusMsgCmd(), m.statusProbeCmd(), m.statusMetricsCmd())
}

func (m *Model) maybeReconnectForged(now time.Time) {
	if m == nil || m.forgedClient != nil {
		return
	}
	addr := strings.TrimSpace(m.forgedAddr)
	if addr == "" {
		sock := filepath.Join(m.root, ".fmail", "forged.sock")
		if _, err := os.Stat(sock); err == nil {
			addr = "unix://" + sock
		}
	}
	if strings.TrimSpace(addr) == "" {
		return
	}
	if !m.lastReconnectAttempt.IsZero() && now.Sub(m.lastReconnectAttempt) < 5*time.Second {
		return
	}
	m.lastReconnectAttempt = now

	client, err := connectForged(addr)
	if err != nil {
		m.forgedErr = err
		m.reconnectAttempts++
		return
	}

	m.forgedClient = client
	m.forgedErr = nil
	m.reconnectAttempts = 0
}

func (m *Model) startStatusSubscription() {
	if m == nil || m.provider == nil || m.statusCh != nil {
		return
	}
	ch, cancel := m.provider.Subscribe(data.SubscriptionFilter{IncludeDM: true})
	m.statusCh = ch
	m.statusCancel = cancel
}

func (m *Model) waitForStatusMsgCmd() tea.Cmd {
	if m == nil || m.statusCh == nil {
		return nil
	}
	return func() tea.Msg {
		msg, ok := <-m.statusCh
		if !ok {
			return nil
		}
		return statusIncomingMsg{msg: msg}
	}
}

func (m *Model) statusMetricsCmd() tea.Cmd {
	if m == nil || m.provider == nil || m.tuiState == nil {
		return nil
	}
	provider := m.provider
	stateMgr := m.tuiState
	self := strings.TrimSpace(m.selfAgent)
	return func() tea.Msg {
		now := time.Now().UTC()
		agents, _ := provider.Agents()
		cut := now.Add(-10 * time.Minute)
		agentsRecent := 0
		for _, a := range agents {
			if !a.LastSeen.IsZero() && a.LastSeen.After(cut) {
				agentsRecent++
			}
		}

		unread := 0
		snap := stateMgr.Snapshot()
		topics, err := provider.Topics()
		if err == nil {
			for _, topic := range topics {
				marker := readMarkerForTarget(snap.ReadMarkers, topic.Name)
				n, err := unreadCountForTopic(provider, topic.Name, marker, topic.MessageCount)
				if err != nil {
					continue
				}
				unread += n
			}
		}

		if self != "" {
			convs, err := provider.DMConversations(self)
			if err == nil {
				for _, conv := range convs {
					marker := readMarkerForTarget(snap.ReadMarkers, "@"+conv.Agent)
					n, err := unreadCountForDM(provider, self, conv.Agent, marker)
					if err != nil {
						continue
					}
					unread += n
				}
			}
		}

		return statusMetricsMsg{agentsRecent: agentsRecent, unread: unread}
	}
}

func (m *Model) statusProbeCmd() tea.Cmd {
	if m == nil {
		return nil
	}
	root := m.root
	addr := m.forgedAddr
	return func() tea.Msg {
		configured := forgedConfigured(root, addr)
		if !configured {
			return statusProbeMsg{configured: false}
		}
		connected, method := probeForged(root, addr)
		return statusProbeMsg{configured: true, connected: connected, method: method}
	}
}

func (m *Model) breadcrumb() string {
	if m == nil || len(m.viewStack) == 0 {
		return ""
	}
	parts := make([]string, 0, len(m.viewStack))
	for _, id := range m.viewStack {
		label := viewLabel(id)
		if id == ViewThread {
			if view := m.views[ViewThread]; view != nil {
				if ctx, ok := view.(interface{ ComposeTarget() string }); ok {
					if target := strings.TrimSpace(ctx.ComposeTarget()); target != "" {
						label = target
					}
				}
			}
		}
		parts = append(parts, label)
	}
	if m.activeViewID() == ViewDashboard {
		if view := m.views[ViewDashboard]; view != nil {
			if focused, ok := view.(interface{ FocusLabel() string }); ok {
				if pane := strings.TrimSpace(focused.FocusLabel()); pane != "" {
					parts = append(parts, pane)
				}
			}
		}
	}
	return strings.Join(parts, " > ")
}

func viewLabel(id ViewID) string {
	switch id {
	case ViewDashboard:
		return "Dashboard"
	case ViewTopics:
		return "Topics"
	case ViewThread:
		return "Thread"
	case ViewAgents:
		return "Agents"
	case ViewOperator:
		return "Operator"
	case ViewSearch:
		return "Search"
	case ViewLiveTail:
		return "Live"
	case ViewTimeline:
		return "Timeline"
	default:
		return string(id)
	}
}

func (m *Model) renderStatusBar() string {
	palette, ok := styles.Themes[string(m.theme)]
	if !ok {
		palette = styles.DefaultTheme
	}
	now := m.status.now
	if now.IsZero() {
		now = time.Now().UTC()
	}

	style := lipgloss.NewStyle().
		Foreground(lipgloss.Color(palette.Base.Foreground)).
		Background(lipgloss.Color(palette.Chrome.Footer)).
		Padding(0, 1).
		Width(maxInt(0, m.width))

	connText, connStyle := renderConn(m.status.conn, now, palette)
	if m.width < 90 {
		// Drop method details when narrow.
		connText = dropConnMethod(connText)
	}
	conn := connStyle.Render(connText)

	rate := fmt.Sprintf("%d msg/m", m.status.msgPerMin(now))
	spark := m.status.spark10m(now)
	throughput := rate
	if strings.TrimSpace(spark) != "" && m.width >= 92 {
		throughput = rate + " " + spark
	}

	agentsCount := m.status.agentsRecent
	if agentsCount == 0 {
		agentsCount = m.status.agentsActive(now)
	}
	agents := fmt.Sprintf("%d agents", agentsCount)

	notif := ""
	if m.status.unread > 0 {
		notif = fmt.Sprintf("[N:%d]", m.status.unread)
	}
	pane := ""
	if m.activeViewID() == ViewDashboard {
		if view := m.views[ViewDashboard]; view != nil {
			if focused, ok := view.(interface{ FocusLabel() string }); ok {
				if label := strings.TrimSpace(focused.FocusLabel()); label != "" {
					pane = "pane:" + label
				}
			}
		}
	}
	clock := now.Format("15:04")

	segments := make([]string, 0, 6)
	segments = append(segments, conn)
	if m.width >= 52 {
		segments = append(segments, throughput)
	}
	if m.width >= 72 {
		segments = append(segments, agents)
	}
	if notif != "" && m.width >= 86 {
		segments = append(segments, notif)
	}
	if pane != "" && m.width >= 96 {
		segments = append(segments, pane)
	}
	segments = append(segments, clock)

	if m.width >= 120 {
		bytes := m.status.diskUsage(m.root, now)
		if bytes > 0 {
			seg := fmt.Sprintf(".fmail:%s", humanBytes(bytes))
			if bytes > 100*1024*1024 {
				seg = lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Priority.High)).Bold(true).Render(seg)
			}
			segments = append(segments, seg)
		}
	}

	line := strings.Join(segments, "  |  ")
	return style.Render(truncateVis(line, maxInt(0, m.width-2)))
}

func (s *statusState) applyMetrics(msg statusMetricsMsg) {
	s.agentsRecent = msg.agentsRecent
	s.unread = msg.unread
}

func (s *statusState) applyProbe(msg statusProbeMsg, now time.Time) {
	s.now = now
	if s.conn.maxAttempts == 0 {
		s.conn.maxAttempts = 10
	}
	s.conn.lastProbe = now

	if !msg.configured {
		s.conn.state = connPolling
		s.conn.method = ""
		s.conn.attempt = 0
		s.conn.nextProbe = now.Add(30 * time.Second)
		return
	}

	s.conn.method = strings.TrimSpace(msg.method)
	if msg.connected {
		s.conn.state = connConnected
		s.conn.attempt = 0
		s.conn.nextProbe = now.Add(5 * time.Second)
		return
	}

	if s.conn.attempt < s.conn.maxAttempts {
		s.conn.attempt++
		s.conn.state = connReconnecting
	} else {
		s.conn.state = connDisconnected
	}
	s.conn.nextProbe = now.Add(reconnectBackoff(s.conn.attempt))
}

func dropConnMethod(s string) string {
	// Keep left-most part; drop trailing method markers for narrow terminals.
	if i := strings.LastIndex(s, " unix:"); i > 0 {
		return strings.TrimSpace(s[:i])
	}
	if i := strings.LastIndex(s, " tcp:"); i > 0 {
		return strings.TrimSpace(s[:i])
	}
	if i := strings.LastIndex(s, " file:"); i > 0 {
		return strings.TrimSpace(s[:i])
	}
	return strings.TrimSpace(s)
}

func forgedConfigured(root string, forgedAddr string) bool {
	if strings.TrimSpace(forgedAddr) != "" {
		return true
	}
	if strings.TrimSpace(root) == "" {
		return false
	}
	path := filepath.Join(root, ".fmail", "forged.sock")
	info, err := os.Stat(path)
	if err != nil {
		return false
	}
	return !info.IsDir()
}

func reconnectBackoff(attempt int) time.Duration {
	base := 5 * time.Second
	if attempt <= 1 {
		return base
	}
	backoff := base << (attempt - 1) // 5s, 10s, 20s...
	if backoff > 60*time.Second {
		return 60 * time.Second
	}
	return backoff
}

func probeForged(root string, forgedAddr string) (bool, string) {
	dialer := &net.Dialer{Timeout: forgedDialTimeout}

	// Prefer project socket if present.
	socketPath := ""
	if strings.TrimSpace(root) != "" {
		socketPath = filepath.Join(root, ".fmail", "forged.sock")
		if info, err := os.Stat(socketPath); err == nil && info != nil && !info.IsDir() {
			if conn, err := dialer.Dial("unix", socketPath); err == nil {
				_ = conn.Close()
				return true, "unix:.fmail/forged.sock"
			}
			if strings.TrimSpace(forgedAddr) == "" {
				return false, "unix:.fmail/forged.sock"
			}
		}
	}

	addr := strings.TrimSpace(forgedAddr)
	if addr != "" {
		network, target := forgedEndpoint(addr)
		method := network + ":" + target
		if network == "unix" && target == socketPath && target != "" {
			method = "unix:.fmail/forged.sock"
		}
		conn, err := dialer.Dial(network, target)
		if err != nil {
			return false, method
		}
		_ = conn.Close()
		return true, method
	}

	// Last resort: default forged TCP.
	const defaultForgedTCPAddr = "127.0.0.1:7463"
	conn, err := dialer.Dial("tcp", defaultForgedTCPAddr)
	if err != nil {
		return false, "tcp:" + defaultForgedTCPAddr
	}
	_ = conn.Close()
	return true, "tcp:" + defaultForgedTCPAddr
}

func renderConn(conn statusConn, now time.Time, palette styles.Theme) (string, lipgloss.Style) {
	method := strings.TrimSpace(conn.method)
	fileMethod := fmt.Sprintf("file:%s", defaultFileProviderPollInterval)

	switch conn.state {
	case connConnected:
		if method == "" {
			method = "unix:.fmail/forged.sock"
		}
		return fmt.Sprintf("● connected %s", method),
			lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Status.Online))
	case connReconnecting:
		if method == "" {
			method = "unix:.fmail/forged.sock"
		}
		indicator := "↻"
		if now.Unix()%2 == 0 {
			indicator = " "
		}
		return fmt.Sprintf("%s reconnecting (attempt %d/%d) %s (%s)", indicator, conn.attempt, conn.maxAttempts, method, fileMethod),
			lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Status.Recent))
	case connDisconnected:
		if method == "" {
			method = "unix:.fmail/forged.sock"
		}
		return fmt.Sprintf("✕ disconnected %s (%s)", method, fileMethod),
			lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Priority.High))
	default:
		return fmt.Sprintf("◐ polling (%s)", fileMethod),
			lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Status.Recent))
	}
}

func shortEndpoint(endpoint string) string {
	endpoint = strings.TrimSpace(endpoint)
	switch {
	case strings.HasPrefix(endpoint, "unix://"):
		return "unix:" + filepath.Base(strings.TrimPrefix(endpoint, "unix://"))
	case strings.HasPrefix(endpoint, "tcp://"):
		return "tcp:" + strings.TrimPrefix(endpoint, "tcp://")
	case strings.Contains(endpoint, string(filepath.Separator)):
		return "unix:" + filepath.Base(endpoint)
	default:
		return "tcp:" + endpoint
	}
}

func humanBytes(n int64) string {
	if n < 1024 {
		return fmt.Sprintf("%dB", n)
	}
	const unit = 1024
	div, exp := int64(unit), 0
	for v := n / unit; v >= unit; v /= unit {
		div *= unit
		exp++
	}
	suffix := []string{"KB", "MB", "GB", "TB"}
	if exp >= len(suffix) {
		exp = len(suffix) - 1
	}
	return fmt.Sprintf("%.1f%s", float64(n)/float64(div), suffix[exp])
}
