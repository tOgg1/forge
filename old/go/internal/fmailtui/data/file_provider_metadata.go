package data

import (
	"errors"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
)

type messageMetadataEntry struct {
	id       string
	from     string
	to       string
	activity time.Time
	modTime  time.Time
	message  fmail.Message
}

type topicMetadataEntry struct {
	dirModTime time.Time
	checkedAt  time.Time
	files      map[string]messageMetadataEntry
	summary    TopicInfo
}

type dmDirMetadataEntry struct {
	dirModTime time.Time
	checkedAt  time.Time
	files      map[string]messageMetadataEntry
}

func (p *FileProvider) buildTopicsFromMetadata() ([]TopicInfo, error) {
	topicNames, err := p.listTopicNames()
	if err != nil {
		return nil, err
	}

	topics := make([]TopicInfo, 0, len(topicNames))
	for _, topic := range topicNames {
		info, err := p.topicInfoFromMetadata(topic)
		if err != nil {
			return nil, err
		}
		topics = append(topics, info)
	}
	sort.Slice(topics, func(i, j int) bool { return topics[i].Name < topics[j].Name })
	return topics, nil
}

func (p *FileProvider) topicInfoFromMetadata(topic string) (TopicInfo, error) {
	dir := p.store.TopicDir(topic)
	dirModTime, dirExists := dirModTimeUTC(dir)
	now := time.Now().UTC()
	ttl := p.metadataTTL
	if ttl <= 0 {
		ttl = defaultMetadataTTL
	}

	p.mu.RLock()
	cached, ok := p.topicMetadataCache[topic]
	p.mu.RUnlock()

	if ok && topicMetadataFresh(cached, now, dirModTime, dirExists, ttl) {
		return cloneTopicInfo(cached.summary), nil
	}

	refreshed, err := p.refreshTopicMetadata(topic, dir, dirModTime, dirExists, cached)
	if err != nil {
		return TopicInfo{}, err
	}

	p.mu.Lock()
	p.topicMetadataCache[topic] = refreshed
	p.mu.Unlock()
	return cloneTopicInfo(refreshed.summary), nil
}

func (p *FileProvider) refreshTopicMetadata(topic string, dir string, dirModTime time.Time, dirExists bool, previous topicMetadataEntry) (topicMetadataEntry, error) {
	refreshed := topicMetadataEntry{
		dirModTime: dirModTime,
		checkedAt:  time.Now().UTC(),
		files:      make(map[string]messageMetadataEntry),
		summary: TopicInfo{
			Name: topic,
		},
	}
	if !dirExists {
		return refreshed, nil
	}

	entries, err := os.ReadDir(dir)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return refreshed, nil
		}
		return topicMetadataEntry{}, err
	}

	for _, entry := range entries {
		if entry.IsDir() || filepath.Ext(entry.Name()) != ".json" {
			continue
		}
		name := entry.Name()
		info, err := entry.Info()
		if err != nil {
			if errors.Is(err, os.ErrNotExist) {
				continue
			}
			return topicMetadataEntry{}, err
		}
		// FileProvider messages are append-only (O_EXCL writes). Truncate modTime to
		// avoid false cache misses on filesystems with coarser/unstable precision.
		modTime := info.ModTime().UTC().Truncate(time.Second)
		if prev, ok := previous.files[name]; ok && prev.modTime.Equal(modTime) {
			refreshed.files[name] = prev
			continue
		}

		path := filepath.Join(dir, name)
		message, ok, err := p.readMessageFile(path, entry)
		if err != nil {
			return topicMetadataEntry{}, err
		}
		if !ok {
			continue
		}
		refreshed.files[name] = messageMetadataEntry{
			id:       strings.TrimSpace(message.ID),
			from:     strings.TrimSpace(message.From),
			to:       strings.TrimSpace(message.To),
			activity: latestActivity(message),
			modTime:  modTime,
			message:  cloneMessage(message),
		}
	}

	refreshed.summary = buildTopicSummary(topic, refreshed.files)
	return refreshed, nil
}

func buildTopicSummary(topic string, files map[string]messageMetadataEntry) TopicInfo {
	info := TopicInfo{Name: topic}
	if len(files) == 0 {
		return info
	}

	participants := make(map[string]struct{}, len(files))
	var (
		lastID      string
		lastName    string
		lastMessage fmail.Message
		lastOK      bool
	)
	for name, meta := range files {
		info.MessageCount++
		if meta.from != "" {
			participants[meta.from] = struct{}{}
		}
		if meta.id > lastID || (meta.id == lastID && name > lastName) {
			lastID = meta.id
			lastName = name
			lastMessage = cloneMessage(meta.message)
			lastOK = true
		}
	}

	if len(participants) > 0 {
		info.Participants = make([]string, 0, len(participants))
		for participant := range participants {
			info.Participants = append(info.Participants, participant)
		}
		sort.Strings(info.Participants)
	}
	if lastOK {
		last := cloneMessage(lastMessage)
		info.LastMessage = &last
		info.LastActivity = latestActivity(last)
	}
	return info
}

func (p *FileProvider) buildDMConversationsFromMetadata(viewer string) ([]DMConversation, error) {
	if conversations, ok := p.cachedDMConversations(viewer); ok {
		return conversations, nil
	}

	dmDirs, err := p.listDMDirectoryNames()
	if err != nil {
		return nil, err
	}
	byAgent := make(map[string]*DMConversation)
	for _, dirAgent := range dmDirs {
		dmMeta, err := p.dmDirectoryMetadata(dirAgent)
		if err != nil {
			return nil, err
		}
		for _, meta := range dmMeta.files {
			msg := fmail.Message{
				ID:   meta.id,
				From: meta.from,
				To:   meta.to,
				Time: meta.activity,
			}
			peer := dmPeer(viewer, dirAgent, msg)
			if peer == "" {
				continue
			}
			conv, ok := byAgent[peer]
			if !ok {
				conv = &DMConversation{Agent: peer}
				byAgent[peer] = conv
			}
			conv.MessageCount++
			if meta.activity.After(conv.LastActivity) {
				conv.LastActivity = meta.activity
			}
		}
	}

	conversations := make([]DMConversation, 0, len(byAgent))
	for _, conversation := range byAgent {
		conversations = append(conversations, *conversation)
	}
	sort.Slice(conversations, func(i, j int) bool {
		if !conversations[i].LastActivity.Equal(conversations[j].LastActivity) {
			return conversations[i].LastActivity.After(conversations[j].LastActivity)
		}
		return conversations[i].Agent < conversations[j].Agent
	})
	p.storeDMConversations(viewer, conversations)
	return cloneDMConversations(conversations), nil
}

func (p *FileProvider) dmDirectoryMetadata(agent string) (dmDirMetadataEntry, error) {
	dir := p.store.DMDir(agent)
	dirModTime, dirExists := dirModTimeUTC(dir)
	now := time.Now().UTC()
	ttl := p.metadataTTL
	if ttl <= 0 {
		ttl = defaultMetadataTTL
	}

	p.mu.RLock()
	cached, ok := p.dmDirMetadataCache[agent]
	p.mu.RUnlock()
	if ok && dmDirMetadataFresh(cached, now, dirModTime, dirExists, ttl) {
		return cached, nil
	}

	refreshed, err := p.refreshDMDirectoryMetadata(dir, dirModTime, dirExists, cached)
	if err != nil {
		return dmDirMetadataEntry{}, err
	}
	p.mu.Lock()
	p.dmDirMetadataCache[agent] = refreshed
	p.mu.Unlock()
	return refreshed, nil
}

func (p *FileProvider) refreshDMDirectoryMetadata(dir string, dirModTime time.Time, dirExists bool, previous dmDirMetadataEntry) (dmDirMetadataEntry, error) {
	refreshed := dmDirMetadataEntry{
		dirModTime: dirModTime,
		checkedAt:  time.Now().UTC(),
		files:      make(map[string]messageMetadataEntry),
	}
	if !dirExists {
		return refreshed, nil
	}

	entries, err := os.ReadDir(dir)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return refreshed, nil
		}
		return dmDirMetadataEntry{}, err
	}

	for _, entry := range entries {
		if entry.IsDir() || filepath.Ext(entry.Name()) != ".json" {
			continue
		}
		name := entry.Name()
		info, err := entry.Info()
		if err != nil {
			if errors.Is(err, os.ErrNotExist) {
				continue
			}
			return dmDirMetadataEntry{}, err
		}
		modTime := info.ModTime().UTC().Truncate(time.Second)
		if prev, ok := previous.files[name]; ok && prev.modTime.Equal(modTime) {
			refreshed.files[name] = prev
			continue
		}

		path := filepath.Join(dir, name)
		message, ok, err := p.readMessageFile(path, entry)
		if err != nil {
			return dmDirMetadataEntry{}, err
		}
		if !ok {
			continue
		}
		refreshed.files[name] = messageMetadataEntry{
			id:       strings.TrimSpace(message.ID),
			from:     strings.TrimSpace(message.From),
			to:       strings.TrimSpace(message.To),
			activity: latestActivity(message),
			modTime:  modTime,
		}
	}
	return refreshed, nil
}

func topicMetadataFresh(entry topicMetadataEntry, now time.Time, dirModTime time.Time, dirExists bool, ttl time.Duration) bool {
	if ttl <= 0 || now.Sub(entry.checkedAt) > ttl {
		return false
	}
	if !dirExists {
		return len(entry.files) == 0
	}
	return entry.dirModTime.Equal(dirModTime)
}

func dmDirMetadataFresh(entry dmDirMetadataEntry, now time.Time, dirModTime time.Time, dirExists bool, ttl time.Duration) bool {
	if ttl <= 0 || now.Sub(entry.checkedAt) > ttl {
		return false
	}
	if !dirExists {
		return len(entry.files) == 0
	}
	return entry.dirModTime.Equal(dirModTime)
}

func (p *FileProvider) cachedDMConversations(viewer string) ([]DMConversation, bool) {
	p.mu.RLock()
	defer p.mu.RUnlock()
	entry, ok := p.dmConversationsCache[viewer]
	if !ok || !entry.ok || time.Now().After(entry.expires) {
		return nil, false
	}
	return cloneDMConversations(entry.value), true
}

func (p *FileProvider) storeDMConversations(viewer string, conversations []DMConversation) {
	ttl := p.metadataTTL
	if ttl <= 0 {
		ttl = defaultMetadataTTL
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	p.dmConversationsCache[viewer] = timedEntry[[]DMConversation]{
		value:   cloneDMConversations(conversations),
		expires: time.Now().Add(ttl),
		ok:      true,
	}
}

func (p *FileProvider) invalidateCachesForMessage(message fmail.Message) {
	target := strings.TrimSpace(message.To)
	if target == "" {
		return
	}
	searchTarget := ""
	p.mu.Lock()

	if strings.HasPrefix(target, "@") {
		agent := strings.TrimPrefix(target, "@")
		if normalized, err := fmail.NormalizeAgentName(agent); err == nil {
			agent = normalized
		}
		delete(p.dmMsgCache, agent)
		p.markDMMetadataDirtyLocked(agent)
		p.dmConversationsCache = make(map[string]timedEntry[[]DMConversation])
		if agent != "" {
			searchTarget = "@" + agent
		}
		p.mu.Unlock()
		p.markSearchTargetDirty(searchTarget)
		return
	}

	topic := target
	if normalized, err := fmail.NormalizeTopic(topic); err == nil {
		topic = normalized
	}
	p.topicsCache = timedEntry[[]TopicInfo]{}
	delete(p.topicMsgCache, topic)
	p.markTopicMetadataDirtyLocked(topic)
	searchTarget = topic
	p.mu.Unlock()
	p.markSearchTargetDirty(searchTarget)
}

func (p *FileProvider) invalidateMetadataForPath(path string) {
	kind, key := p.metadataTargetFromPath(path)
	if kind == "" || key == "" {
		return
	}
	searchTarget := ""

	p.mu.Lock()

	switch kind {
	case "topic":
		p.topicsCache = timedEntry[[]TopicInfo]{}
		delete(p.topicMsgCache, key)
		p.markTopicMetadataDirtyLocked(key)
		searchTarget = key
	case "dm":
		delete(p.dmMsgCache, key)
		p.markDMMetadataDirtyLocked(key)
		p.dmConversationsCache = make(map[string]timedEntry[[]DMConversation])
		searchTarget = "@" + key
	}
	p.mu.Unlock()
	p.markSearchTargetDirty(searchTarget)
}

func (p *FileProvider) markTopicMetadataDirtyLocked(topic string) {
	entry, ok := p.topicMetadataCache[topic]
	if !ok {
		return
	}
	entry.checkedAt = time.Time{}
	p.topicMetadataCache[topic] = entry
}

func (p *FileProvider) markDMMetadataDirtyLocked(agent string) {
	entry, ok := p.dmDirMetadataCache[agent]
	if !ok {
		return
	}
	entry.checkedAt = time.Time{}
	p.dmDirMetadataCache[agent] = entry
}

func (p *FileProvider) metadataTargetFromPath(path string) (string, string) {
	cleanPath := filepath.Clean(strings.TrimSpace(path))
	if cleanPath == "" || cleanPath == "." {
		return "", ""
	}
	cleanRoot := filepath.Clean(p.store.Root)
	rel, err := filepath.Rel(cleanRoot, cleanPath)
	if err != nil || rel == "." || strings.HasPrefix(rel, "..") {
		return "", ""
	}
	parts := strings.Split(rel, string(os.PathSeparator))
	if len(parts) < 2 {
		return "", ""
	}

	switch parts[0] {
	case "topics":
		topic, err := fmail.NormalizeTopic(parts[1])
		if err != nil {
			return "", ""
		}
		return "topic", topic
	case "dm":
		agent, err := fmail.NormalizeAgentName(parts[1])
		if err != nil {
			return "", ""
		}
		return "dm", agent
	default:
		return "", ""
	}
}

func (p *FileProvider) markSearchTargetDirty(target string) {
	target = strings.TrimSpace(target)
	if target == "" {
		return
	}
	p.searchMu.Lock()
	defer p.searchMu.Unlock()
	if p.searchDirty == nil {
		p.searchDirty = make(map[string]struct{})
	}
	p.searchDirty[target] = struct{}{}
}

func cloneTopicInfo(info TopicInfo) TopicInfo {
	cloned := info
	if len(info.Participants) > 0 {
		cloned.Participants = append([]string(nil), info.Participants...)
	}
	if info.LastMessage != nil {
		last := cloneMessage(*info.LastMessage)
		cloned.LastMessage = &last
	}
	return cloned
}

func cloneDMConversations(conversations []DMConversation) []DMConversation {
	if len(conversations) == 0 {
		return nil
	}
	cloned := make([]DMConversation, len(conversations))
	copy(cloned, conversations)
	return cloned
}

func (p *FileProvider) messageReadStats() (int64, int64) {
	return p.messageReadLookups.Load(), p.messageDiskReads.Load()
}
