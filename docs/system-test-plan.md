# Forge System Test Plan

This test plan prioritizes the **happy path** and core user workflows. Tests are ordered by importance - complete the Critical Path tests before moving to secondary tests.

---

## Table of Contents

1. [Prerequisites](#1-prerequisites)
2. [Critical Path: Happy Path Tests](#2-critical-path-happy-path-tests)
3. [Core Workflow Tests](#3-core-workflow-tests)
4. [TUI Live Experience Tests](#4-tui-live-experience-tests)
5. [Multi-Agent Orchestration Tests](#5-multi-agent-orchestration-tests)
6. [Secondary Tests](#6-secondary-tests)
7. [Test Results Template](#7-test-results-template)

---

## 1. Prerequisites

### 1.1 Environment Check

```bash
# Run the doctor command first
forge doctor

# Expected output should show:
# ✓ tmux 3.x+
# ✓ git 2.x+
# ✓ opencode (for OpenCode agents)
# ✓ Database accessible
# ✓ Migrations applied
```

### 1.2 Quick Build & Verify

```bash
# Build
make build

# Verify
./build/forge --version
./build/forge doctor
```

---

## 2. Critical Path: Happy Path Tests

> **These are the most important tests.** They validate the core user journey from UX_FEEDBACK_2.md: "I'm in a repo → create/open workspace → spawn OpenCode agent → send instructions → watch status"

### 2.1 Test: Single Agent Happy Path (MUST PASS)

This is the golden path. If this doesn't work, nothing else matters.

```bash
# 1. Start in a git repository
cd /path/to/your/repo
git status  # Verify it's a git repo

# 2. Initialize Forge (one-time setup)
forge migrate up

# 3. Create workspace for current repo
forge ws create --path .
WS_ID=$(forge ws list --json | jq -r '.[0].id')
echo "Workspace ID: $WS_ID"

# 4. Spawn an OpenCode agent
forge agent spawn --workspace $WS_ID --type opencode
AGENT_ID=$(forge agent list --workspace $WS_ID --json | jq -r '.[0].id')
echo "Agent ID: $AGENT_ID"

# 5. Send a simple instruction
forge send $AGENT_ID "List all files in this repository"

# 6. Check agent status
forge ps
# Expected: Agent shows state (idle, working, etc.)

# 7. Attach to see the agent working
forge attach $AGENT_ID
# Press Ctrl-b d to detach

# 8. Clean up
forge agent terminate $AGENT_ID
forge ws remove $WS_ID --destroy
```

**Expected Results:**
- [ ] Workspace created successfully
- [ ] Agent spawned in tmux pane
- [ ] Message sent to agent
- [ ] Agent processes the message
- [ ] Status shows correct state
- [ ] Can attach and see agent output

### 2.2 Test: TUI Happy Path (MUST PASS)

```bash
# 1. Setup (if not already done)
forge ws create --path /path/to/repo
WS_ID=$(forge ws list --json | jq -r '.[0].id')
forge agent spawn --workspace $WS_ID --type opencode
AGENT_ID=$(forge agent list --json | jq -r '.[0].id')

# 2. Launch TUI
forge ui

# 3. In TUI, verify:
#    - Press 3 to go to Agent view
#    - Agent card shows with correct state
#    - Press Enter on agent to see details
#    - Press t to see transcript
#    - Press Q to see queue
#    - Press q to quit

# 4. Send message from another terminal while TUI is open
forge send $AGENT_ID "What is 2+2?"

# 5. Watch TUI update:
#    - Agent state should change to "working"
#    - Then back to "idle" when done
#    - No manual refresh needed!
```

**Expected Results:**
- [ ] TUI launches without errors
- [ ] Agent cards display correctly
- [ ] State updates appear automatically (within 5 seconds)
- [ ] Navigation works (j/k, numbers, Enter)
- [ ] Transcript shows agent output

### 2.3 Test: Queue-First Workflow (MUST PASS)

From UX_FEEDBACK_3.md: "send" should mean **enqueue + scheduler dispatch**

```bash
# 1. Setup agent
AGENT_ID=$(forge agent list --json | jq -r '.[0].id')

# 2. Queue multiple messages (not immediate injection)
forge send $AGENT_ID "First: List all Go files"
forge send $AGENT_ID "Second: Count lines of code"
forge send $AGENT_ID "Third: Find TODO comments"

# 3. Check queue
forge queue list --agent $AGENT_ID
# Expected: 3 items queued (or 2 if first already dispatched)

# 4. Watch agent process queue
forge ps --watch
# Expected: Agent works through queue items in order
```

**Expected Results:**
- [ ] Messages queued, not immediately injected
- [ ] Queue shows pending items
- [ ] Agent processes items in order
- [ ] Queue empties as items complete

---

## 3. Core Workflow Tests

### 3.1 Test: Agent State Detection

The TUI should answer these questions within 2 seconds:
- What is working?
- What is stuck?
- What needs my permission?
- What is cooling down?

```bash
# 1. Spawn agent and send work
AGENT_ID=$(forge agent list --json | jq -r '.[0].id')
forge send $AGENT_ID "Analyze this codebase in detail"

# 2. Check state (should be "working")
forge agent status $AGENT_ID --json | jq '.state'
# Expected: "working"

# 3. Wait for completion, check again
sleep 30
forge agent status $AGENT_ID --json | jq '.state'
# Expected: "idle"

# 4. Check state info
forge agent status $AGENT_ID --json | jq '.state_info'
# Expected: Shows confidence, reason, evidence
```

**Expected Results:**
- [ ] Working state detected while agent is processing
- [ ] Idle state detected when agent is done
- [ ] State confidence is medium or high
- [ ] State reason explains why

### 3.2 Test: Templates and Sequences

```bash
# 1. List available templates
forge template list
# Expected: Shows built-in templates (continue, review, etc.)

# 2. Run a template
AGENT_ID=$(forge agent list --json | jq -r '.[0].id')
forge template run continue --agent $AGENT_ID
# Expected: Template message queued

# 3. List sequences
forge seq list
# Expected: Shows built-in sequences

# 4. Run a sequence
forge seq run baseline --agent $AGENT_ID
# Expected: All sequence steps queued
```

### 3.3 Test: Message Palette in TUI

```bash
# 1. Launch TUI
forge ui

# 2. Press Ctrl+P to open message palette
# Expected: Shows templates and sequences

# 3. Select a template with Enter
# Expected: Prompts for target agent

# 4. Select agent and confirm
# Expected: Message queued to agent
```

---

## 4. TUI Live Experience Tests

### 4.1 Test: Real-time State Updates

```bash
# Terminal 1: Launch TUI
forge ui

# Terminal 2: Trigger state changes
AGENT_ID=$(forge agent list --json | jq -r '.[0].id')
forge send $AGENT_ID "Count to 10 slowly"

# Watch Terminal 1:
# - Agent state should change from idle → working
# - "Last updated" timestamp should update
# - No need to press 'r' to refresh
```

**Expected Results:**
- [ ] State change appears within 5 seconds
- [ ] No manual refresh needed
- [ ] Timestamp updates on state change

### 4.2 Test: Multi-Select and Bulk Actions

```bash
# 1. Launch TUI with multiple agents
forge ui

# 2. Go to Agent view (press 3)

# 3. Multi-select:
#    - Space: toggle selection on current agent
#    - Shift+Space: select range
#    - Ctrl+A: select all

# 4. Bulk action:
#    - T: open message palette for selected agents
#    - P: pause all selected
#    - K: terminate all selected (with confirm)
```

### 4.3 Test: Queue Editor

```bash
# 1. In TUI, select an agent and press Q
# Expected: Queue editor opens

# 2. Add items:
#    - i: insert message
#    - p: insert pause
#    - t: insert from template

# 3. Reorder:
#    - J/K: move item down/up

# 4. Delete:
#    - d: delete item (with confirm)
```

### 4.4 Test: Inspector Panel

```bash
# 1. In TUI, press Tab or i
# Expected: Inspector panel toggles

# 2. Navigate to different agents
# Expected: Inspector shows details for selected agent

# 3. Check inspector shows:
#    - Agent ID and name
#    - Current state with confidence
#    - Queue length
#    - Account info
#    - Recent activity
```

---

## 5. Multi-Agent Orchestration Tests

### 5.1 Test: Spawn Multiple Agents

```bash
# 1. Spawn 3 agents
forge agent spawn --workspace $WS_ID --type opencode --count 3

# 2. Verify all spawned
forge agent list --workspace $WS_ID
# Expected: 3 agents listed

# 3. Check TUI shows all agents
forge ui
# Press 3 for agent view
# Expected: All 3 agents visible with cards
```

### 5.2 Test: Parallel Work Assignment

```bash
# 1. Get agent IDs
AGENTS=$(forge agent list --json | jq -r '.[].id')

# 2. Assign different tasks
echo "$AGENTS" | head -1 | xargs -I {} forge send {} "Review auth code"
echo "$AGENTS" | sed -n 2p | xargs -I {} forge send {} "Review database code"
echo "$AGENTS" | tail -1 | xargs -I {} forge send {} "Review API handlers"

# 3. Monitor all agents
forge ps
# Expected: All agents working on different tasks
```

### 5.3 Test: Recipe-Based Spawning

```bash
# 1. List available recipes
forge recipe list

# 2. Run a recipe (spawns multiple configured agents)
forge recipe run baseline --workspace $WS_ID

# 3. Verify agents spawned with correct configuration
forge agent list --workspace $WS_ID
```

---

## 6. Secondary Tests

These tests are important but should be run after the critical path passes.

### 6.1 Unit Tests

```bash
# Run all unit tests
go test ./... -v

# Expected: All tests pass
# Key packages:
# - internal/state: State detection
# - internal/scheduler: Queue dispatch
# - internal/tmux: tmux operations
# - internal/adapters: Agent adapters
```

### 6.2 Account and Credential Tests

```bash
# Add account
forge accounts add --provider anthropic --profile work \
  --credential-ref 'env:ANTHROPIC_API_KEY' --non-interactive

# List accounts
forge accounts list

# Set cooldown
forge accounts cooldown set <account-id> --until 30m

# Clear cooldown
forge accounts cooldown clear <account-id>
```

### 6.3 Vault Tests

```bash
# Initialize vault
forge vault init

# Backup credentials
forge vault backup claude work

# List profiles
forge vault list

# Activate profile
forge vault activate claude work
```

### 6.4 Remote Node Tests (Optional)

```bash
# Add remote node
forge node add --name remote --ssh user@hostname

# Run doctor on remote
forge node doctor remote

# Create workspace on remote
forge ws create --node remote --path /home/user/project

# Spawn agent on remote
forge agent spawn --workspace $REMOTE_WS_ID --type opencode
```

### 6.5 Mail and Lock Tests

```bash
# Send mail between agents
forge mail send --to agent-2 --subject "Handoff" --body "Please review my changes"

# Check inbox
forge mail inbox --agent agent-2

# Acquire file lock
forge lock acquire --path src/main.go --ttl 30m

# Check locks
forge lock list

# Release lock
forge lock release --path src/main.go
```

### 6.6 Database Tests

```bash
# Check migrations
forge migrate status

# Verify schema
sqlite3 ~/.local/share/forge/forge.db ".tables"
# Expected: nodes, workspaces, agents, accounts, queue_items, events, etc.
```

### 6.7 Performance Tests

```bash
# Spawn latency
time forge agent spawn --workspace $WS_ID --type opencode --count 1
# Target: < 2 seconds

# Queue throughput
for i in {1..100}; do echo "Task $i"; done > /tmp/tasks.txt
time forge queue add --agent $AGENT_ID --file /tmp/tasks.txt
# Target: < 1 second

# TUI responsiveness with many agents
# Create 20+ agents, navigate quickly
# Target: No perceptible lag
```

---

## 7. Test Results Template

### Critical Path Results

| Test | Status | Notes |
|------|--------|-------|
| 2.1 Single Agent Happy Path | ⬜ | |
| 2.2 TUI Happy Path | ⬜ | |
| 2.3 Queue-First Workflow | ⬜ | |

### Core Workflow Results

| Test | Status | Notes |
|------|--------|-------|
| 3.1 Agent State Detection | ⬜ | |
| 3.2 Templates and Sequences | ⬜ | |
| 3.3 Message Palette in TUI | ⬜ | |

### TUI Experience Results

| Test | Status | Notes |
|------|--------|-------|
| 4.1 Real-time State Updates | ⬜ | |
| 4.2 Multi-Select and Bulk Actions | ⬜ | |
| 4.3 Queue Editor | ⬜ | |
| 4.4 Inspector Panel | ⬜ | |

### Multi-Agent Results

| Test | Status | Notes |
|------|--------|-------|
| 5.1 Spawn Multiple Agents | ⬜ | |
| 5.2 Parallel Work Assignment | ⬜ | |
| 5.3 Recipe-Based Spawning | ⬜ | |

### Environment

```
Date: YYYY-MM-DD
Tester: 
Forge Version: 
OS: 
tmux Version: 
```

### Issues Found

| ID | Severity | Description | Steps to Reproduce |
|----|----------|-------------|-------------------|
| | | | |

### Notes

- 
- 

---

## Quick Reference: Key Commands

```bash
# Happy path commands
forge doctor                    # Check environment
forge ws create --path .        # Create workspace
forge agent spawn --workspace $WS_ID --type opencode  # Spawn agent
forge send $AGENT_ID "message"  # Send message (queued)
forge ps                        # List agents with status
forge attach $AGENT_ID          # Attach to agent tmux pane
forge ui                        # Launch TUI

# TUI shortcuts
3           # Agent view
Tab / i     # Toggle inspector
t           # Toggle transcript
Q           # Queue editor
Ctrl+P      # Message palette
Ctrl+K / :  # Command palette
Space       # Toggle selection
q           # Quit
```
