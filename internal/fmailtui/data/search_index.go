package data

import (
	"sort"
	"strings"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
)

type searchRef struct {
	target string
	idx    int
}

type indexedTarget struct {
	messages   []fmail.Message
	tokens     []string
	dirModTime time.Time
}

type textSearchIndex struct {
	builtAt   time.Time
	checkedAt time.Time
	targets   map[string]indexedTarget
	postings  map[string][]searchRef // token -> refs
}

func (p *FileProvider) ensureTextIndex(now time.Time) (*textSearchIndex, error) {
	p.searchMu.Lock()
	defer p.searchMu.Unlock()

	if p.searchIndex == nil {
		idx, err := p.buildTextIndexLocked(now)
		if err != nil {
			return nil, err
		}
		p.searchIndex = idx
		return p.searchIndex, nil
	}

	dirty := p.consumeDirtySearchTargetsLocked()
	if p.searchIndexTTL <= 0 {
		p.searchIndexTTL = defaultSearchIndexTTL
	}
	if now.Sub(p.searchIndex.checkedAt) >= p.searchIndexTTL {
		drift, err := p.detectIndexDriftLocked(p.searchIndex)
		if err != nil {
			return nil, err
		}
		for target := range drift {
			dirty[target] = struct{}{}
		}
		p.searchIndex.checkedAt = now
	}
	if len(dirty) == 0 {
		return p.searchIndex, nil
	}

	touchedTokens := make(map[string]struct{})
	for target := range dirty {
		if err := p.refreshIndexTargetLocked(p.searchIndex, target, touchedTokens); err != nil {
			return nil, err
		}
	}
	normalizePostingsForTokens(p.searchIndex, touchedTokens)
	p.searchIndex.builtAt = now
	p.searchIndex.checkedAt = now
	return p.searchIndex, nil
}

func (p *FileProvider) buildTextIndexLocked(now time.Time) (*textSearchIndex, error) {
	idx := &textSearchIndex{
		builtAt:   now,
		checkedAt: now,
		targets:   make(map[string]indexedTarget),
		postings:  make(map[string][]searchRef),
	}

	topics, err := p.listTopicNames()
	if err != nil {
		return nil, err
	}
	dmDirs, err := p.listDMDirectoryNames()
	if err != nil {
		return nil, err
	}

	touched := make(map[string]struct{})
	for _, topic := range topics {
		if err := p.refreshIndexTargetLocked(idx, topic, touched); err != nil {
			return nil, err
		}
	}
	for _, dirAgent := range dmDirs {
		if err := p.refreshIndexTargetLocked(idx, "@"+dirAgent, touched); err != nil {
			return nil, err
		}
	}
	normalizePostingsForTokens(idx, touched)
	return idx, nil
}

func (p *FileProvider) refreshIndexTargetLocked(idx *textSearchIndex, target string, touchedTokens map[string]struct{}) error {
	target = strings.TrimSpace(target)
	if idx == nil || target == "" {
		return nil
	}

	if old, ok := idx.targets[target]; ok {
		removeTargetFromPostings(idx, target, old.tokens, touchedTokens)
		delete(idx.targets, target)
	}

	var (
		messages []fmail.Message
		modTime  time.Time
		ok       bool
		err      error
	)
	if strings.HasPrefix(target, "@") {
		agent := strings.TrimPrefix(target, "@")
		if normalized, normalizeErr := fmail.NormalizeAgentName(agent); normalizeErr != nil {
			return nil
		} else {
			agent = normalized
		}
		messages, err = p.messagesForDMDirectory(agent)
		if err != nil {
			return err
		}
		modTime, ok = dirModTimeUTC(p.store.DMDir(agent))
	} else {
		topic := target
		if normalized, normalizeErr := fmail.NormalizeTopic(topic); normalizeErr != nil {
			return nil
		} else {
			topic = normalized
		}
		messages, err = p.messagesForTopic(topic)
		if err != nil {
			return err
		}
		modTime, ok = dirModTimeUTC(p.store.TopicDir(topic))
	}
	if !ok {
		return nil
	}

	sortMessagesByID(messages)
	targetTokens := indexTargetMessages(idx, target, messages, touchedTokens)
	idx.targets[target] = indexedTarget{
		messages:   append([]fmail.Message(nil), messages...),
		tokens:     targetTokens,
		dirModTime: modTime,
	}
	return nil
}

func removeTargetFromPostings(idx *textSearchIndex, target string, tokens []string, touchedTokens map[string]struct{}) {
	if idx == nil || len(tokens) == 0 {
		return
	}
	for _, token := range tokens {
		refs := idx.postings[token]
		if len(refs) == 0 {
			continue
		}
		out := refs[:0]
		for i := range refs {
			if refs[i].target != target {
				out = append(out, refs[i])
			}
		}
		if len(out) == 0 {
			delete(idx.postings, token)
		} else {
			idx.postings[token] = out
		}
		touchedTokens[token] = struct{}{}
	}
}

func indexTargetMessages(idx *textSearchIndex, target string, msgs []fmail.Message, touchedTokens map[string]struct{}) []string {
	if idx == nil || target == "" || len(msgs) == 0 {
		return nil
	}
	tokenSet := make(map[string]struct{})
	for i := range msgs {
		body := strings.ToLower(messageBodyString(msgs[i]))
		tokens := tokenizeForIndex(body)
		if len(tokens) == 0 {
			continue
		}
		seen := make(map[string]struct{}, len(tokens))
		for _, token := range tokens {
			if token == "" {
				continue
			}
			if _, ok := seen[token]; ok {
				continue
			}
			seen[token] = struct{}{}
			tokenSet[token] = struct{}{}
			idx.postings[token] = append(idx.postings[token], searchRef{target: target, idx: i})
			touchedTokens[token] = struct{}{}
		}
	}

	out := make([]string, 0, len(tokenSet))
	for token := range tokenSet {
		out = append(out, token)
	}
	sort.Strings(out)
	return out
}

func normalizePostingsForTokens(idx *textSearchIndex, touchedTokens map[string]struct{}) {
	if idx == nil || len(touchedTokens) == 0 {
		return
	}
	for token := range touchedTokens {
		refs := idx.postings[token]
		if len(refs) == 0 {
			delete(idx.postings, token)
			continue
		}
		sort.SliceStable(refs, func(i, j int) bool {
			if refs[i].target != refs[j].target {
				return refs[i].target < refs[j].target
			}
			return refs[i].idx < refs[j].idx
		})
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
}

func (p *FileProvider) consumeDirtySearchTargetsLocked() map[string]struct{} {
	dirty := make(map[string]struct{}, len(p.searchDirty))
	for target := range p.searchDirty {
		dirty[target] = struct{}{}
	}
	p.searchDirty = make(map[string]struct{})
	return dirty
}

func (p *FileProvider) detectIndexDriftLocked(idx *textSearchIndex) (map[string]struct{}, error) {
	dirty := make(map[string]struct{})
	currentTargets := make(map[string]time.Time)

	topics, err := p.listTopicNames()
	if err != nil {
		return nil, err
	}
	for _, topic := range topics {
		if mod, ok := dirModTimeUTC(p.store.TopicDir(topic)); ok {
			currentTargets[topic] = mod
		}
	}

	dmDirs, err := p.listDMDirectoryNames()
	if err != nil {
		return nil, err
	}
	for _, agent := range dmDirs {
		target := "@" + agent
		if mod, ok := dirModTimeUTC(p.store.DMDir(agent)); ok {
			currentTargets[target] = mod
		}
	}

	for target, mod := range currentTargets {
		existing, ok := idx.targets[target]
		if !ok || !existing.dirModTime.Equal(mod) {
			dirty[target] = struct{}{}
		}
	}
	for target := range idx.targets {
		if _, ok := currentTargets[target]; !ok {
			dirty[target] = struct{}{}
		}
	}
	return dirty, nil
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
