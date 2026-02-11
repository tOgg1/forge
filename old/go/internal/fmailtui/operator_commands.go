package fmailtui

import (
	"fmt"
	"sort"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui/data"
)

func (v *operatorView) handleSlashCommand(input string) (tea.Cmd, bool) {
	input = strings.TrimSpace(input)
	if !strings.HasPrefix(input, "/") {
		return nil, false
	}
	fields := strings.Fields(input)
	if len(fields) == 0 {
		v.statusErr = fmt.Errorf("empty command")
		return nil, true
	}
	name := strings.TrimPrefix(strings.ToLower(fields[0]), "/")
	args := fields[1:]

	switch name {
	case "dm":
		if len(args) < 1 {
			v.statusErr = fmt.Errorf("usage: /dm <agent> [msg]")
			return nil, true
		}
		target := normalizeAgentTarget(args[0])
		if len(args) == 1 {
			v.target = target
			v.statusErr = nil
			v.statusLine = "target -> " + target
			return nil, true
		}
		body := strings.TrimSpace(strings.Join(args[1:], " "))
		if body == "" {
			v.statusErr = fmt.Errorf("usage: /dm <agent> <msg>")
			return nil, true
		}
		return v.sendRequests([]data.SendRequest{v.newRequest(target, body, "", v.composePriority, v.composeTags)}), true
	case "topic":
		if len(args) < 1 {
			v.statusErr = fmt.Errorf("usage: /topic <name> [msg]")
			return nil, true
		}
		topic := strings.TrimSpace(strings.TrimPrefix(args[0], "#"))
		if topic == "" {
			v.statusErr = fmt.Errorf("usage: /topic <name> [msg]")
			return nil, true
		}
		if len(args) == 1 {
			v.target = topic
			v.statusErr = nil
			v.statusLine = "target -> " + topic
			return nil, true
		}
		body := strings.TrimSpace(strings.Join(args[1:], " "))
		if body == "" {
			v.statusErr = fmt.Errorf("usage: /topic <name> <msg>")
			return nil, true
		}
		return v.sendRequests([]data.SendRequest{v.newRequest(topic, body, "", v.composePriority, v.composeTags)}), true
	case "broadcast":
		body := strings.TrimSpace(strings.TrimPrefix(input, "/broadcast"))
		if body == "" {
			v.statusErr = fmt.Errorf("usage: /broadcast <msg>")
			return nil, true
		}
		targets := v.activeAgentTargets()
		if len(targets) == 0 {
			v.statusErr = fmt.Errorf("no active agents")
			return nil, true
		}
		reqs := make([]data.SendRequest, 0, len(targets))
		for _, target := range targets {
			reqs = append(reqs, v.newRequest(target, body, "", v.composePriority, v.composeTags))
		}
		v.statusErr = nil
		v.statusLine = fmt.Sprintf("broadcast -> %d agents", len(reqs))
		return v.sendRequests(reqs), true
	case "status":
		v.statusErr = nil
		v.statusLine = v.statusSummaryLine()
		return nil, true
	case "assign":
		if len(args) < 2 {
			v.statusErr = fmt.Errorf("usage: /assign <agent> <task>")
			return nil, true
		}
		target := normalizeAgentTarget(args[0])
		body := strings.TrimSpace(strings.Join(args[1:], " "))
		if body == "" {
			v.statusErr = fmt.Errorf("usage: /assign <agent> <task>")
			return nil, true
		}
		tags := append([]string{"assignment"}, v.composeTags...)
		return v.sendRequests([]data.SendRequest{v.newRequest(target, body, "", fmail.PriorityHigh, tags)}), true
	case "ask":
		if len(args) < 2 {
			v.statusErr = fmt.Errorf("usage: /ask <agent> <question>")
			return nil, true
		}
		target := normalizeAgentTarget(args[0])
		body := strings.TrimSpace(strings.Join(args[1:], " "))
		if body == "" {
			v.statusErr = fmt.Errorf("usage: /ask <agent> <question>")
			return nil, true
		}
		tags := append([]string{"question"}, v.composeTags...)
		return v.sendRequests([]data.SendRequest{v.newRequest(target, body, "", v.composePriority, tags)}), true
	case "approve":
		if len(args) < 1 {
			v.statusErr = fmt.Errorf("usage: /approve <msg-id>")
			return nil, true
		}
		target := strings.TrimSpace(v.target)
		if target == "" {
			v.statusErr = fmt.Errorf("missing target")
			return nil, true
		}
		replyTo := strings.TrimSpace(args[0])
		if replyTo == "" {
			v.statusErr = fmt.Errorf("usage: /approve <msg-id>")
			return nil, true
		}
		tags := append([]string{"approved"}, v.composeTags...)
		v.pendingApprove = ""
		return v.sendRequests([]data.SendRequest{v.newRequest(target, "Approved.", replyTo, v.composePriority, tags)}), true
	case "reject":
		if len(args) < 2 {
			v.statusErr = fmt.Errorf("usage: /reject <msg-id> <reason>")
			return nil, true
		}
		target := strings.TrimSpace(v.target)
		if target == "" {
			v.statusErr = fmt.Errorf("missing target")
			return nil, true
		}
		replyTo := strings.TrimSpace(args[0])
		reason := strings.TrimSpace(strings.Join(args[1:], " "))
		if replyTo == "" || reason == "" {
			v.statusErr = fmt.Errorf("usage: /reject <msg-id> <reason>")
			return nil, true
		}
		tags := append([]string{"rejected"}, v.composeTags...)
		v.pendingApprove = ""
		return v.sendRequests([]data.SendRequest{v.newRequest(target, reason, replyTo, v.composePriority, tags)}), true
	case "priority":
		if len(args) != 1 {
			v.statusErr = fmt.Errorf("usage: /priority high|normal|low")
			return nil, true
		}
		value := normalizePriorityInput(args[0])
		if value != strings.ToLower(strings.TrimSpace(args[0])) {
			v.statusErr = fmt.Errorf("usage: /priority high|normal|low")
			return nil, true
		}
		v.composePriority = value
		v.statusErr = nil
		v.statusLine = "priority -> " + value
		return nil, true
	case "tag":
		if len(args) == 0 {
			v.composeTags = nil
			v.statusErr = nil
			v.statusLine = "tags cleared"
			return nil, true
		}
		v.composeTags = parseTagCSV(strings.Join(args, ","))
		v.statusErr = nil
		v.statusLine = "tags -> " + strings.Join(v.composeTags, ",")
		return nil, true
	case "group":
		if len(args) < 1 {
			v.statusErr = fmt.Errorf("usage: /group create <name> <agents...> | /group <name> <msg>")
			return nil, true
		}
		if strings.EqualFold(args[0], "create") {
			if len(args) < 3 {
				v.statusErr = fmt.Errorf("usage: /group create <name> <agents...>")
				return nil, true
			}
			name := strings.TrimSpace(args[1])
			if name == "" {
				v.statusErr = fmt.Errorf("usage: /group create <name> <agents...>")
				return nil, true
			}
			members := make([]string, 0, len(args)-2)
			for _, raw := range args[2:] {
				members = append(members, normalizeAgentTarget(raw))
			}
			v.groups[name] = dedupeAndSort(members)
			if v.tuiState != nil {
				v.tuiState.SetGroup(name, v.groups[name])
				v.tuiState.SaveSoon()
			}
			v.statusErr = nil
			v.statusLine = fmt.Sprintf("group %s saved (%d members)", name, len(v.groups[name]))
			return nil, true
		}
		if len(args) < 2 {
			v.statusErr = fmt.Errorf("usage: /group <name> <msg>")
			return nil, true
		}
		name := strings.TrimSpace(args[0])
		members := v.groups[name]
		if len(members) == 0 {
			if v.tuiState != nil {
				members = v.tuiState.Groups()[name]
			}
			if len(members) == 0 {
				v.statusErr = fmt.Errorf("unknown group %q", name)
				return nil, true
			}
		}
		body := strings.TrimSpace(strings.Join(args[1:], " "))
		if body == "" {
			v.statusErr = fmt.Errorf("usage: /group <name> <msg>")
			return nil, true
		}
		reqs := make([]data.SendRequest, 0, len(members))
		for _, member := range members {
			reqs = append(reqs, v.newRequest(member, body, "", v.composePriority, v.composeTags))
		}
		return v.sendRequests(reqs), true
	case "mystatus":
		status := strings.TrimSpace(strings.TrimPrefix(input, "/mystatus"))
		if status == "" {
			v.statusErr = fmt.Errorf("usage: /mystatus <text>")
			return nil, true
		}
		v.touchPresence(status)
		v.statusErr = nil
		v.statusLine = "status -> " + status
		return v.loadCmd(), true
	case "help":
		v.showPalette = true
		v.statusErr = nil
		return nil, true
	default:
		v.statusErr = fmt.Errorf("unknown command: /%s", name)
		return nil, true
	}
}

func (v *operatorView) newRequest(target, body, replyTo, priority string, tags []string) data.SendRequest {
	return data.SendRequest{
		From:     v.self,
		To:       strings.TrimSpace(target),
		Body:     strings.TrimSpace(body),
		ReplyTo:  strings.TrimSpace(replyTo),
		Priority: normalizePriorityInput(priority),
		Tags:     append([]string(nil), tags...),
	}
}

func (v *operatorView) activeAgentTargets() []string {
	records := append([]fmail.AgentRecord(nil), v.agents...)
	if len(records) == 0 && v.provider != nil {
		if loaded, err := v.provider.Agents(); err == nil {
			records = loaded
		}
	}
	now := time.Now().UTC()
	seen := map[string]struct{}{}
	out := make([]string, 0, len(records))
	for _, rec := range records {
		name := strings.TrimSpace(rec.Name)
		if name == "" || strings.EqualFold(name, v.self) {
			continue
		}
		if now.Sub(rec.LastSeen) > operatorActiveWindow {
			continue
		}
		target := "@" + name
		if _, ok := seen[target]; ok {
			continue
		}
		seen[target] = struct{}{}
		out = append(out, target)
	}
	sort.Strings(out)
	return out
}

func (v *operatorView) statusSummaryLine() string {
	records := append([]fmail.AgentRecord(nil), v.agents...)
	if len(records) == 0 {
		return "no agents"
	}
	now := time.Now().UTC()
	sort.SliceStable(records, func(i, j int) bool {
		return records[i].Name < records[j].Name
	})
	parts := make([]string, 0, len(records))
	for _, rec := range records {
		name := strings.TrimSpace(rec.Name)
		if name == "" {
			continue
		}
		state := "idle"
		if now.Sub(rec.LastSeen) <= operatorActiveWindow {
			state = "active"
		}
		if status := strings.TrimSpace(rec.Status); status != "" {
			state = status
		}
		parts = append(parts, name+":"+state)
	}
	if len(parts) == 0 {
		return "no agents"
	}
	return strings.Join(parts, "  ")
}

func normalizeAgentTarget(value string) string {
	value = strings.TrimSpace(value)
	value = strings.TrimPrefix(value, "@")
	if value == "" {
		return ""
	}
	return "@" + value
}

func dedupeAndSort(items []string) []string {
	if len(items) == 0 {
		return nil
	}
	seen := map[string]struct{}{}
	out := make([]string, 0, len(items))
	for _, item := range items {
		item = strings.TrimSpace(item)
		if item == "" {
			continue
		}
		if _, ok := seen[item]; ok {
			continue
		}
		seen[item] = struct{}{}
		out = append(out, item)
	}
	sort.Strings(out)
	return out
}
