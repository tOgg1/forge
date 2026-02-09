---
loop_id: 47c32ed6-092e-47e4-8c1c-1a93bd68c26a
loop_name: forge-fmail-v2-next-2
repo_path: /Users/trmd/Code/oss--forge/repos/forge
created_at: 2026-02-09T12:06:16Z
---

# Loop Ledger: forge-fmail-v2-next-2

## 2026-02-09T12:20:52Z

- run_id: 8b2db0c4-0481-4f80-9548-612450e9793d
- loop_name: forge-fmail-v2-next-2
- status: success
- profile: codex2
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/fmail-next-task.md
- prompt_override: false
- started_at: 2026-02-09T12:06:16Z
- finished_at: 2026-02-09T12:20:52Z
- exit_code: 0

```
+		},
+		byDM: map[string][]fmail.Message{
+			"bob": {
+				{ID: "20260209-100000-0001", From: "bob", To: "@viewer", Time: now.Add(-2 * time.Minute), Body: "old"},
+				{ID: "20260209-101000-0001", From: "bob", To: "@viewer", Time: now.Add(-time.Minute), Body: "new"},
+			},
+			"cara": {
+				{ID: "20260209-100000-0001", From: "cara", To: "@viewer", Time: now.Add(-2 * time.Minute), Body: "old"},
+				{ID: "20260209-101000-0001", From: "cara", To: "@viewer", Time: now.Add(-time.Minute), Body: "new"},
+			},
+		},
+	}
+
+	v := newTopicsView(t.TempDir(), provider, nil)
+	v.self = "viewer"
+	v.topics = []data.TopicInfo{
+		{Name: "task", MessageCount: 2, LastActivity: now},
+		{Name: "build", MessageCount: 2, LastActivity: now},
+	}
+	v.dms = []data.DMConversation{
+		{Agent: "bob", MessageCount: 2, LastActivity: now},
+		{Agent: "cara", MessageCount: 2, LastActivity: now},
+	}
+	v.readMarkers = map[string]string{
+		"task": "20260209-100000-0001",
+		"@bob": "20260209-100000-0001",
+	}
+
+	cmd := v.recomputeUnreadTargetsCmd([]string{"task", "@bob"})
+	require.NotNil(t, cmd)
+	msg, ok := cmd().(topicsUnreadSnapshotMsg)
+	require.True(t, ok)
+	require.NoError(t, msg.err)
+	require.Equal(t, 1, msg.unreadByTop["task"])
+	require.Equal(t, 1, msg.unreadByDM["bob"])
+
+	require.Equal(t, 1, provider.topicCalls["task"])
+	require.Equal(t, 0, provider.topicCalls["build"])
+	require.Equal(t, 1, provider.dmCalls["bob"])
+	require.Equal(t, 0, provider.dmCalls["cara"])
+}
+
 func TestTopicsViewPreviewLoadsLazilyAndCaches(t *testing.T) {
 	now := time.Date(2026, 2, 9, 12, 0, 0, 0, time.UTC)
 	provider := &stubTopicsProvider{

tokens used
146,489
Task `forge-2tr` done + closed.

Changes
- `internal/fmailtui/topics_view.go`: unread recompute now per-changed read-marker targets (no full topic+dm scan on marker updates); DM unread sync uses delta on count increase (avoids `DMs()` rescan loops).
- `internal/fmailtui/topics_view_test.go`: add targeted recompute + marker-diff tests.
- `internal/fmailtui/data/file_provider.go`: drop unused `context` import (build fix).

Validation
- `env -u GOROOT -u GOTOOLDIR go test ./...` ok

Next loop
- Claimed `forge-s06` (incremental search index + TTL tuning); starting implementation.
```

## 2026-02-09T12:36:26Z

- run_id: f7e741d0-8e41-4efa-aa8d-93d9e52647c7
- loop_name: forge-fmail-v2-next-2
- status: success
- profile: codex2
- harness: codex
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/fmail-next-task.md
- prompt_override: false
- started_at: 2026-02-09T12:22:52Z
- finished_at: 2026-02-09T12:36:26Z
- exit_code: 0

```
+
 	target := v.selectedTarget()
 	title := "Preview"
 	if target != "" {
 		title = "Preview: " + target
 	}
-	titleLine := lipgloss.NewStyle().
-		Bold(true).
-		Foreground(lipgloss.Color(palette.Chrome.Breadcrumb)).
-		Render(truncateVis(title, innerW))
-	meta := "ctrl+u/d scroll preview  Enter open  n new  Esc back"
+	titleLine := titleStyle.Render(truncateVis(title, innerW))
+	meta := "ctrl+u/d scroll  Enter open  n compose  Esc back"
 	if v.composeActive {
-		meta = "To " + target + ": " + v.composeBody + "_" + "  (Enter send, Esc cancel)"
+		draft := truncateVis(firstLine(v.composeBody), maxInt(0, innerW-24))
+		meta = fmt.Sprintf("Compose to %s: %s_", target, draft)
+		meta = meta + "  (Enter send, Esc cancel)"
 		if v.composeSending {
 			meta = "Sending..."
 		} else if v.composeErr != nil {
 			meta = "Send failed: " + v.composeErr.Error()
 		}
 	}
-	metaLine := lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Base.Muted)).Render(truncateVis(meta, innerW))
+	metaLine := muted.Render(truncateVis(meta, innerW))
+	if v.composeErr != nil {
+		metaLine = lipgloss.NewStyle().Foreground(lipgloss.Color(palette.Priority.High)).Bold(true).Render(truncateVis(meta, innerW))
+	}
 
 	used := lipgloss.Height(titleLine) + lipgloss.Height(metaLine) + 1
 	bodyH := maxInt(1, innerH-used)
diff --git a/internal/fmailtui/topics_view_test.go b/internal/fmailtui/topics_view_test.go
index 5e89907aa8afd32c80bcd420ce37504e1f657532..2aee68424fa7058096ea12bcad0073a1a086428f
--- a/internal/fmailtui/topics_view_test.go
+++ b/internal/fmailtui/topics_view_test.go
@@ -270,7 +270,7 @@
 	v.rebuildItems()
 
 	rendered := v.renderListPanel(96, 14, themePalette(ThemeDefault))
-	require.True(t, strings.Contains(rendered, "Enter:open thread"))
+	require.True(t, strings.Contains(rendered, "j/k move"))
 	require.True(t, strings.Contains(rendered, "No matches for"))
 }
 

tokens used
122,699
forge loop: 2 tasks claimed, shipped, closed.

- `forge-9gj` closed
  - Perf harness + budgets: `internal/fmailtui/perf/`, `docs/forge-mail/fmail-tui-perf.md`, `Makefile` targets `perf-smoke`/`perf-bench` (note: if shell overrides `make`, use `/usr/bin/make`).
  - Validated: `env -u GOROOT -u GOTOOLDIR go test ./...`, `/usr/bin/make perf-smoke`, `/usr/bin/make perf-bench`.

- `forge-7gg` closed
  - UX/readability polish (core flow): `internal/fmailtui/topics_view.go`, `internal/fmailtui/search_view_render.go`, updated test `internal/fmailtui/topics_view_test.go`.
  - Before/after + manual checklist: `docs/forge-mail/fmail-tui-ux-polish-2026-02-09.md`.
  - Validated: `env -u GOROOT -u GOTOOLDIR go test ./...`.

Project `prj-afyxck62`: no `sv task ready` remaining. In-progress left untouched: `forge-w6k`, `forge-jv3`, `forge-s06`, `forge-9h5`.
```

