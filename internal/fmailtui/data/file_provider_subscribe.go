package data

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
)

const defaultSubscribeSeenPaths = 50_000

type seenSet struct {
	max int
	m   map[string]struct{}
	q   []string
}

func newSeenSet(max int) *seenSet {
	if max <= 0 {
		max = defaultSubscribeSeenPaths
	}
	return &seenSet{
		max: max,
		m:   make(map[string]struct{}, max),
	}
}

func (s *seenSet) has(path string) bool {
	if s == nil {
		return false
	}
	_, ok := s.m[path]
	return ok
}

func (s *seenSet) add(path string) {
	if s == nil || path == "" {
		return
	}
	if _, ok := s.m[path]; ok {
		return
	}
	s.m[path] = struct{}{}
	s.q = append(s.q, path)
	for len(s.q) > s.max {
		evict := s.q[0]
		s.q = s.q[1:]
		delete(s.m, evict)
	}
}

type subscriptionFile struct {
	path  string
	id    string
	entry os.DirEntry
}

func (p *FileProvider) Subscribe(filter SubscriptionFilter) (<-chan fmail.Message, func()) {
	ctx, cancel := context.WithCancel(context.Background())
	out := make(chan fmail.Message, p.subscribeBuffer)
	go p.subscribeLoop(ctx, out, filter)
	return out, cancel
}

func (p *FileProvider) subscribeLoop(ctx context.Context, out chan<- fmail.Message, filter SubscriptionFilter) {
	defer close(out)

	lastSeenID := strings.TrimSpace(filter.SinceID)
	seenPaths := newSeenSet(defaultSubscribeSeenPaths)

	interval := p.pollMin
	if interval <= 0 {
		interval = defaultPollInterval
	}
	maxInterval := p.pollMax
	if maxInterval <= 0 {
		maxInterval = defaultPollMax
	}
	if maxInterval < interval {
		maxInterval = interval
	}

	timer := time.NewTimer(interval)
	defer timer.Stop()

	var (
		dirs          []string
		dirModTime    = make(map[string]time.Time)
		topicsRootMod time.Time
		dmRootMod     time.Time
	)

	topic := strings.TrimSpace(filter.Topic)
	includeDM := filter.IncludeDM || topic == "*" || (topic == "" && strings.TrimSpace(filter.Agent) == "")
	staticDirs := topic != "" && topic != "*"

	refreshDirs := func(force bool) {
		if staticDirs && len(dirs) > 0 && !force {
			return
		}
		if staticDirs {
			resolved, err := p.subscriptionDirs(filter)
			if err == nil {
				dirs = resolved
			}
			return
		}

		var shouldRefresh bool
		if mod, ok := dirModTimeUTC(filepath.Join(p.store.Root, "topics")); ok && !mod.Equal(topicsRootMod) {
			topicsRootMod = mod
			shouldRefresh = true
		}
		if includeDM {
			if mod, ok := dirModTimeUTC(filepath.Join(p.store.Root, "dm")); ok && !mod.Equal(dmRootMod) {
				dmRootMod = mod
				shouldRefresh = true
			}
		}
		if !force && !shouldRefresh && len(dirs) > 0 {
			return
		}

		resolved, err := p.subscriptionDirs(filter)
		if err != nil {
			return
		}
		dirs = resolved
	}

	refreshDirs(true)

	for {
		select {
		case <-ctx.Done():
			return
		case <-timer.C:
		}

		refreshDirs(false)

		changedDirs := make([]string, 0, len(dirs))
		for _, dir := range dirs {
			mod, ok := dirModTimeUTC(dir)
			if !ok {
				continue
			}
			if prev, ok := dirModTime[dir]; ok && mod.Equal(prev) {
				continue
			}
			dirModTime[dir] = mod
			changedDirs = append(changedDirs, dir)
		}

		files := make([]subscriptionFile, 0)
		for _, dir := range changedDirs {
			entries, err := os.ReadDir(dir)
			if err != nil {
				if errors.Is(err, os.ErrNotExist) {
					continue
				}
				continue
			}
			for _, entry := range entries {
				if entry.IsDir() || filepath.Ext(entry.Name()) != ".json" {
					continue
				}
				id := trimJSONSuffix(entry.Name())
				path := filepath.Join(dir, entry.Name())
				// Avoid per-tick full-store reprocessing by only considering:
				// - IDs after lastSeenID
				// - IDs equal to lastSeenID if not seen yet (cross-target collisions possible).
				if lastSeenID != "" {
					if id < lastSeenID {
						continue
					}
					if id == lastSeenID && seenPaths.has(path) {
						continue
					}
				}
				files = append(files, subscriptionFile{path: path, id: id, entry: entry})
			}
		}

		sort.SliceStable(files, func(i, j int) bool {
			if files[i].id != files[j].id {
				return files[i].id < files[j].id
			}
			return files[i].path < files[j].path
		})

		delivered := 0
		for _, file := range files {
			if seenPaths.has(file.path) {
				continue
			}
			message, ok, err := p.readMessageFile(file.path, file.entry)
			if err != nil {
				continue
			}
			if !ok {
				seenPaths.add(file.path)
				continue
			}
			p.invalidateMetadataForPath(file.path)
			if !filter.Since.IsZero() && message.Time.Before(filter.Since) {
				seenPaths.add(file.path)
				continue
			}
			if !messageMatchesSubscription(message, filter) {
				seenPaths.add(file.path)
				continue
			}
			// Track "cursor" by ID, but never skip ID-equal messages (collisions across dirs) unless seen by path.
			if message.ID != "" && message.ID > lastSeenID {
				lastSeenID = message.ID
			}
			seenPaths.add(file.path)

			select {
			case <-ctx.Done():
				return
			case out <- cloneMessage(message):
				delivered++
			}
		}

		// Adaptive/backoff polling:
		// - Any delivery: snap back to min interval for low latency.
		// - No changes: exponential backoff up to max interval to reduce idle CPU.
		// - Changed dirs but no matching deliveries: keep current interval (avoid oscillation on irrelevant writes).
		switch {
		case delivered > 0:
			interval = p.pollMin
		case len(changedDirs) == 0:
			interval *= 2
			if interval > maxInterval {
				interval = maxInterval
			}
		}
		if interval <= 0 {
			interval = defaultPollInterval
		}
		timer.Reset(interval)
	}
}

func dirModTimeUTC(path string) (time.Time, bool) {
	info, err := os.Stat(path)
	if err != nil {
		return time.Time{}, false
	}
	return info.ModTime().UTC(), true
}

func (p *FileProvider) subscriptionDirs(filter SubscriptionFilter) ([]string, error) {
	topic := strings.TrimSpace(filter.Topic)
	if topic != "" && topic != "*" {
		if strings.HasPrefix(topic, "@") {
			agent := strings.TrimPrefix(topic, "@")
			normalized, err := fmail.NormalizeAgentName(agent)
			if err != nil {
				return nil, err
			}
			return []string{p.store.DMDir(normalized)}, nil
		}
		normalized, err := fmail.NormalizeTopic(topic)
		if err != nil {
			return nil, err
		}
		return []string{p.store.TopicDir(normalized)}, nil
	}

	dirs := make([]string, 0)
	topics, err := p.listTopicNames()
	if err != nil {
		return nil, err
	}
	for _, topicName := range topics {
		dirs = append(dirs, p.store.TopicDir(topicName))
	}

	includeDM := filter.IncludeDM || topic == "*" || (topic == "" && strings.TrimSpace(filter.Agent) == "")
	if includeDM {
		dmDirs, err := p.listDMDirectoryNames()
		if err != nil {
			return nil, err
		}
		for _, dmDir := range dmDirs {
			dirs = append(dirs, p.store.DMDir(dmDir))
		}
	}
	return dirs, nil
}

func messageMatchesSubscription(message fmail.Message, filter SubscriptionFilter) bool {
	topic := strings.TrimSpace(filter.Topic)
	if topic != "" && topic != "*" {
		if strings.HasPrefix(topic, "@") {
			normalizedTopic := strings.ToLower(topic)
			if strings.ToLower(message.To) != normalizedTopic {
				return false
			}
		} else if !strings.EqualFold(message.To, topic) {
			return false
		}
	} else if !filter.IncludeDM && strings.HasPrefix(message.To, "@") {
		return false
	}

	if agent := strings.TrimSpace(filter.Agent); agent != "" {
		normalized, err := fmail.NormalizeAgentName(agent)
		if err == nil {
			target := strings.TrimPrefix(strings.ToLower(message.To), "@")
			if !strings.EqualFold(message.From, normalized) && target != strings.ToLower(normalized) {
				return false
			}
		}
	}

	return messageMatchesFilter(&message, MessageFilter{
		Since:    filter.Since,
		From:     filter.From,
		Priority: filter.Priority,
		Tags:     filter.Tags,
	})
}
