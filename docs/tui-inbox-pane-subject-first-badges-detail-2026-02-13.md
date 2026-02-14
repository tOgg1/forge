# Inbox Pane Rewrite: Subject-First + Badges + Detail (2026-02-13)

Task: `forge-ke7`

## Updates

- Thread list rows now render subject-first.
- Compact unread/ack badges added per thread row.
- Compact latest message id retained in row suffix.
- Thread aggregation now tracks latest message subject (not first-seen subject), preventing stale subject labels.
- Detail pane behavior preserved (thread metadata + message previews + handoff snapshot section).

## Row Format

`▸ <subject> [u:<unread>][a:<pending-ack>] (m-<id>)`

## Regression Coverage

- `inbox_thread_list_is_subject_first_with_badges`
- `inbox_detail_pane_keeps_thread_details_visible`
- Existing inbox interaction tests remain green.
