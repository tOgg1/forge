# Scheduler Daemon Integration - Task Breakdown

> **Design Document**: [scheduler-daemon.md](./scheduler-daemon.md)

This document contains detailed task descriptions (beads) for integrating the scheduler and SSE event watching into the `forged` daemon.

---

## Epic 1: Add Scheduler to forged Daemon

> **Reference**: [scheduler-daemon.md](./scheduler-daemon.md) - Sections 2-4

**Goal**: Transform `forged` from a stateless gRPC server into a full orchestration daemon that can dispatch queued messages to agents autonomously.

**Current State**: 
- `forged` runs as a gRPC server with resource monitoring
- It has no database access - all state is in-memory
- The scheduler package exists but is never instantiated

**End State**:
- `forged` connects to SQLite database on startup
- Scheduler runs in background, dispatching messages when agents are idle
- State poller monitors agent states continuously

---

### Task 1.1: Add database connection and repositories to forged daemon

**File**: `internal/forged/daemon.go`

**Description**:
Add database connectivity to the forged daemon. Currently forged is stateless - it tracks agents in an in-memory map. To run the scheduler, we need access to the SQLite database where agents, queues, and workspaces are persisted.

**Changes Required**:

1. Add new fields to the `Daemon` struct:
```go
type Daemon struct {
    // ... existing fields ...
    
    // NEW: Database and repositories
    database   *db.DB
    agentRepo  *db.AgentRepository
    queueRepo  *db.QueueRepository
    wsRepo     *db.WorkspaceRepository
    eventRepo  *db.EventRepository
    nodeRepo   *db.NodeRepository
    portRepo   *db.PortRepository
}
```

2. Add database initialization in `New()`:
```go
// Open database
dbPath := cfg.Database.Path
if dbPath == "" {
    dbPath = filepath.Join(cfg.Global.DataDir, "forge.db")
}
database, err := db.Open(dbPath)
if err != nil {
    return nil, fmt.Errorf("failed to open database: %w", err)
}

// Run migrations
if err := db.Migrate(database); err != nil {
    database.Close()
    return nil, fmt.Errorf("failed to run migrations: %w", err)
}

// Create repositories
agentRepo := db.NewAgentRepository(database)
queueRepo := db.NewQueueRepository(database)
wsRepo := db.NewWorkspaceRepository(database)
eventRepo := db.NewEventRepository(database)
nodeRepo := db.NewNodeRepository(database)
portRepo := db.NewPortRepository(database)
```

3. Add cleanup in shutdown path:
```go
func (d *Daemon) Close() error {
    if d.database != nil {
        return d.database.Close()
    }
    return nil
}
```

4. Update `Options` struct to allow disabling database:
```go
type Options struct {
    // ... existing fields ...
    
    // DisableDatabase skips database initialization (for testing)
    DisableDatabase bool
}
```

**Imports to Add**:
```go
import (
    "github.com/tOgg1/forge/internal/db"
    "path/filepath"
)
```

**Testing**:
- Verify forged starts successfully with valid database path
- Verify forged fails gracefully with invalid database path
- Verify database is closed on shutdown
- Verify migrations run on first start

**Acceptance Criteria**:
- [ ] Daemon struct has database and repository fields
- [ ] Database opens on daemon initialization
- [ ] Migrations run automatically
- [ ] Database closes on daemon shutdown
- [ ] Tests pass with database enabled and disabled

---

### Task 1.2: Initialize StateEngine, AgentService, QueueService in forged

**Files**: `internal/forged/daemon.go`

**Description**:
Create the service layer that the scheduler needs. These services wrap the repositories and provide business logic for agent management, queue operations, and state tracking.

**Dependencies**: Task 1.1 (database connection)

**Changes Required**:

1. Add new fields to `Daemon` struct:
```go
type Daemon struct {
    // ... existing fields ...
    
    // Services
    tmuxClient   *tmux.Client
    stateEngine  *state.Engine
    agentService *agent.Service
    queueService *queue.Service
    wsService    *workspace.Service
    nodeService  *node.Service
}
```

2. Initialize services in `New()` after database setup:
```go
// Create tmux client
tmuxClient := tmux.NewLocalClient()

// Create adapter registry
registry := adapters.NewRegistry()

// Create services
nodeService := node.NewService(nodeRepo)
wsService := workspace.NewService(wsRepo, nodeService, agentRepo)
stateEngine := state.NewEngine(agentRepo, eventRepo, tmuxClient, registry)

// Create agent service with all options
agentServiceOpts := []agent.ServiceOption{
    agent.WithEventRepository(eventRepo),
    agent.WithPortRepository(portRepo),
}
if cfg.Global.DataDir != "" {
    archiveDir := filepath.Join(cfg.Global.DataDir, "archives", "agents")
    agentServiceOpts = append(agentServiceOpts, agent.WithArchiveDir(archiveDir))
}
agentService := agent.NewService(agentRepo, queueRepo, wsService, nil, tmuxClient, agentServiceOpts...)

// Create queue service
queueService := queue.NewService(queueRepo)
```

3. Store services in daemon struct for later use by scheduler.

**Imports to Add**:
```go
import (
    "github.com/tOgg1/forge/internal/adapters"
    "github.com/tOgg1/forge/internal/agent"
    "github.com/tOgg1/forge/internal/node"
    "github.com/tOgg1/forge/internal/queue"
    "github.com/tOgg1/forge/internal/state"
    "github.com/tOgg1/forge/internal/tmux"
    "github.com/tOgg1/forge/internal/workspace"
)
```

**Testing**:
- Verify all services initialize without error
- Verify services can perform basic operations (list agents, list queue, etc.)
- Integration test: spawn agent via CLI, verify forged can see it via services

**Acceptance Criteria**:
- [ ] All services initialize successfully
- [ ] Services are accessible from Daemon struct
- [ ] tmux client connects successfully
- [ ] State engine can detect agent states

---

### Task 1.3: Initialize and start Scheduler in forged.Run()

**Files**: `internal/forged/daemon.go`

**Description**:
Create and start the scheduler that will dispatch queued messages to agents. The scheduler runs a tick loop that checks for idle agents with pending queue items and dispatches them.

**Dependencies**: Task 1.2 (services)

**Changes Required**:

1. Add scheduler field to `Daemon`:
```go
type Daemon struct {
    // ... existing fields ...
    scheduler   *scheduler.Scheduler
    statePoller *state.Poller
}
```

2. Add scheduler initialization in `New()`:
```go
// Create state poller for fallback state detection
pollerConfig := state.DefaultPollerConfig()
statePoller := state.NewPoller(pollerConfig, stateEngine, agentRepo)

// Create scheduler
schedConfig := scheduler.DefaultConfig()
if cfg.Scheduler.TickInterval > 0 {
    schedConfig.TickInterval = cfg.Scheduler.TickInterval
}
if cfg.Scheduler.MaxConcurrentDispatches > 0 {
    schedConfig.MaxConcurrentDispatches = cfg.Scheduler.MaxConcurrentDispatches
}
schedConfig.IdleStateRequired = cfg.Scheduler.IdleStateRequired
schedConfig.AutoResumeEnabled = cfg.Scheduler.AutoResumeEnabled

sched := scheduler.New(
    schedConfig,
    agentService,
    queueService,
    stateEngine,
    nil, // accountService - can add later
)
```

3. Start scheduler in `Run()`:
```go
func (d *Daemon) Run(ctx context.Context) error {
    // ... existing validation ...

    // Start state poller
    if d.statePoller != nil {
        if err := d.statePoller.Start(ctx); err != nil {
            return fmt.Errorf("failed to start state poller: %w", err)
        }
        d.logger.Info().Msg("state poller started")
    }

    // Start scheduler
    if d.scheduler != nil {
        if err := d.scheduler.Start(ctx); err != nil {
            if d.statePoller != nil {
                d.statePoller.Stop()
            }
            return fmt.Errorf("failed to start scheduler: %w", err)
        }
        d.logger.Info().Msg("scheduler started")
    }

    // ... existing gRPC server startup ...
    
    // Wait for shutdown
    select {
    case <-ctx.Done():
        d.logger.Info().Msg("forged shutting down...")
        // Shutdown order: scheduler -> poller -> gRPC
        if d.scheduler != nil {
            d.scheduler.Stop()
        }
        if d.statePoller != nil {
            d.statePoller.Stop()
        }
        d.grpcServer.GracefulStop()
    // ... rest of select ...
    }
}
```

**Imports to Add**:
```go
import (
    "github.com/tOgg1/forge/internal/scheduler"
)
```

**Testing**:
- Start forged, queue a message via CLI, verify it gets dispatched
- Verify scheduler tick interval is configurable
- Verify scheduler stops cleanly on shutdown
- Verify scheduler doesn't dispatch to busy agents

**Acceptance Criteria**:
- [ ] Scheduler starts when forged starts
- [ ] Scheduler dispatches queued messages to idle agents
- [ ] Scheduler respects configuration (tick interval, etc.)
- [ ] Scheduler stops cleanly on shutdown

---

### Task 1.4: Add scheduler config options to forged (enable/disable, intervals)

**Files**: 
- `cmd/forged/main.go`
- `internal/forged/daemon.go`
- `internal/config/config.go`

**Description**:
Add command-line flags and configuration options to control scheduler behavior. Users should be able to disable the scheduler entirely (for gRPC-only mode) or tune its parameters.

**Dependencies**: Task 1.3 (scheduler running)

**Changes Required**:

1. Add flags to `cmd/forged/main.go`:
```go
noScheduler := flag.Bool("no-scheduler", false, "disable the message scheduler")
schedulerTick := flag.Duration("scheduler-tick", 0, "scheduler tick interval (default: 1s)")
noStatePoller := flag.Bool("no-state-poller", false, "disable the state poller")
```

2. Update `Options` struct:
```go
type Options struct {
    // ... existing fields ...
    
    // SchedulerEnabled controls whether the scheduler runs (default: true)
    SchedulerEnabled *bool
    
    // SchedulerTickInterval overrides the scheduler tick interval
    SchedulerTickInterval time.Duration
    
    // StatePollerEnabled controls whether the state poller runs (default: true)
    StatePollerEnabled *bool
}
```

3. Apply options in `New()`:
```go
schedulerEnabled := opts.SchedulerEnabled == nil || *opts.SchedulerEnabled
if schedulerEnabled {
    // ... create scheduler ...
}

statePollerEnabled := opts.StatePollerEnabled == nil || *opts.StatePollerEnabled
if statePollerEnabled {
    // ... create poller ...
}
```

4. Ensure config file settings are respected:
```yaml
# config.yaml
scheduler:
  enabled: true
  tick_interval: 1s
  max_concurrent_dispatches: 10
  idle_state_required: true
  auto_resume_enabled: true
```

**Testing**:
- `forged --no-scheduler` starts without scheduler
- `forged --scheduler-tick 5s` uses custom tick interval
- Config file settings are applied
- Command-line flags override config file

**Acceptance Criteria**:
- [ ] `--no-scheduler` flag disables scheduler
- [ ] `--scheduler-tick` overrides tick interval
- [ ] `--no-state-poller` disables poller
- [ ] Config file settings work
- [ ] Flags override config file

---

### Task 1.5: Ensure graceful shutdown stops scheduler before gRPC server

**Files**: `internal/forged/daemon.go`

**Description**:
Ensure proper shutdown order to prevent race conditions and data corruption. The scheduler should stop first (finish any in-progress dispatches), then the state poller, then the gRPC server.

**Dependencies**: Task 1.3 (scheduler running)

**Changes Required**:

1. Implement ordered shutdown in `Run()`:
```go
// Wait for shutdown signal
select {
case <-ctx.Done():
    d.logger.Info().Msg("forged shutting down...")
    
    // 1. Stop scheduler first (waits for in-progress dispatches)
    if d.scheduler != nil {
        d.logger.Debug().Msg("stopping scheduler...")
        if err := d.scheduler.Stop(); err != nil {
            d.logger.Warn().Err(err).Msg("scheduler stop returned error")
        }
        d.logger.Debug().Msg("scheduler stopped")
    }
    
    // 2. Stop state poller
    if d.statePoller != nil {
        d.logger.Debug().Msg("stopping state poller...")
        if err := d.statePoller.Stop(); err != nil {
            d.logger.Warn().Err(err).Msg("state poller stop returned error")
        }
        d.logger.Debug().Msg("state poller stopped")
    }
    
    // 3. Stop SSE event watcher (Task 2.x)
    if d.eventWatcher != nil {
        d.logger.Debug().Msg("stopping event watcher...")
        d.eventWatcher.UnwatchAll()
        d.logger.Debug().Msg("event watcher stopped")
    }
    
    // 4. Stop gRPC server
    d.logger.Debug().Msg("stopping gRPC server...")
    d.grpcServer.GracefulStop()
    d.logger.Debug().Msg("gRPC server stopped")
    
    // 5. Close database
    if d.database != nil {
        d.logger.Debug().Msg("closing database...")
        if err := d.database.Close(); err != nil {
            d.logger.Warn().Err(err).Msg("database close returned error")
        }
        d.logger.Debug().Msg("database closed")
    }
    
case err := <-errCh:
    // ... handle gRPC error ...
}
```

2. Ensure scheduler.Stop() waits for in-progress work:
```go
// In scheduler/scheduler.go - verify Stop() behavior
func (s *Scheduler) Stop() error {
    // Should wait for current dispatch to complete
    // Should not accept new dispatches
    // Should have a timeout to prevent hanging
}
```

**Testing**:
- Send SIGINT during active dispatch, verify dispatch completes
- Verify no "database is closed" errors during shutdown
- Verify all goroutines exit cleanly (no leaks)
- Test with `go test -race` to detect race conditions

**Acceptance Criteria**:
- [ ] Shutdown completes in-progress dispatches
- [ ] Shutdown order is correct (scheduler → poller → gRPC → DB)
- [ ] No race conditions during shutdown
- [ ] Shutdown has reasonable timeout (doesn't hang forever)
- [ ] All resources are cleaned up

---

## Epic 2: Integrate OpenCode SSE Event Watcher

> **Reference**: [scheduler-daemon.md](./scheduler-daemon.md) - Section 5

**Goal**: Enable real-time state detection for OpenCode agents via SSE events, providing high-confidence state updates that are more reliable than screen-based polling.

**Current State**:
- `OpenCodeEventWatcher` exists in `internal/adapters/opencode_events.go`
- It's fully implemented but never instantiated
- OpenCode agents expose `/events` SSE endpoint
- State detection falls back to unreliable screen polling

**End State**:
- forged creates and manages SSE connections to all OpenCode agents
- State updates from SSE have high confidence and update the database
- Connections auto-start on agent spawn and stop on termination

---

### Task 2.1: Create OpenCodeEventWatcher in forged daemon initialization

**Files**: `internal/forged/daemon.go`

**Description**:
Instantiate the `OpenCodeEventWatcher` in the forged daemon. This watcher manages SSE connections to multiple OpenCode agents and translates events into state updates.

**Dependencies**: Task 1.2 (StateEngine available)

**Changes Required**:

1. Add field to `Daemon`:
```go
type Daemon struct {
    // ... existing fields ...
    eventWatcher *adapters.OpenCodeEventWatcher
}
```

2. Create watcher in `New()`:
```go
// Create SSE event watcher for OpenCode agents
eventWatcherConfig := adapters.DefaultEventWatcherConfig()
// Optionally configure from cfg:
// eventWatcherConfig.ReconnectDelay = cfg.OpenCode.ReconnectDelay

eventWatcher := adapters.NewOpenCodeEventWatcher(
    eventWatcherConfig,
    nil, // onState handler added in Task 2.2
)

d.eventWatcher = eventWatcher
```

3. Add shutdown in `Run()`:
```go
// In shutdown sequence
if d.eventWatcher != nil {
    d.eventWatcher.UnwatchAll()
}
```

**Testing**:
- Verify eventWatcher is created successfully
- Verify eventWatcher is cleaned up on shutdown
- Verify no goroutine leaks

**Acceptance Criteria**:
- [ ] EventWatcher is created during daemon init
- [ ] EventWatcher is accessible from Daemon
- [ ] EventWatcher is cleaned up on shutdown

---

### Task 2.2: Wire onState handler to update StateEngine (persists to DB)

**Files**: `internal/forged/daemon.go`

**Description**:
Connect the SSE event watcher to the state engine so that events from OpenCode update the database. When we receive `session.idle` or `session.busy`, the agent's state in the database should be updated immediately.

**Dependencies**: Task 2.1 (watcher created)

**Changes Required**:

1. Create state handler function:
```go
// In New(), create the state handler
onStateUpdate := func(agentID string, newState models.AgentState, info models.StateInfo) {
    ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
    defer cancel()
    
    // Update state in database via StateEngine
    if err := stateEngine.UpdateState(ctx, agentID, newState, info, nil, nil); err != nil {
        logger.Warn().
            Err(err).
            Str("agent_id", agentID).
            Str("new_state", string(newState)).
            Msg("failed to update agent state from SSE event")
        return
    }
    
    logger.Debug().
        Str("agent_id", agentID).
        Str("new_state", string(newState)).
        Str("confidence", string(info.Confidence)).
        Msg("agent state updated from SSE event")
}

// Pass handler to watcher
eventWatcher := adapters.NewOpenCodeEventWatcher(eventWatcherConfig, onStateUpdate)
```

2. Verify the watcher calls `mapEventToState()` which sets high confidence:
```go
// In opencode_events.go - already implemented
case OpenCodeEventSessionIdle:
    return models.AgentStateIdle, models.StateInfo{
        State:      models.AgentStateIdle,
        Confidence: models.StateConfidenceHigh,  // HIGH confidence
        Reason:     "OpenCode SSE: session idle",
        DetectedAt: now,
    }, true
```

**Testing**:
- Mock SSE server, send `session.idle` event, verify DB is updated
- Verify state confidence is High
- Verify error handling when StateEngine fails
- Verify logging of state changes

**Acceptance Criteria**:
- [ ] SSE events trigger StateEngine.UpdateState()
- [ ] State updates are persisted to database
- [ ] State updates have high confidence
- [ ] Errors are logged but don't crash the daemon

---

### Task 2.3: Subscribe to agent spawn events - auto-start SSE watching

**Files**: `internal/forged/daemon.go`

**Description**:
When a new OpenCode agent is spawned (either via forged or CLI), automatically start watching its SSE endpoint. This ensures we don't miss state updates for newly created agents.

**Dependencies**: Task 2.2 (watcher wired to state engine)

**Changes Required**:

1. Subscribe to state engine events:
```go
// In Run(), after services are ready
stateEngine.SubscribeFunc("sse-auto-watcher", func(change state.StateChange) {
    // Only care about new agents becoming active
    if change.PreviousState == models.AgentStateStarting && 
       change.CurrentState == models.AgentStateIdle {
        d.maybeStartWatching(ctx, change.AgentID)
    }
})
```

2. Implement maybeStartWatching:
```go
func (d *Daemon) maybeStartWatching(ctx context.Context, agentID string) {
    agent, err := d.agentRepo.Get(ctx, agentID)
    if err != nil {
        d.logger.Warn().Err(err).Str("agent_id", agentID).Msg("failed to get agent for SSE watching")
        return
    }
    
    // Only watch OpenCode agents with connection info
    if !agent.HasOpenCodeConnection() {
        return
    }
    
    // Already watching?
    if d.eventWatcher.IsWatching(agentID) {
        return
    }
    
    // Start watching
    if err := d.eventWatcher.WatchAgent(ctx, agent); err != nil {
        d.logger.Warn().Err(err).Str("agent_id", agentID).Msg("failed to start SSE watching")
        return
    }
    
    d.logger.Info().
        Str("agent_id", agentID).
        Str("events_url", agent.Metadata.OpenCode.EventsURL()).
        Msg("started SSE watching for agent")
}
```

3. Alternative: Hook into agent spawn directly if state subscription is too late:
```go
// Could also add a hook in agent.Service.SpawnAgent() 
// that notifies forged of new agents
```

**Testing**:
- Spawn OpenCode agent via CLI, verify forged starts watching
- Verify non-OpenCode agents are not watched
- Verify duplicate watch attempts are ignored
- Verify agents with incomplete connection info are skipped

**Acceptance Criteria**:
- [ ] New OpenCode agents are automatically watched
- [ ] Non-OpenCode agents are ignored
- [ ] Already-watched agents are not re-watched
- [ ] Logging shows watch start

---

### Task 2.4: Subscribe to agent terminate events - stop SSE watching

**Files**: `internal/forged/daemon.go`

**Description**:
When an agent is terminated or stopped, stop watching its SSE endpoint to clean up resources and avoid connection errors.

**Dependencies**: Task 2.3 (auto-start watching)

**Changes Required**:

1. Extend the state change subscriber:
```go
stateEngine.SubscribeFunc("sse-auto-watcher", func(change state.StateChange) {
    // Start watching new agents
    if change.PreviousState == models.AgentStateStarting && 
       change.CurrentState == models.AgentStateIdle {
        d.maybeStartWatching(ctx, change.AgentID)
    }
    
    // Stop watching terminated agents
    if change.CurrentState == models.AgentStateStopped ||
       change.CurrentState == models.AgentStateError {
        d.maybeStopWatching(change.AgentID)
    }
})
```

2. Implement maybeStopWatching:
```go
func (d *Daemon) maybeStopWatching(agentID string) {
    if !d.eventWatcher.IsWatching(agentID) {
        return
    }
    
    if err := d.eventWatcher.Unwatch(agentID); err != nil {
        d.logger.Warn().Err(err).Str("agent_id", agentID).Msg("failed to stop SSE watching")
        return
    }
    
    d.logger.Info().
        Str("agent_id", agentID).
        Msg("stopped SSE watching for agent")
}
```

**Testing**:
- Terminate agent via CLI, verify forged stops watching
- Kill agent forcefully, verify watching stops
- Verify error state triggers watch stop
- Verify graceful handling of already-stopped watches

**Acceptance Criteria**:
- [ ] Terminated agents stop being watched
- [ ] Error-state agents stop being watched
- [ ] Resources are cleaned up properly
- [ ] No errors for already-stopped watches

---

### Task 2.5: On forged startup, start watching all existing OpenCode agents

**Files**: `internal/forged/daemon.go`

**Description**:
When forged starts (or restarts), it should resume watching all existing OpenCode agents that are still running. This handles the case where forged was restarted but agents are still active.

**Dependencies**: Task 2.3 (maybeStartWatching implemented)

**Changes Required**:

1. Add startup function:
```go
func (d *Daemon) startWatchingExistingAgents(ctx context.Context) {
    agents, err := d.agentRepo.List(ctx)
    if err != nil {
        d.logger.Error().Err(err).Msg("failed to list agents for SSE watching")
        return
    }
    
    watched := 0
    for _, agent := range agents {
        // Skip non-running agents
        if agent.State == models.AgentStateStopped ||
           agent.State == models.AgentStateError {
            continue
        }
        
        // Skip non-OpenCode agents
        if !agent.HasOpenCodeConnection() {
            continue
        }
        
        // Start watching
        if err := d.eventWatcher.WatchAgent(ctx, agent); err != nil {
            d.logger.Warn().
                Err(err).
                Str("agent_id", agent.ID).
                Msg("failed to start SSE watching for existing agent")
            continue
        }
        watched++
    }
    
    d.logger.Info().
        Int("agents_watched", watched).
        Int("agents_total", len(agents)).
        Msg("started SSE watching for existing agents")
}
```

2. Call during startup in `Run()`:
```go
func (d *Daemon) Run(ctx context.Context) error {
    // ... start poller and scheduler ...
    
    // Start watching existing agents
    if d.eventWatcher != nil {
        d.startWatchingExistingAgents(ctx)
    }
    
    // ... start gRPC server ...
}
```

**Testing**:
- Start agents, restart forged, verify watching resumes
- Verify stopped agents are not watched
- Verify connection errors are handled gracefully
- Verify startup completes even if some agents fail to watch

**Acceptance Criteria**:
- [ ] Existing running agents are watched on startup
- [ ] Stopped/error agents are skipped
- [ ] Connection failures don't block startup
- [ ] Logging shows how many agents are being watched

---

## Epic 3: State Detection Priority System

> **Reference**: [scheduler-daemon.md](./scheduler-daemon.md) - Section 6

**Goal**: Ensure that high-confidence state updates (from SSE) take priority over low-confidence updates (from polling), preventing state thrashing.

**Current State**:
- StateEngine.UpdateState() always overwrites state
- Polling and SSE could conflict, causing rapid state changes
- Low-confidence polling could override high-confidence SSE

**End State**:
- State updates respect confidence levels
- SSE updates (high confidence) are not overwritten by polling (low confidence)
- Optional: polling is disabled for agents with active SSE connections

---

### Task 3.1: SSE events should have StateConfidenceHigh (already done)

**Files**: `internal/adapters/opencode_events.go`

**Description**:
Verify that SSE events are already mapped to high-confidence states. This is already implemented but we should verify and document it.

**Verification**:
```go
// In opencode_events.go - mapEventToState()
case OpenCodeEventSessionIdle:
    return models.AgentStateIdle, models.StateInfo{
        State:      models.AgentStateIdle,
        Confidence: models.StateConfidenceHigh,  // ✓ Already high
        Reason:     "OpenCode SSE: session idle",
        DetectedAt: now,
    }, true

case OpenCodeEventSessionBusy, OpenCodeEventToolStart:
    return models.AgentStateWorking, models.StateInfo{
        State:      models.AgentStateWorking,
        Confidence: models.StateConfidenceHigh,  // ✓ Already high
        Reason:     fmt.Sprintf("OpenCode SSE: %s", event.Type),
        DetectedAt: now,
    }, true
```

**Acceptance Criteria**:
- [x] All SSE events map to StateConfidenceHigh
- [ ] Add unit test to verify confidence levels
- [ ] Document confidence levels in code comments

---

### Task 3.2: StateEngine.UpdateState should respect confidence (don't downgrade)

**Files**: `internal/state/engine.go`

**Description**:
Modify StateEngine.UpdateState() to not overwrite high-confidence state with low-confidence state, unless the state actually changes. This prevents polling from undoing SSE updates.

**Changes Required**:

1. Add confidence comparison helper:
```go
// confidenceRank returns a numeric rank for comparison
func confidenceRank(c models.StateConfidence) int {
    switch c {
    case models.StateConfidenceHigh:
        return 3
    case models.StateConfidenceMedium:
        return 2
    case models.StateConfidenceLow:
        return 1
    default:
        return 0
    }
}
```

2. Modify UpdateState() to check confidence:
```go
func (e *Engine) UpdateState(ctx context.Context, agentID string, newState models.AgentState, info models.StateInfo, usage *models.UsageMetrics, diff *models.DiffMetadata) error {
    agent, err := e.repo.Get(ctx, agentID)
    if err != nil {
        if errors.Is(err, db.ErrAgentNotFound) {
            return ErrAgentNotFound
        }
        return err
    }

    previousState := agent.State
    
    // NEW: Don't downgrade confidence for same state
    if previousState == newState {
        // Same state - only update if new confidence is >= current
        if confidenceRank(info.Confidence) < confidenceRank(agent.StateInfo.Confidence) {
            e.logger.Debug().
                Str("agent_id", agentID).
                Str("state", string(newState)).
                Str("current_confidence", string(agent.StateInfo.Confidence)).
                Str("new_confidence", string(info.Confidence)).
                Msg("skipping low-confidence update for same state")
            return nil
        }
    }
    
    // State changed or confidence is same/higher - proceed with update
    agent.State = newState
    agent.StateInfo = info
    // ... rest of existing code ...
}
```

3. Add logging for debugging state conflicts.

**Testing**:
- SSE sets state to idle (high conf), polling tries to set working (low conf) → should stay idle
- SSE sets state to idle (high conf), SSE sets working (high conf) → should change to working
- Polling sets idle (low conf), SSE sets working (high conf) → should change to working
- State changes always succeed regardless of confidence

**Acceptance Criteria**:
- [ ] Same-state updates respect confidence
- [ ] State changes always succeed
- [ ] Debug logging shows skipped updates
- [ ] Unit tests cover all confidence combinations

---

### Task 3.3: Optionally skip polling for agents with active SSE connections

**Files**: `internal/state/poller.go`, `internal/forged/daemon.go`

**Description**:
As an optimization, skip polling for agents that have active SSE connections since SSE provides more timely and accurate state information.

**Changes Required**:

1. Add interface for checking SSE status:
```go
// In state/poller.go
type SSEWatchChecker interface {
    IsWatching(agentID string) bool
}

type PollerConfig struct {
    // ... existing fields ...
    
    // SSEWatcher is used to skip polling for SSE-watched agents
    SSEWatcher SSEWatchChecker
    
    // SkipSSEWatchedAgents controls whether to skip polling SSE-watched agents
    SkipSSEWatchedAgents bool
}
```

2. Modify shouldPoll() in poller:
```go
func (p *Poller) shouldPoll(agent *models.Agent, now time.Time) bool {
    // Skip if SSE watching is active for this agent
    if p.config.SkipSSEWatchedAgents && 
       p.config.SSEWatcher != nil && 
       p.config.SSEWatcher.IsWatching(agent.ID) {
        return false
    }
    
    // ... existing logic ...
}
```

3. Wire up in forged:
```go
pollerConfig := state.DefaultPollerConfig()
pollerConfig.SSEWatcher = eventWatcher
pollerConfig.SkipSSEWatchedAgents = true  // or from config
```

**Testing**:
- Agent with SSE connection → polling skipped
- Agent without SSE connection → polling continues
- SSE connection drops → polling resumes
- Config can disable this behavior

**Acceptance Criteria**:
- [ ] Polling is skipped for SSE-watched agents
- [ ] Polling resumes when SSE connection drops
- [ ] Behavior is configurable
- [ ] Non-OpenCode agents are always polled

---

## Epic 4: CLI Integration

> **Reference**: [scheduler-daemon.md](./scheduler-daemon.md) - Section 7

**Goal**: Help users understand if forged is running and provide helpful guidance when it's not.

**Current State**:
- CLI has no way to check if forged is running
- Users may not realize queued messages won't be dispatched
- No documentation about running forged

**End State**:
- CLI can check forged status
- Helpful warnings when forged is not running
- Clear documentation

---

### Task 4.1: Add `forge daemon status` command

**Files**: `internal/cli/daemon.go` (new file)

**Description**:
Add a command to check if forged is running and display its status.

**Changes Required**:

1. Create new CLI command:
```go
// internal/cli/daemon.go
package cli

import (
    "github.com/spf13/cobra"
    "github.com/tOgg1/forge/internal/forged"
)

func init() {
    rootCmd.AddCommand(daemonCmd)
    daemonCmd.AddCommand(daemonStatusCmd)
}

var daemonCmd = &cobra.Command{
    Use:   "daemon",
    Short: "Manage the forged daemon",
    Long:  "Commands for managing the forged background daemon.",
}

var daemonStatusCmd = &cobra.Command{
    Use:   "status",
    Short: "Check if forged is running",
    RunE: func(cmd *cobra.Command, args []string) error {
        ctx := cmd.Context()
        
        // Try to connect to forged
        client, err := forged.Dial(ctx, forged.DefaultAddress())
        if err != nil {
            fmt.Println("forged is not running")
            fmt.Println()
            fmt.Println("Start it with: forged")
            fmt.Println()
            fmt.Println("Without forged, queued messages will not be dispatched automatically.")
            return nil
        }
        defer client.Close()
        
        // Get status
        status, err := client.GetStatus(ctx)
        if err != nil {
            return fmt.Errorf("failed to get status: %w", err)
        }
        
        fmt.Println("forged is running")
        fmt.Printf("  Version:   %s\n", status.Version)
        fmt.Printf("  Uptime:    %s\n", status.Uptime)
        fmt.Printf("  Agents:    %d\n", status.AgentCount)
        fmt.Printf("  Scheduler: %s\n", status.SchedulerStatus)
        return nil
    },
}
```

2. Add GetStatus RPC to forged if not exists:
```go
// May need to add to forged/server.go and proto definition
```

**Testing**:
- Run without forged → shows "not running"
- Run with forged → shows status
- JSON output format works

**Acceptance Criteria**:
- [ ] `forge daemon status` works
- [ ] Shows helpful message when forged not running
- [ ] Shows useful info when forged is running
- [ ] JSON output supported

---

### Task 4.2: CLI commands warn if forged is not running

**Files**: `internal/cli/send.go`, `internal/cli/queue.go`

**Description**:
When users queue messages, warn them if forged is not running so they understand the message won't be dispatched automatically.

**Changes Required**:

1. Add helper function:
```go
// internal/cli/helpers.go
func checkForgedRunning() bool {
    ctx, cancel := context.WithTimeout(context.Background(), 1*time.Second)
    defer cancel()
    
    client, err := forged.Dial(ctx, forged.DefaultAddress())
    if err != nil {
        return false
    }
    client.Close()
    return true
}

func warnIfForgedNotRunning() {
    if !checkForgedRunning() {
        fmt.Fprintln(os.Stderr, "⚠ forged not running. Message queued but won't be dispatched.")
        fmt.Fprintln(os.Stderr, "  Run 'forged' in another terminal to enable autonomous dispatch.")
        fmt.Fprintln(os.Stderr, "")
    }
}
```

2. Call in send command:
```go
// In send.go, after successful queue
warnIfForgedNotRunning()
fmt.Printf("✓ Queued for agent %s at position #%d\n", shortID(agentID), position)
```

3. Make warning configurable:
```go
// Skip warning with --quiet or if explicitly disabled in config
if !quiet && cfg.CLI.WarnForgedNotRunning {
    warnIfForgedNotRunning()
}
```

**Testing**:
- Queue without forged → warning shown
- Queue with forged → no warning
- `--quiet` suppresses warning
- Warning doesn't break JSON output

**Acceptance Criteria**:
- [ ] Warning shown when forged not running
- [ ] Warning is helpful and actionable
- [ ] Warning can be suppressed
- [ ] Doesn't break scripts (exit code still 0)

---

### Task 4.3: Update docs to indicate forged is required for queue dispatch

**Files**: `README.md`, `docs/` files

**Description**:
Update documentation to explain that forged must be running for messages to be dispatched automatically.

**Changes Required**:

1. Update README.md quick start:
```markdown
## Quick Start

1. Initialize Forge:
   ```bash
   forge init
   ```

2. Start the daemon (in a separate terminal):
   ```bash
   forged
   ```

3. Create a workspace and spawn an agent:
   ```bash
   forge up
   ```

4. Send a message:
   ```bash
   forge send "Fix the failing tests"
   ```

The daemon will automatically dispatch the message when the agent becomes idle.
```

2. Add architecture explanation:
```markdown
## Architecture

Forge consists of:
- `forge` - CLI for managing agents and workspaces
- `forged` - Daemon that handles scheduling and real-time state detection

The daemon must be running for:
- Automatic message dispatch to idle agents
- Real-time state updates via SSE (for OpenCode agents)
- Resource monitoring
```

**Acceptance Criteria**:
- [ ] README explains forged requirement
- [ ] Quick start includes forged
- [ ] Architecture is documented
- [ ] Common issues section mentions forged

---

## Epic 5: Production Readiness

> **Reference**: [scheduler-daemon.md](./scheduler-daemon.md) - Configuration section

**Goal**: Make it easy to run forged as a system service that starts automatically.

---

### Task 5.1: Add systemd service file for forged

**Files**: `contrib/systemd/forged.service` (new file)

**Description**:
Create a systemd service file for running forged on Linux systems.

**Changes Required**:

1. Create service file:
```ini
# contrib/systemd/forged.service
[Unit]
Description=Forge Daemon - Agent Orchestration Service
After=network.target

[Service]
Type=simple
User=%i
ExecStart=/usr/local/bin/forged
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal

# Security hardening
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

2. Add installation instructions:
```markdown
# Installing as systemd service

1. Copy the service file:
   ```bash
   sudo cp contrib/systemd/forged.service /etc/systemd/system/forged@.service
   ```

2. Enable and start for your user:
   ```bash
   sudo systemctl enable forged@$USER
   sudo systemctl start forged@$USER
   ```

3. Check status:
   ```bash
   systemctl status forged@$USER
   ```
```

**Acceptance Criteria**:
- [ ] Service file created
- [ ] Service starts successfully
- [ ] Service restarts on failure
- [ ] Installation documented

---

### Task 5.2: Add launchd plist for macOS

**Files**: `contrib/launchd/com.forge.forged.plist` (new file)

**Description**:
Create a launchd plist for running forged on macOS.

**Changes Required**:

1. Create plist file:
```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.forge.forged</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/forged</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/forged.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/forged.err</string>
</dict>
</plist>
```

2. Add installation instructions.

**Acceptance Criteria**:
- [ ] Plist file created
- [ ] Service starts on login
- [ ] Service restarts on failure
- [ ] Installation documented

---

### Task 5.3: Document forged setup in README

**Files**: `README.md`

**Description**:
Comprehensive documentation for setting up and running forged.

**Changes Required**:

1. Add section on running forged:
```markdown
## Running the Daemon

### Development / Manual

Run in foreground (recommended for development):
```bash
forged
```

### Production / Background

#### Linux (systemd)
See [contrib/systemd/README.md](contrib/systemd/README.md)

#### macOS (launchd)
See [contrib/launchd/README.md](contrib/launchd/README.md)

### Configuration

Configure via `~/.config/forge/config.yaml`:

```yaml
scheduler:
  enabled: true
  tick_interval: 1s
```

Or command-line flags:
```bash
forged --no-scheduler  # Disable scheduler
forged --log-level debug  # Verbose logging
```
```

**Acceptance Criteria**:
- [ ] README has forged section
- [ ] All run modes documented
- [ ] Configuration options explained
- [ ] Troubleshooting tips included

---

## Summary

| Epic | Tasks | Priority | Est. Effort |
|------|-------|----------|-------------|
| 1. Add Scheduler to forged | 5 | High | 3-4 days |
| 2. Integrate SSE Event Watcher | 5 | High | 2-3 days |
| 3. State Detection Priority | 3 | Medium | 1-2 days |
| 4. CLI Integration | 3 | Medium | 1 day |
| 5. Production Readiness | 3 | Low | 1 day |

**Total estimated effort**: 8-11 days

**Recommended implementation order**:
1. Tasks 1.1-1.3 (get basic scheduler running)
2. Tasks 2.1-2.3 (get SSE watching working)
3. Task 3.2 (prevent state conflicts)
4. Tasks 1.4-1.5, 2.4-2.5 (polish)
5. Epic 4 (CLI improvements)
6. Epic 5 (production readiness)
