package fmailtui

import (
	"sort"
	"strings"

	"github.com/tOgg1/forge/internal/fmail"
)

func (v *searchView) insertRunes(s string) {
	if s == "" {
		return
	}
	q := []rune(v.query)
	pos := clampInt(v.cursor, 0, len(q))
	ins := []rune(s)
	next := make([]rune, 0, len(q)+len(ins))
	next = append(next, q[:pos]...)
	next = append(next, ins...)
	next = append(next, q[pos:]...)
	v.query = string(next)
	v.cursor = pos + len(ins)
	v.resetCompletion()
}

func (v *searchView) deleteLeft() bool {
	q := []rune(v.query)
	pos := clampInt(v.cursor, 0, len(q))
	if pos <= 0 || len(q) == 0 {
		return false
	}
	next := append(q[:pos-1], q[pos:]...)
	v.query = string(next)
	v.cursor = pos - 1
	v.resetCompletion()
	return true
}

func (v *searchView) resetCompletion() {
	v.completeKey = ""
	v.completePrefix = ""
	v.completeOptions = nil
	v.completeIndex = 0
}

func (v *searchView) tryComplete(delta int) bool {
	raw := v.query
	cursor := clampInt(v.cursor, 0, len([]rune(raw)))
	start, end := tokenBoundsAt(raw, cursor)
	if start < 0 || end < start {
		return false
	}
	token := strings.TrimSpace(string([]rune(raw)[start:end]))
	if token == "" || !strings.Contains(token, ":") {
		return false
	}
	key, val, _ := strings.Cut(token, ":")
	key = strings.ToLower(strings.TrimSpace(key))
	val = strings.TrimSpace(val)

	cands := v.completionCandidates(key)
	if len(cands) == 0 {
		return false
	}
	matches := make([]string, 0, len(cands))
	for _, c := range cands {
		if val == "" || strings.HasPrefix(strings.ToLower(c), strings.ToLower(val)) {
			matches = append(matches, c)
		}
	}
	if len(matches) == 0 {
		return false
	}
	sort.Strings(matches)

	// Preserve cycling if the token prefix didn't change since last Tab.
	if v.lastCompletionRev != v.rev || v.completeKey != key || v.completePrefix != val {
		v.completeKey = key
		v.completePrefix = val
		v.completeOptions = matches
		v.completeIndex = 0
		v.lastCompletionRev = v.rev
	} else {
		v.completeIndex = (v.completeIndex + delta + len(v.completeOptions)) % len(v.completeOptions)
	}
	chosen := v.completeOptions[v.completeIndex]

	repl := key + ":" + chosen
	whole := []rune(raw)
	next := string(append(append([]rune(nil), whole[:start]...), append([]rune(repl), whole[end:]...)...))
	v.query = next
	v.cursor = start + len([]rune(repl))
	return true
}

func (v *searchView) completionCandidates(key string) []string {
	switch key {
	case "from":
		return v.agents
	case "to":
		out := make([]string, 0, len(v.topics)+len(v.agents))
		out = append(out, v.topics...)
		for _, a := range v.agents {
			out = append(out, "@"+a)
		}
		return out
	case "in":
		return v.topics
	case "priority":
		return []string{fmail.PriorityHigh, fmail.PriorityNormal, fmail.PriorityLow}
	case "has":
		return []string{"reply", "bookmark", "annotation"}
	case "is":
		return []string{"unread"}
	case "since", "until":
		return []string{"15m", "1h", "6h", "1d", "7d"}
	default:
		return nil
	}
}

func tokenBoundsAt(raw string, cursor int) (int, int) {
	r := []rune(raw)
	if cursor < 0 {
		cursor = 0
	}
	if cursor > len(r) {
		cursor = len(r)
	}
	start := cursor
	for start > 0 && !isSpaceRune(r[start-1]) {
		start--
	}
	end := cursor
	for end < len(r) && !isSpaceRune(r[end]) {
		end++
	}
	return start, end
}

func isSpaceRune(r rune) bool {
	return r == ' ' || r == '\t' || r == '\n' || r == '\r'
}
