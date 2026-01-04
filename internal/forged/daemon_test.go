package forged

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/rs/zerolog"
	"github.com/tOgg1/forge/internal/config"
)

func TestNewDefaultsHostname(t *testing.T) {
	cfg := config.DefaultConfig()
	daemon, err := New(cfg, zerolog.Nop(), Options{DisableDatabase: true})
	if err != nil {
		t.Fatalf("New() error = %v", err)
	}

	want := fmt.Sprintf("127.0.0.1:%d", DefaultPort)
	if got := daemon.bindAddr(); got != want {
		t.Fatalf("bindAddr() = %q, want %q", got, want)
	}
}

func TestRunReturnsOnCanceledContext(t *testing.T) {
	cfg := config.DefaultConfig()
	// Use a high ephemeral port to avoid conflicts with other tests
	daemon, err := New(cfg, zerolog.Nop(), Options{Port: 50099, DisableDatabase: true})
	if err != nil {
		t.Fatalf("New() error = %v", err)
	}

	ctx, cancel := context.WithCancel(context.Background())

	// Run in goroutine since Run blocks until context is canceled
	done := make(chan error, 1)
	go func() {
		done <- daemon.Run(ctx)
	}()

	// Give the server a moment to start, then cancel
	time.Sleep(50 * time.Millisecond)
	cancel()

	select {
	case err := <-done:
		if err != nil {
			t.Fatalf("Run() error = %v", err)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("Run() did not return after context cancellation")
	}
}

func TestNewWithDatabase(t *testing.T) {
	// Create temp directory for database
	tmpDir, err := os.MkdirTemp("", "forged-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer os.RemoveAll(tmpDir)

	cfg := config.DefaultConfig()
	cfg.Global.DataDir = tmpDir
	cfg.Database.Path = filepath.Join(tmpDir, "test.db")

	daemon, err := New(cfg, zerolog.Nop(), Options{DisableDatabase: false})
	if err != nil {
		t.Fatalf("New() error = %v", err)
	}
	defer daemon.Close()

	// Verify database is initialized
	if daemon.Database() == nil {
		t.Fatal("expected database to be initialized")
	}

	// Verify repositories are created
	if daemon.AgentRepository() == nil {
		t.Fatal("expected agent repository to be initialized")
	}
	if daemon.QueueRepository() == nil {
		t.Fatal("expected queue repository to be initialized")
	}
	if daemon.WorkspaceRepository() == nil {
		t.Fatal("expected workspace repository to be initialized")
	}
	if daemon.EventRepository() == nil {
		t.Fatal("expected event repository to be initialized")
	}
	if daemon.NodeRepository() == nil {
		t.Fatal("expected node repository to be initialized")
	}
	if daemon.PortRepository() == nil {
		t.Fatal("expected port repository to be initialized")
	}

	// Verify database file exists
	if _, err := os.Stat(cfg.Database.Path); os.IsNotExist(err) {
		t.Fatal("expected database file to exist")
	}

	// Verify services are initialized
	if daemon.TmuxClient() == nil {
		t.Fatal("expected tmux client to be initialized")
	}
	if daemon.AdapterRegistry() == nil {
		t.Fatal("expected adapter registry to be initialized")
	}
	if daemon.StateEngine() == nil {
		t.Fatal("expected state engine to be initialized")
	}
	if daemon.QueueService() == nil {
		t.Fatal("expected queue service to be initialized")
	}
	if daemon.StatePoller() == nil {
		t.Fatal("expected state poller to be initialized")
	}
	if daemon.EventWatcher() == nil {
		t.Fatal("expected event watcher to be initialized")
	}
}

func TestNewWithDatabaseDisabled(t *testing.T) {
	cfg := config.DefaultConfig()

	daemon, err := New(cfg, zerolog.Nop(), Options{DisableDatabase: true})
	if err != nil {
		t.Fatalf("New() error = %v", err)
	}
	defer daemon.Close()

	// Verify database is nil when disabled
	if daemon.Database() != nil {
		t.Fatal("expected database to be nil when disabled")
	}

	// Verify repositories are nil when disabled
	if daemon.AgentRepository() != nil {
		t.Fatal("expected agent repository to be nil when disabled")
	}

	// Verify tmux client and registry are still created even without database
	if daemon.TmuxClient() == nil {
		t.Fatal("expected tmux client to be initialized even without database")
	}
	if daemon.AdapterRegistry() == nil {
		t.Fatal("expected adapter registry to be initialized even without database")
	}

	// Verify services are nil when database is disabled
	if daemon.StateEngine() != nil {
		t.Fatal("expected state engine to be nil when database is disabled")
	}
	if daemon.QueueService() != nil {
		t.Fatal("expected queue service to be nil when database is disabled")
	}
	if daemon.StatePoller() != nil {
		t.Fatal("expected state poller to be nil when database is disabled")
	}
	if daemon.EventWatcher() != nil {
		t.Fatal("expected event watcher to be nil when database is disabled")
	}

	// Verify scheduler is nil by default
	if daemon.Scheduler() != nil {
		t.Fatal("expected scheduler to be nil by default")
	}
}
