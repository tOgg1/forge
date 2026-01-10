---
loop_id: b14582b2-47c3-470c-88a9-dca507966f06
loop_name: Brisk Lisa
repo_path: /root/Code/oss--forge/repos/forge
created_at: 2026-01-10T17:38:37Z
---

# Loop Ledger: Brisk Lisa

## 2026-01-10T17:44:17Z

- run_id: 9d4c9b9a-6933-4324-939d-80af2a9ac2e1
- loop_name: Brisk Lisa
- status: success
- profile: cc1
- harness: claude
- prompt_source: base
- prompt_path: /root/Code/oss--forge/repos/forge/PROMPT.md
- prompt_override: false
- started_at: 2026-01-10T17:38:37Z
- finished_at: 2026-01-10T17:44:17Z
- exit_code: 0

```
I've created a comprehensive specification for Forge Mail. Here's a summary of what I've produced:

## Documents Created

**`docs/forge-mail/README.md`** - Quick overview and entry point

**`docs/forge-mail/SPEC.md`** - Full specification covering:
- Core concepts: Projects, Agents, Topics, Direct Messages
- Two-mode architecture (Standalone vs Connected with forged)
- CLI commands: send, log, watch, wait, who, inbox, gc, help
- Message format (simple JSON)
- Storage layout (`.fmail/` directory structure)
- Environment variables (just 4)
- Forged integration and gRPC protocol
- Robot-help output format
- Usage patterns and examples
- Comparison with mcp_agent_mail
- Design decisions and FAQ

**`docs/forge-mail/DESIGN.md`** - Design notes covering:
- Unix philosophy alignment
- Key decisions with rationale and trade-offs
- Edge cases (concurrent writes, name validation, etc.)
- Implementation notes
- Future considerations
- Testing strategy

**`docs/forge-mail/ROBOT-HELP.md`** - Machine-readable help format:
- Complete JSON schema for `fmail --robot-help`
- Every command with syntax, args, flags, examples
- Common patterns for AI agents
- Environment variable documentation

## Key Improvements from v1.1

1. **Clearer architecture** - Standalone vs Connected modes explained
2. **Project ID concept** - For cross-host coordination
3. **Simpler message format** - Dropped metadata and reply fields for v1
4. **gRPC protocol defined** - For forged integration
5. **Comprehensive robot-help** - AI agents can self-learn the tool
6. **Design rationale** - Every decision documented with trade-offs

The spec maintains the "SUPER simple" requirement - just `export FMAIL_AGENT=myname` and start using it, while still supporting advanced scenarios with forged for real-time cross-host messaging.
[<u[?1004l[?2004l[?25h[?25h
```

## 2026-01-10T17:50:09Z

- run_id: ff2bf29b-300e-4ae4-9654-329e8a1a5817
- loop_name: Brisk Lisa
- status: success
- profile: cc1
- harness: claude
- prompt_source: base
- prompt_path: /root/Code/oss--forge/repos/forge/PROMPT.md
- prompt_override: false
- started_at: 2026-01-10T17:44:47Z
- finished_at: 2026-01-10T17:50:09Z
- exit_code: 0

```
I've completed this iteration of improvements to the Forge Mail specification. Here's a summary of the changes made:

## Summary of Improvements (v2.1.0)

### Simplifications
1. **Removed redundant `inbox` command** - `fmail log @$FMAIL_AGENT` serves the same purpose
2. **Removed separate `wait` command** - `fmail watch --count 1` replaces it
3. **Replaced gRPC with JSON lines** - Much simpler protocol for forged integration
4. **Reduced env vars** from 4 to 3 (removed `FMAIL_HOST`, forged auto-discovered via socket)

### New Features Added
1. **`fmail status`** - Set your status visible in `fmail who`
2. **`fmail topics`** - List all topics with activity stats
3. **`fmail init`** - Explicit project initialization (optional)
4. **`--reply-to` flag** - Simple message correlation without full threading
5. **`--priority` flag** - Mark messages as low/normal/high priority
6. **`--count` flag on watch** - Exit after N messages (replaces wait command)
7. **`--follow` flag on log** - Stream new messages like `tail -f`

### Improved Design Decisions
1. **Timestamp-based IDs** - `YYYYMMDD-HHMMSS-NNNN` format is globally sortable and human-readable
2. **Cleaner storage layout** - Separate `topics/` and `dm/` directories
3. **Faster polling** - 100ms instead of 500ms for standalone mode
4. **Simpler protocol** - JSON lines over Unix socket instead of gRPC

### Documentation Improvements
1. **More compact ROBOT-HELP.md** - Easier for agents to parse
2. **Better patterns section** - Real-world usage examples
3. **Clearer architecture diagrams** - Standalone vs connected mode
4. **Updated FAQ** - More concise answers

The spec now better embodies the core philosophy: **zero friction, files as truth, shell-native, optional enhancement**.
[<u[?1004l[?2004l[?25h[?25h
```

