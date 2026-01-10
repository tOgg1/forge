---
loop_id: f3184938-2d63-40ff-9b82-7ee236d402e4
loop_name: Stellar Krusty
repo_path: /root/Code/oss--forge/repos/forge
created_at: 2026-01-10T16:51:29Z
---

# Loop Ledger: Stellar Krusty

## 2026-01-10T16:55:34Z

- run_id: 192069a6-ee32-4066-8427-7ad9e6784226
- loop_name: Stellar Krusty
- status: success
- profile: cc1
- harness: claude
- prompt_source: base
- prompt_path: /root/Code/oss--forge/repos/forge/PROMPT.md
- prompt_override: false
- started_at: 2026-01-10T16:51:29Z
- finished_at: 2026-01-10T16:55:34Z
- exit_code: 0

```
I've updated the Forge Mail specification with several improvements. Here's a summary of the changes:

## Key Improvements Made

### 1. **Added Design Principles Section**
Explicit principles that guide all decisions: Files are the API, Zero config, Progressive enhancement, Agent-first, Composable.

### 2. **Simplified Directory Structure**
Changed `dm-<agent>/` to `@<agent>/` - the `@` is now part of the directory name, making the intent visible when browsing the filesystem.

### 3. **Added `inbox` Command**
New convenience command for agents to see all messages addressed to them. Simple alias for `fmail log @$FMAIL_AGENT`.

### 4. **Added `--re` Flag for Replies**
Lightweight threading without full thread support. Messages can reference the ID they're replying to.

### 5. **Fixed `--robot-help` Flag**
Changed from `--robot` to `--robot-help` as specified in the requirements.

### 6. **Better Wildcard Support**
Added `@*` to match all direct messages.

### 7. **Cleaner Environment Variables**
- Renamed `FMAIL_PROJECT` to `FMAIL_ROOT` (clearer meaning)
- Better documentation of auto-detection behavior

### 8. **Improved Forged Integration Section**
- Clear table showing file-only vs forged features
- Cross-host sync includes `host` field in messages

### 9. **Added FAQ Section**
Answers common questions about concurrency, acknowledgments, standalone usage, cleanup, and message size limits.

### 10. **Cleaner Examples**
- Simplified the scripting patterns
- Better request/response examples
- More practical build pipeline example

### 11. **Comparison Table**
Clear side-by-side with mcp_agent_mail showing the simplicity wins.

The spec is now ~780 lines but more comprehensive and better organized. The core simplicity remains: files in directories, debug with `ls` and `cat`.
```

