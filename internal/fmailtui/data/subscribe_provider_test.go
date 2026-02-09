package data

import (
	"bufio"
	"bytes"
	"encoding/json"
	"io"
	"net"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/stretchr/testify/require"

	"github.com/tOgg1/forge/internal/fmail"
)

func TestForgedProviderSubscribeFallsBackToFileProvider(t *testing.T) {
	root := shortTempDir(t)
	store, err := fmail.NewStore(root)
	require.NoError(t, err)
	require.NoError(t, store.EnsureRoot())

	socketPath := filepath.Join(root, ".fmail", "forged.sock")
	server := startTestForgedServer(t, socketPath)
	defer server.Close()

	fileProvider, err := NewFileProvider(FileProviderConfig{
		Root:         root,
		PollInterval: 20 * time.Millisecond,
	})
	require.NoError(t, err)

	forgedProvider, err := NewForgedProvider(ForgedProviderConfig{
		Root:              root,
		Fallback:          fileProvider,
		ReconnectInterval: 40 * time.Millisecond,
		SubscribeBuffer:   8,
	})
	require.NoError(t, err)

	stream, cancel := forgedProvider.Subscribe(SubscriptionFilter{Topic: "task"})
	defer cancel()

	require.True(t, server.WaitForWatchers(1, 2*time.Second))

	liveMessage := fmail.Message{
		ID:       fmail.GenerateMessageID(time.Now().UTC().Add(-time.Second)),
		From:     "alice",
		To:       "task",
		Time:     time.Now().UTC().Add(-time.Second),
		Body:     "live",
		Priority: fmail.PriorityNormal,
	}
	require.NoError(t, server.Send(liveMessage))

	first := waitMessage(t, stream)
	require.Equal(t, liveMessage.ID, first.ID)
	require.Equal(t, "live", first.Body)

	server.Close()
	time.Sleep(100 * time.Millisecond)

	fallbackMessage := fmail.Message{
		From: "alice",
		To:   "task",
		Body: "fallback",
		Time: time.Now().UTC(),
	}
	_, err = store.SaveMessage(&fallbackMessage)
	require.NoError(t, err)

	second := waitMessage(t, stream)
	require.Equal(t, "fallback", second.Body)
}

func TestHybridProviderSwitchesFromFileToForged(t *testing.T) {
	root := shortTempDir(t)
	store, err := fmail.NewStore(root)
	require.NoError(t, err)
	require.NoError(t, store.EnsureRoot())

	fileProvider, err := NewFileProvider(FileProviderConfig{
		Root:         root,
		PollInterval: 20 * time.Millisecond,
	})
	require.NoError(t, err)
	forgedProvider, err := NewForgedProvider(ForgedProviderConfig{
		Root:              root,
		Fallback:          fileProvider,
		ReconnectInterval: 40 * time.Millisecond,
		SubscribeBuffer:   8,
	})
	require.NoError(t, err)
	hybridProvider, err := NewHybridProvider(HybridProviderConfig{
		Root:              root,
		FileProvider:      fileProvider,
		ForgedProvider:    forgedProvider,
		ReconnectInterval: 50 * time.Millisecond,
		SubscribeBuffer:   8,
	})
	require.NoError(t, err)

	stream, cancel := hybridProvider.Subscribe(SubscriptionFilter{Topic: "task"})
	defer cancel()

	fileMessage := fmail.Message{
		From: "alice",
		To:   "task",
		Body: "from-file",
		Time: time.Now().UTC(),
	}
	_, err = store.SaveMessage(&fileMessage)
	require.NoError(t, err)

	first := waitMessage(t, stream)
	require.Equal(t, "from-file", first.Body)

	socketPath := filepath.Join(root, ".fmail", "forged.sock")
	server := startTestForgedServer(t, socketPath)
	defer server.Close()
	require.True(t, server.WaitForWatchers(1, 3*time.Second))

	liveMessage := fmail.Message{
		ID:   fmail.GenerateMessageID(time.Now().UTC().Add(time.Second)),
		From: "bob",
		To:   "task",
		Time: time.Now().UTC().Add(time.Second),
		Body: "from-forged",
	}
	require.NoError(t, server.Send(liveMessage))

	second := waitMessage(t, stream)
	require.Equal(t, "from-forged", second.Body)
	require.Equal(t, "bob", second.From)
}

func waitMessage(t *testing.T, stream <-chan fmail.Message) fmail.Message {
	t.Helper()
	select {
	case msg, ok := <-stream:
		require.True(t, ok, "subscription closed unexpectedly")
		return msg
	case <-time.After(3 * time.Second):
		t.Fatal("timed out waiting for message")
		return fmail.Message{}
	}
}

func shortTempDir(t *testing.T) string {
	t.Helper()
	root, err := os.MkdirTemp("/tmp", "fmailtui-data-")
	require.NoError(t, err)
	t.Cleanup(func() { _ = os.RemoveAll(root) })
	return root
}

type testForgedServer struct {
	mu       sync.Mutex
	listener net.Listener
	watchers []*testWatcher
}

type testWatcher struct {
	mu    sync.Mutex
	conn  net.Conn
	topic string
}

func startTestForgedServer(t *testing.T, socketPath string) *testForgedServer {
	t.Helper()
	require.NoError(t, os.MkdirAll(filepath.Dir(socketPath), 0o755))
	_ = os.Remove(socketPath)
	listener, err := net.Listen("unix", socketPath)
	require.NoError(t, err)

	server := &testForgedServer{listener: listener}
	go server.serve()
	return server
}

func (s *testForgedServer) serve() {
	for {
		conn, err := s.listener.Accept()
		if err != nil {
			return
		}
		go s.handleConn(conn)
	}
}

func (s *testForgedServer) handleConn(conn net.Conn) {
	reader := bufio.NewReader(conn)
	line, err := readForgedLine(reader)
	if err != nil || len(line) == 0 {
		_ = conn.Close()
		return
	}
	var req forgedWatchRequest
	if err := json.Unmarshal(line, &req); err != nil {
		_ = conn.Close()
		return
	}
	if strings.TrimSpace(req.Cmd) != "watch" {
		_ = conn.Close()
		return
	}

	watcher := &testWatcher{
		conn:  conn,
		topic: strings.TrimSpace(req.Topic),
	}
	s.mu.Lock()
	s.watchers = append(s.watchers, watcher)
	s.mu.Unlock()
	defer s.removeWatcher(watcher)

	writer := bufio.NewWriter(conn)
	if err := writeJSONLine(writer, forgedWatchAck{OK: true}); err != nil {
		return
	}
	_, _ = io.Copy(io.Discard, conn)
}

func (s *testForgedServer) removeWatcher(target *testWatcher) {
	s.mu.Lock()
	defer s.mu.Unlock()
	for i, watcher := range s.watchers {
		if watcher == target {
			s.watchers = append(s.watchers[:i], s.watchers[i+1:]...)
			break
		}
	}
	_ = target.conn.Close()
}

func (s *testForgedServer) Send(message fmail.Message) error {
	s.mu.Lock()
	watchers := append([]*testWatcher(nil), s.watchers...)
	s.mu.Unlock()

	for _, watcher := range watchers {
		if !watcher.matches(message) {
			continue
		}
		if err := watcher.send(message); err != nil {
			return err
		}
	}
	return nil
}

func (s *testForgedServer) WaitForWatchers(count int, timeout time.Duration) bool {
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		s.mu.Lock()
		n := len(s.watchers)
		s.mu.Unlock()
		if n >= count {
			return true
		}
		time.Sleep(20 * time.Millisecond)
	}
	return false
}

func (s *testForgedServer) CloseWatchers() {
	s.mu.Lock()
	watchers := append([]*testWatcher(nil), s.watchers...)
	s.watchers = nil
	s.mu.Unlock()
	for _, watcher := range watchers {
		_ = watcher.conn.Close()
	}
}

func (s *testForgedServer) Close() {
	s.CloseWatchers()
	if s.listener != nil {
		_ = s.listener.Close()
	}
}

func (w *testWatcher) matches(message fmail.Message) bool {
	topic := strings.TrimSpace(w.topic)
	if topic == "" || topic == "*" {
		return !strings.HasPrefix(message.To, "@")
	}
	return strings.EqualFold(topic, message.To)
}

func (w *testWatcher) send(message fmail.Message) error {
	w.mu.Lock()
	defer w.mu.Unlock()
	var encoded bytes.Buffer
	if err := json.NewEncoder(&encoded).Encode(forgedWatchEnvelope{Msg: &message}); err != nil {
		return err
	}
	_, err := w.conn.Write(encoded.Bytes())
	return err
}
