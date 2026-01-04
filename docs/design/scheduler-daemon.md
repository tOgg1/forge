# Scheduler Daemon Design

## Overview

This document describes the design for integrating the scheduler and SSE event watching into the existing `forged` daemon. This enables autonomous agent management - dispatching queued messages when agents become idle and receiving real-time state updates via SSE events from OpenCode agents.

## Problem Statement

Currently, Forge CLI commands are short-lived processes. When you run `forge send "message"`:
1. The message is queued in SQLite
2. The CLI exits immediately
3. **Nothing dispatches the message to the agent**

The scheduler logic exists (`internal/scheduler/`) but has no persistent process to run it. Additionally, OpenCode agents emit SSE events for state changes, but nothing listens to them.

## Goals

1. **Autonomous Operation**: Agents should receive queued messages without human intervention
2. **Real-time State Detection**: Use SSE events from OpenCode for high-confidence state updates
3. **Reliability**: Handle restarts, reconnections, and failures gracefully
4. **Single Daemon**: Integrate into existing `forged` rather than adding another process

## Why Integrate into forged?

The `forged` daemon already:
- Manages agent lifecycle (spawn, kill, send input)
- Streams pane content updates
- Monitors resources (CPU, memory, disk)
- Runs a long-lived event loop

Adding the scheduler here means:
- Single daemon to manage instead of two
- Shared infrastructure (logging, config, shutdown handling)
- Natural place for SSE watching alongside pane streaming
- Users only need to run `forged` for full functionality

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              forged daemon                               │
│                                                                          │
│  ┌─────────────────────────────────────────────────────────────────────┐│
│  │                     NEW: Scheduler Components                        ││
│  │  ┌─────────────┐    ┌─────────────┐    ┌────────────────────────┐   ││
│  │  │  Scheduler  │◄───│ StateEngine │◄───│ OpenCodeEventWatcher   │   ││
│  │  │             │    │             │    │ (SSE connections)      │   ││
│  │  │ - tick loop │    │ - DB update │    │ - session.idle/busy    │   ││
│  │  │ - dispatch  │    │ - notify    │    │ - permission.requested │   ││
│  │  └──────┬──────┘    └─────────────┘    └────────────────────────┘   ││
│  │         │                                                            ││
│  │         ▼                                                            ││
│  │  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐              ││
│  │  │QueueService │    │AgentService │    │StatePoller  │              ││
│  │  │ - dequeue   │    │ - send msg  │    │ (fallback)  │              ││
│  │  └─────────────┘    └─────────────┘    └─────────────┘              ││
│  └─────────────────────────────────────────────────────────────────────┘│
│                                                                          │
│  ┌─────────────────────────────────────────────────────────────────────┐│
│  │                  EXISTING: forged Components                         ││
│  │  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐              ││
│  │  │gRPC Server  │    │ Resource    │    │Rate Limiter │              ││
│  │  │             │    │ Monitor     │    │             │              ││
│  │  └─────────────┘    └─────────────┘    └─────────────┘              ││
│  └─────────────────────────────────────────────────────────────────────┘│
│                                                                          │
└──────────────────────────────┬───────────────────────────────────────────┘
                               │
          ┌────────────────────┼────────────────────┐
          ▼                    ▼                    ▼
    ┌───────────┐        ┌───────────┐        ┌───────────┐
    │  SQLite   │        │   tmux    │        │ OpenCode  │
    │  Database │        │  panes    │        │ SSE APIs  │
    └───────────┘        └───────────┘        └───────────┘
```

## Components

### 1. Enhanced forged Daemon

The existing `forged` command gains scheduler capabilities:

```bash
# Run forged with scheduler (default)
forged

# Run without scheduler (gRPC-only mode)
forged --no-scheduler

# Check status
forge daemon status
```

### 2. Database Integration in forged

Currently forged is stateless. We add database access:

```go
// In forged/daemon.go - New()

// NEW: Open database for scheduler
database, err := db.Open(cfg.Database.Path)
if err != nil {
    return nil, fmt.Errorf("failed to open database: %w", err)
}

// Create repositories
agentRepo := db.NewAgentRepository(database)
queueRepo := db.NewQueueRepository(database)
wsRepo := db.NewWorkspaceRepository(database)
eventRepo := db.NewEventRepository(database)
```

### 3. Scheduler Initialization in forged

```go
// In forged/daemon.go - New()

// Create services for scheduler
tmuxClient := tmux.NewLocalClient()
registry := adapters.NewRegistry()
stateEngine := state.NewEngine(agentRepo, eventRepo, tmuxClient, registry)
queueService := queue.NewService(queueRepo)
nodeService := node.NewService(nodeRepo)
wsService := workspace.NewService(wsRepo, nodeService, agentRepo)
agentService := agent.NewService(agentRepo, queueRepo, wsService, nil, tmuxClient)

// Create SSE event watcher
eventWatcher := adapters.NewOpenCodeEventWatcher(
    adapters.DefaultEventWatcherConfig(),
    func(agentID string, newState models.AgentState, info models.StateInfo) {
        // Update state in database
        if err := stateEngine.UpdateState(ctx, agentID, newState, info, nil, nil); err != nil {
            logger.Warn().Err(err).Str("agent_id", agentID).Msg("failed to update state from SSE")
        }
    },
)

// Create scheduler
sched := scheduler.New(
    scheduler.DefaultConfig(),
    agentService,
    queueService,
    stateEngine,
    nil, // accountService (optional)
)

// Create state poller (fallback for non-OpenCode agents)
statePoller := state.NewPoller(state.DefaultPollerConfig(), stateEngine, agentRepo)
```

### 4. Starting Components in forged.Run()

```go
// In forged/daemon.go - Run()

// Start state poller
if err := d.statePoller.Start(ctx); err != nil {
    return fmt.Errorf("failed to start state poller: %w", err)
}
defer d.statePoller.Stop()

// Start scheduler
if err := d.scheduler.Start(ctx); err != nil {
    return fmt.Errorf("failed to start scheduler: %w", err)
}
defer d.scheduler.Stop()

// Start watching existing OpenCode agents
d.startWatchingExistingAgents(ctx)

// Start gRPC server (existing code)
// ...
```

### 5. OpenCode SSE Integration

The `OpenCodeEventWatcher` connects to each OpenCode agent's SSE endpoint:

```
http://127.0.0.1:<port>/events
```

Events received:
- `session.idle` → `AgentStateIdle` (high confidence)
- `session.busy` → `AgentStateWorking` (high confidence)
- `permission.requested` → `AgentStateAwaitingApproval` (high confidence)
- `error` → `AgentStateError` (high confidence)

**Auto-watching lifecycle**:

```go
// Subscribe to agent events for auto-watching
stateEngine.SubscribeFunc("sse-watcher", func(change state.StateChange) {
    agent, _ := agentRepo.Get(ctx, change.AgentID)
    if agent == nil {
        return
    }
    
    // Start watching when agent becomes active
    if change.CurrentState == models.AgentStateIdle && agent.HasOpenCodeConnection() {
        eventWatcher.WatchAgent(ctx, agent)
    }
    
    // Stop watching when agent terminates
    if change.CurrentState == models.AgentStateStopped {
        eventWatcher.Unwatch(change.AgentID)
    }
})

// On startup, watch all existing OpenCode agents
func (d *Daemon) startWatchingExistingAgents(ctx context.Context) {
    agents, _ := d.agentRepo.List(ctx)
    for _, agent := range agents {
        if agent.HasOpenCodeConnection() && agent.State != models.AgentStateStopped {
            d.eventWatcher.WatchAgent(ctx, agent)
        }
    }
}
```

### 6. State Priority

When both SSE and polling detect state:

| Source | Confidence | Priority |
|--------|------------|----------|
| OpenCode SSE | High | 1 (highest) |
| Adapter detection | Medium | 2 |
| Fallback heuristics | Low | 3 |

The `StateEngine.UpdateState` should respect confidence:

```go
func (e *Engine) UpdateState(ctx context.Context, agentID string, newState models.AgentState, info models.StateInfo, ...) error {
    agent, err := e.repo.Get(ctx, agentID)
    if err != nil {
        return err
    }
    
    // Don't downgrade confidence unless state actually changed
    if agent.State == newState && confidenceRank(info.Confidence) < confidenceRank(agent.StateInfo.Confidence) {
        return nil // Skip low-confidence update for same state
    }
    
    // Update state
    agent.State = newState
    agent.StateInfo = info
    // ... persist and notify
}
```

### 7. forged-CLI Communication

**PID File**: `~/.local/share/forge/forged.pid`

**gRPC Health Check**: forged already exposes gRPC - add a health/status method.

**CLI Warnings**:
```
$ forge send "message"
⚠ forged not running. Message queued but won't be dispatched.
  Run 'forged' in another terminal to enable autonomous dispatch.
✓ Queued for agent abc123 at position #1
```

## Implementation Plan

### Phase 1: Database Integration in forged (Tasks 1.1-1.2)
1. Add database connection to forged daemon
2. Initialize repositories (agent, queue, workspace, event)
3. Create StateEngine, AgentService, QueueService

### Phase 2: Scheduler Integration (Tasks 1.3-1.5)
1. Initialize and start Scheduler in forged.Run()
2. Add config options (--no-scheduler, intervals)
3. Ensure graceful shutdown order (scheduler → gRPC)

### Phase 3: SSE Integration (Tasks 2.1-2.5)
1. Create OpenCodeEventWatcher with state handler
2. Wire to StateEngine for DB persistence
3. Subscribe to agent events for auto-watching
4. Watch existing agents on startup

### Phase 4: State Priority (Tasks 3.1-3.3)
1. Verify SSE events have high confidence (already done)
2. Add confidence-aware state updates
3. Optionally skip polling for SSE-watched agents

### Phase 5: CLI Integration (Tasks 4.1-4.3)
1. Add `forge daemon status` command
2. CLI warnings when forged not running
3. Update documentation

## Configuration

Add to `~/.config/forge/config.yaml`:

```yaml
scheduler:
  # Enable scheduler in forged (default: true)
  enabled: true
  
  # How often to check for dispatchable items
  tick_interval: 1s
  
  # Maximum concurrent dispatches
  max_concurrent_dispatches: 10
  
  # Require agent to be idle before dispatch
  idle_state_required: true
  
  # Auto-resume paused agents
  auto_resume_enabled: true

state:
  # Polling intervals (fallback when SSE unavailable)
  active_poll_interval: 500ms
  idle_poll_interval: 2s
  
  # Prefer SSE over polling for OpenCode agents
  prefer_sse: true
```

forged command-line flags:

```bash
forged --no-scheduler      # Disable scheduler (gRPC-only mode)
forged --scheduler-tick 2s # Override tick interval
```

## Open Questions

1. **Should `forge up` auto-start forged if not running?**
   - Pro: Better UX for new users
   - Con: Implicit background process may surprise users
   - Decision: Start with explicit `forged` requirement, consider auto-start later

2. **How to handle the per-node vs control-plane distinction?**
   - For local-only usage (current): forged handles everything
   - For multi-node (future): May need separate control-plane scheduler
   - Decision: Design for local-only now, keep interfaces clean for future split

3. **How to handle multiple CLI instances queueing?**
   - Current: SQLite handles concurrent writes with WAL mode
   - The scheduler dequeues atomically
   - No changes needed

## Success Metrics

1. Messages are dispatched within 2 seconds of agent becoming idle
2. SSE state updates have <100ms latency
3. Daemon uses <50MB memory, <1% CPU when idle
4. Zero message loss on daemon restart
