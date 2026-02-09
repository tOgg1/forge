package data

import (
	"sort"
	"strings"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
)

const searchIndexTTL = 2 * time.Second

type searchRef struct {
	target string
	idx    int
}

type indexedTarget struct {
	messages []fmail.Message
}

type textSearchIndex struct {
	builtAt  time.Time
	targets  map[string]indexedTarget
	postings map[string][]searchRef // token -> refs
}

func (p *FileProvider) ensureTextIndex(now time.Time) (*textSearchIndex, error) {
	p.searchMu.Lock()
	defer p.searchMu.Unlock()

	if p.searchIndex != nil && now.Sub(p.searchIndex.builtAt) <= searchIndexTTL {
		return p.searchIndex, nil
	}

	idx := &textSearchIndex{
		builtAt:  now,
		targets:  make(map[string]indexedTarget),
		postings: make(map[string][]searchRef),
	}

	topicNames, err := p.listTopicNames()
	if err != nil {
		return nil, err
	}
	dmDirs, err := p.listDMDirectoryNames()
	if err != nil {
		return nil, err
	}

	for _, topic := range topicNames {
		msgs, err := p.messagesForTopic(topic)
		if err != nil {
			return nil, err
		}
		msgs = append([]fmail.Message(nil), msgs...)
		sortMessagesByID(msgs)
		idx.targets[topic] = indexedTarget{messages: msgs}
		indexMessages(idx, topic, msgs)
	}

	for _, dirAgent := range dmDirs {
		target := "@" + dirAgent
		msgs, err := p.messagesForDMDirectory(dirAgent)
		if err != nil {
			return nil, err
		}
		msgs = append([]fmail.Message(nil), msgs...)
		sortMessagesByID(msgs)
		idx.targets[target] = indexedTarget{messages: msgs}
		indexMessages(idx, target, msgs)
	}

	for token, refs := range idx.postings {
		sort.SliceStable(refs, func(i, j int) bool {
			if refs[i].target != refs[j].target {
				return refs[i].target < refs[j].target
			}
			return refs[i].idx < refs[j].idx
		})
		// De-dupe.
		out := refs[:0]
		var prev searchRef
		for i := range refs {
			if i > 0 && refs[i] == prev {
				continue
			}
			prev = refs[i]
			out = append(out, refs[i])
		}
		if len(out) == 0 {
			delete(idx.postings, token)
			continue
		}
		idx.postings[token] = out
	}

	p.searchIndex = idx
	return idx, nil
}

func indexMessages(idx *textSearchIndex, target string, msgs []fmail.Message) {
	if idx == nil || target == "" || len(msgs) == 0 {
		return
	}
	for i := range msgs {
		body := strings.ToLower(messageBodyString(msgs[i]))
		tokens := tokenizeForIndex(body)
		if len(tokens) == 0 {
			continue
		}
		seen := make(map[string]struct{}, len(tokens))
		for _, tok := range tokens {
			if tok == "" {
				continue
			}
			if _, ok := seen[tok]; ok {
				continue
			}
			seen[tok] = struct{}{}
			idx.postings[tok] = append(idx.postings[tok], searchRef{target: target, idx: i})
		}
	}
}

func tokenizeForIndex(text string) []string {
	if strings.TrimSpace(text) == "" {
		return nil
	}
	parts := strings.FieldsFunc(text, func(r rune) bool {
		switch {
		case r >= 'a' && r <= 'z':
			return false
		case r >= '0' && r <= '9':
			return false
		default:
			return true
		}
	})
	out := make([]string, 0, len(parts))
	for _, p := range parts {
		p = strings.TrimSpace(p)
		if p == "" {
			continue
		}
		out = append(out, p)
	}
	return out
}
