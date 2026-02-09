---
loop_id: 7bc822bf-8f16-4273-a292-9f55bf5b66da
loop_name: forge-fmail-next-4
repo_path: /Users/trmd/Code/oss--forge/repos/forge
created_at: 2026-02-09T08:45:35Z
---

# Loop Ledger: forge-fmail-next-4

## 2026-02-09T11:00:23Z

- run_id: 6b8cf2b6-5d3e-4aa4-9438-d92d0245c8ae
- loop_name: forge-fmail-next-4
- status: success
- profile: codex2
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/fmail-next-task.md
- prompt_override: false
- started_at: 2026-02-09T10:15:06Z
- finished_at: 2026-02-09T11:00:23Z
- exit_code: 0

```
diff --git a/internal/teammsg/fmail_test.go b/internal/teammsg/fmail_test.go
new file mode 100644
index 0000000000000000000000000000000000000000..76e55e9cde13fa6f4acd790db7415519597810aa
--- /dev/null
+++ b/internal/teammsg/fmail_test.go
@@ -0,0 +1,41 @@
+package teammsg
+
+import (
+	"testing"
+
+	"github.com/stretchr/testify/require"
+
+	"github.com/tOgg1/forge/internal/fmail"
+)
+
+type fakeSaver struct {
+	last *fmail.Message
+	err  error
+}
+
+func (s *fakeSaver) SaveMessage(message *fmail.Message) (string, error) {
+	s.last = message
+	return "id", s.err
+}
+
+func TestFmailMessenger_SendTaskPrefixesAt(t *testing.T) {
+	saver := &fakeSaver{}
+	m := &fmailMessenger{from: "sender", save: saver}
+	require.NoError(t, m.SendTask("agent-1", "do it"))
+	require.NotNil(t, saver.last)
+	require.Equal(t, "sender", saver.last.From)
+	require.Equal(t, "@agent-1", saver.last.To)
+	require.Equal(t, "do it", saver.last.Body)
+}
+
+func TestFmailMessenger_SendTopicRejectsDMTarget(t *testing.T) {
+	saver := &fakeSaver{}
+	m := &fmailMessenger{from: "sender", save: saver}
+	require.Error(t, m.SendTopic("@agent-1", "x"))
+}
+
+func TestFmailMessenger_EmptyBodyRejected(t *testing.T) {
+	saver := &fakeSaver{}
+	m := &fmailMessenger{from: "sender", save: saver}
+	require.Error(t, m.SendTopic("task", " "))
+}

tokens used
355,908
Hi Tormod. Keep loop tight; keep shipping.

Closed (sv + fmail):
- `forge-by0` (theme/render polish): `Ctrl+T` theme cycle (persist), per-theme agent palettes/borders, URL underline, thread cards (reply-to agent, priority/star header, annotation box, 2-char border) + cache; live-tail uses shared body renderer.
- `forge-84q` (bookmarks/annotations): `Ctrl+B` Bookmarks view + markdown export; Thread: `b` toggle (confirm remove), `B` note edit, `a` annotation edit, `X` thread export; Timeline: `B` bookmark note; Search: `has:annotation` + `b` toggles bookmark; state CRUD + tests.
- `sv-g7c` (fmail integration): new `internal/teammsg` fmail adapter (noop if `FMAIL_AGENT` unset) + unit tests; `forge send` mirrors queued message as fmail DM to `@<agentID>`; `forge work set` broadcasts status to topic `${FORGE_FMAIL_WORK_TOPIC:-task}` (best-effort, warn-only).

Tests:
- `go test ./...` fails due local toolchain mismatch (`go`=1.25.6, `GOROOT` points at 1.25.7).
- `env -u GOROOT -u GOTOOLDIR go test ./...` passes.
```

## 2026-02-09T12:14:13Z

- run_id: 8cdc7f37-faea-4538-bded-580a2753f4e4
- loop_name: forge-fmail-next-4
- status: success
- profile: codex2
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/fmail-next-task.md
- prompt_override: false
- started_at: 2026-02-09T12:05:17Z
- finished_at: 2026-02-09T12:14:13Z
- exit_code: 0

```
+			if !strings.EqualFold(message.From, normalized) && target != strings.ToLower(normalized) {
+				return false
+			}
+		}
+	}
+
+	return messageMatchesFilter(&message, MessageFilter{
+		Since:    filter.Since,
+		From:     filter.From,
+		Priority: filter.Priority,
+		Tags:     filter.Tags,
+	})
+}
+
diff --git a/internal/fmailtui/data/provider.go b/internal/fmailtui/data/provider.go
index 0cd4dbf5cd0eac18a776a53c7f8c0101be3b70cf..18fb334ad9dd7b952a92f9fbb76aef6231bf900a
--- a/internal/fmailtui/data/provider.go
+++ b/internal/fmailtui/data/provider.go
@@ -12,6 +12,7 @@
 const (
 	defaultCacheTTL            = 500 * time.Millisecond
 	defaultPollInterval        = 100 * time.Millisecond
+	defaultPollMax             = 2 * time.Second
 	defaultReconnectInterval   = 2 * time.Second
 	defaultSubscribeBufferSize = 256
 	defaultMessageCacheSize    = 2048
@@ -110,7 +111,10 @@
 type FileProviderConfig struct {
 	Root            string
 	CacheTTL        time.Duration
+	// PollInterval controls the minimum poll cadence used by Subscribe().
 	PollInterval    time.Duration
+	// PollMax controls the maximum poll cadence used by Subscribe() backoff.
+	PollMax         time.Duration
 	CacheCapacity   int
 	SubscribeBuffer int
 	SelfAgent       string

tokens used
139,322
Editing provider wiring next: pass TUI `--poll-interval` into file provider min poll, add `poll-max` knob; then add subscription backoff/coalesce tests.俺去也assistant to=functions.apply_patch մեկնաբանություն code
*** Begin Patch
*** Update File: internal/fmailtui/build_provider.go
@@
 const defaultFileProviderPollInterval = 100 * time.Millisecond
 
-func buildProvider(root, forgedAddr, selfAgent string) (data.MessageProvider, error) {
+func buildProvider(root, forgedAddr, selfAgent string, pollInterval time.Duration) (data.MessageProvider, error) {
 	selfAgent = strings.TrimSpace(selfAgent)
+	if pollInterval <= 0 {
+		pollInterval = defaultFileProviderPollInterval
+	}
 	fileProvider, err := data.NewFileProvider(data.FileProviderConfig{
 		Root:         root,
 		SelfAgent:    selfAgent,
-		PollInterval: 100 * time.Millisecond,
+		PollInterval: pollInterval,
+		PollMax:      2 * time.Second,
 	})
*** End Patch
```

