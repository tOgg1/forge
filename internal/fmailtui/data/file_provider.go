package data

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
)

type FileProvider struct {
	root            string
	store           *fmail.Store
	cacheTTL        time.Duration
	pollInterval    time.Duration
	subscribeBuffer int
	selfAgent       string
	messageCache    *messageCache

	mu            sync.RWMutex
	topicsCache   timedEntry[[]TopicInfo]
	agentsCache   timedEntry[[]fmail.AgentRecord]
	topicMsgCache map[string]timedEntry[[]fmail.Message]
	dmMsgCache    map[string]timedEntry[[]fmail.Message]
}

func NewFileProvider(cfg FileProviderConfig) (*FileProvider, error) {
	root, err := normalizeRoot(cfg.Root)
	if err != nil {
		return nil, err
	}
	store, err := fmail.NewStore(root)
	if err != nil {
		return nil, fmt.Errorf("init store: %w", err)
	}
	if err := store.EnsureRoot(); err != nil {
		return nil, fmt.Errorf("ensure store root: %w", err)
	}

	cacheTTL := cfg.CacheTTL
	if cacheTTL <= 0 {
		cacheTTL = defaultCacheTTL
	}
	pollInterval := cfg.PollInterval
	if pollInterval <= 0 {
		pollInterval = defaultPollInterval
	}
	subscribeBuffer := cfg.SubscribeBuffer
	if subscribeBuffer <= 0 {
		subscribeBuffer = defaultSubscribeBufferSize
	}
	cacheSize := cfg.CacheCapacity
	if cacheSize <= 0 {
		cacheSize = defaultMessageCacheSize
	}

	selfAgent := strings.TrimSpace(cfg.SelfAgent)
	if selfAgent != "" {
		normalized, err := fmail.NormalizeAgentName(selfAgent)
		if err != nil {
			return nil, fmt.Errorf("normalize self agent: %w", err)
		}
		selfAgent = normalized
	}

	return &FileProvider{
		root:            root,
		store:           store,
		cacheTTL:        cacheTTL,
		pollInterval:    pollInterval,
		subscribeBuffer: subscribeBuffer,
		selfAgent:       selfAgent,
		messageCache:    newMessageCache(cacheSize),
		topicMsgCache:   make(map[string]timedEntry[[]fmail.Message]),
		dmMsgCache:      make(map[string]timedEntry[[]fmail.Message]),
	}, nil
}

func (p *FileProvider) Topics() ([]TopicInfo, error) {
	if topics, ok := p.cachedTopics(); ok {
		return topics, nil
	}

	topicNames, err := p.listTopicNames()
	if err != nil {
		return nil, err
	}

	topics := make([]TopicInfo, 0, len(topicNames))
	for _, topic := range topicNames {
		messages, err := p.messagesForTopic(topic)
		if err != nil {
			return nil, err
		}

		info := TopicInfo{
			Name:         topic,
			MessageCount: len(messages),
			Participants: participantsFromMessages(messages),
		}
		if len(messages) > 0 {
			last := cloneMessage(messages[len(messages)-1])
			info.LastMessage = &last
			info.LastActivity = latestActivity(last)
		}
		topics = append(topics, info)
	}

	sort.Slice(topics, func(i, j int) bool { return topics[i].Name < topics[j].Name })
	p.storeTopics(topics)
	return cloneTopics(topics), nil
}

func (p *FileProvider) Messages(topic string, opts MessageFilter) ([]fmail.Message, error) {
	normalized, err := fmail.NormalizeTopic(topic)
	if err != nil {
		return nil, err
	}

	messages, err := p.messagesForTopic(normalized)
	if err != nil {
		return nil, err
	}
	ranged := sliceMessagesByIDRange(messages, opts.Since, opts.Until)
	return applyMessageFilter(ranged, opts), nil
}

func (p *FileProvider) DMConversations(agent string) ([]DMConversation, error) {
	viewer, err := normalizeViewerAgent(agent, p.selfAgent)
	if err != nil {
		return nil, err
	}

	dmDirs, err := p.listDMDirectoryNames()
	if err != nil {
		return nil, err
	}
	byAgent := make(map[string]*DMConversation)

	for _, dirAgent := range dmDirs {
		messages, err := p.messagesForDMDirectory(dirAgent)
		if err != nil {
			return nil, err
		}
		for i := range messages {
			msg := messages[i]
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
			if activity := latestActivity(msg); activity.After(conv.LastActivity) {
				conv.LastActivity = activity
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
	return conversations, nil
}

func (p *FileProvider) DMs(agent string, opts MessageFilter) ([]fmail.Message, error) {
	target, err := fmail.NormalizeAgentName(agent)
	if err != nil {
		return nil, err
	}

	viewer := p.selfAgent
	if viewer == "" && strings.TrimSpace(opts.To) != "" {
		candidate := strings.TrimPrefix(strings.TrimSpace(opts.To), "@")
		normalized, err := fmail.NormalizeAgentName(candidate)
		if err == nil {
			viewer = normalized
		}
	}

	if viewer == "" {
		messages, err := p.messagesForDMDirectory(target)
		if err != nil {
			return nil, err
		}
		ranged := sliceMessagesByIDRange(messages, opts.Since, opts.Until)
		return applyMessageFilter(ranged, opts), nil
	}

	allMessages, err := p.dmConversationMessages(viewer, target)
	if err != nil {
		return nil, err
	}
	ranged := sliceMessagesByIDRange(allMessages, opts.Since, opts.Until)
	return applyMessageFilter(ranged, opts), nil
}

func (p *FileProvider) Agents() ([]fmail.AgentRecord, error) {
	if records, ok := p.cachedAgents(); ok {
		return records, nil
	}

	records, err := p.store.ListAgentRecords()
	if err != nil {
		return nil, err
	}
	p.storeAgents(records)
	return cloneAgentRecords(records), nil
}

func (p *FileProvider) Search(query SearchQuery) ([]SearchResult, error) {
	topicNames, err := p.listTopicNames()
	if err != nil {
		return nil, err
	}
	dmDirs, err := p.listDMDirectoryNames()
	if err != nil {
		return nil, err
	}

	results := make([]SearchResult, 0)
	for _, topic := range topicNames {
		messages, err := p.messagesForTopic(topic)
		if err != nil {
			return nil, err
		}
		ranged := sliceMessagesByIDRange(messages, query.Since, query.Until)
		for i := range ranged {
			msg := ranged[i]
			ok, offset, length := searchMatches(&msg, query)
			if !ok {
				continue
			}
			results = append(results, SearchResult{
				Message:     cloneMessage(msg),
				Topic:       topic,
				MatchOffset: offset,
				MatchLength: length,
			})
		}
	}

	for _, dirAgent := range dmDirs {
		messages, err := p.messagesForDMDirectory(dirAgent)
		if err != nil {
			return nil, err
		}
		ranged := sliceMessagesByIDRange(messages, query.Since, query.Until)
		for i := range ranged {
			msg := ranged[i]
			ok, offset, length := searchMatches(&msg, query)
			if !ok {
				continue
			}
			results = append(results, SearchResult{
				Message:     cloneMessage(msg),
				Topic:       "@" + dirAgent,
				MatchOffset: offset,
				MatchLength: length,
			})
		}
	}

	sort.SliceStable(results, func(i, j int) bool {
		if results[i].Message.ID != results[j].Message.ID {
			return results[i].Message.ID < results[j].Message.ID
		}
		return results[i].Topic < results[j].Topic
	})
	return results, nil
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
	seenPaths := make(map[string]struct{})
	ticker := time.NewTicker(p.pollInterval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
		}

		files, err := p.listSubscriptionFiles(filter)
		if err != nil {
			continue
		}
		for _, file := range files {
			if _, ok := seenPaths[file.path]; ok {
				continue
			}
			if lastSeenID != "" && file.id <= lastSeenID {
				seenPaths[file.path] = struct{}{}
				continue
			}

			message, ok, err := p.readMessageFile(file.path, file.entry)
			if err != nil {
				continue
			}
			if !ok {
				seenPaths[file.path] = struct{}{}
				continue
			}
			if !filter.Since.IsZero() && message.Time.Before(filter.Since) {
				seenPaths[file.path] = struct{}{}
				continue
			}
			if !messageMatchesSubscription(message, filter) {
				seenPaths[file.path] = struct{}{}
				continue
			}
			if message.ID != "" && message.ID > lastSeenID {
				lastSeenID = message.ID
			}
			seenPaths[file.path] = struct{}{}

			select {
			case <-ctx.Done():
				return
			case out <- cloneMessage(message):
			}
		}
	}
}

func (p *FileProvider) messagesForTopic(topic string) ([]fmail.Message, error) {
	if messages, ok := p.cachedTopicMessages(topic); ok {
		return messages, nil
	}
	messages, err := p.readMessagesFromDir(p.store.TopicDir(topic), time.Time{}, time.Time{})
	if err != nil {
		return nil, err
	}
	p.storeTopicMessages(topic, messages)
	return cloneMessages(messages), nil
}

func (p *FileProvider) messagesForDMDirectory(agent string) ([]fmail.Message, error) {
	if messages, ok := p.cachedDMMessages(agent); ok {
		return messages, nil
	}
	messages, err := p.readMessagesFromDir(p.store.DMDir(agent), time.Time{}, time.Time{})
	if err != nil {
		return nil, err
	}
	p.storeDMMessages(agent, messages)
	return cloneMessages(messages), nil
}

func (p *FileProvider) dmConversationMessages(viewer string, peer string) ([]fmail.Message, error) {
	dmDirs, err := p.listDMDirectoryNames()
	if err != nil {
		return nil, err
	}
	collected := make([]fmail.Message, 0)
	for _, dirAgent := range dmDirs {
		messages, err := p.messagesForDMDirectory(dirAgent)
		if err != nil {
			return nil, err
		}
		for i := range messages {
			msg := messages[i]
			if isDMBetween(msg, viewer, peer) {
				collected = append(collected, cloneMessage(msg))
			}
		}
	}
	sortMessagesByID(collected)
	return collected, nil
}

func (p *FileProvider) listTopicNames() ([]string, error) {
	root := filepath.Join(p.store.Root, "topics")
	entries, err := os.ReadDir(root)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return nil, nil
		}
		return nil, err
	}
	names := make([]string, 0, len(entries))
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		topic := strings.TrimSpace(entry.Name())
		if topic == "" {
			continue
		}
		if err := fmail.ValidateTopic(topic); err != nil {
			continue
		}
		names = append(names, topic)
	}
	sort.Strings(names)
	return names, nil
}

func (p *FileProvider) listDMDirectoryNames() ([]string, error) {
	root := filepath.Join(p.store.Root, "dm")
	entries, err := os.ReadDir(root)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return nil, nil
		}
		return nil, err
	}
	names := make([]string, 0, len(entries))
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		agent := strings.TrimSpace(entry.Name())
		if agent == "" {
			continue
		}
		if err := fmail.ValidateAgentName(agent); err != nil {
			continue
		}
		names = append(names, agent)
	}
	sort.Strings(names)
	return names, nil
}

func (p *FileProvider) readMessagesFromDir(dir string, since time.Time, until time.Time) ([]fmail.Message, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return nil, nil
		}
		return nil, err
	}

	names, entryByName := sortedJSONNames(entries)
	names = selectNamesByIDRange(names, since, until)

	messages := make([]fmail.Message, 0, len(names))
	for _, name := range names {
		entry := entryByName[name]
		path := filepath.Join(dir, name)
		message, ok, err := p.readMessageFile(path, entry)
		if err != nil {
			return nil, err
		}
		if !ok {
			continue
		}
		messages = append(messages, cloneMessage(message))
	}
	sortMessagesByID(messages)
	return messages, nil
}

func (p *FileProvider) readMessageFile(path string, entry os.DirEntry) (fmail.Message, bool, error) {
	info, err := entry.Info()
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return fmail.Message{}, false, nil
		}
		return fmail.Message{}, false, err
	}
	modTime := info.ModTime().UTC()
	if cached, ok := p.messageCache.get(path, modTime); ok {
		return cached, true, nil
	}

	message, err := p.store.ReadMessage(path)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return fmail.Message{}, false, nil
		}
		return fmail.Message{}, false, err
	}
	p.messageCache.put(path, modTime, *message)
	return cloneMessage(*message), true, nil
}

func sortedJSONNames(entries []os.DirEntry) ([]string, map[string]os.DirEntry) {
	names := make([]string, 0, len(entries))
	entryByName := make(map[string]os.DirEntry, len(entries))
	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}
		if filepath.Ext(entry.Name()) != ".json" {
			continue
		}
		name := entry.Name()
		names = append(names, name)
		entryByName[name] = entry
	}
	sort.Strings(names)
	return names, entryByName
}

func selectNamesByIDRange(names []string, since time.Time, until time.Time) []string {
	if len(names) == 0 {
		return nil
	}
	lower := idLowerBound(since)
	upper := idUpperBound(until)

	start := 0
	if lower != "" {
		start = sort.Search(len(names), func(i int) bool {
			return trimJSONSuffix(names[i]) >= lower
		})
	}
	end := len(names)
	if upper != "" {
		end = sort.Search(len(names), func(i int) bool {
			return trimJSONSuffix(names[i]) > upper
		})
	}
	if start > end {
		start = end
	}
	return names[start:end]
}

func trimJSONSuffix(name string) string {
	return strings.TrimSuffix(name, filepath.Ext(name))
}

func sliceMessagesByIDRange(messages []fmail.Message, since time.Time, until time.Time) []fmail.Message {
	if len(messages) == 0 {
		return nil
	}
	lower := idLowerBound(since)
	upper := idUpperBound(until)

	start := 0
	if lower != "" {
		start = sort.Search(len(messages), func(i int) bool {
			return messages[i].ID >= lower
		})
	}
	end := len(messages)
	if upper != "" {
		end = sort.Search(len(messages), func(i int) bool {
			return messages[i].ID > upper
		})
	}
	if start > end {
		start = end
	}
	return cloneMessages(messages[start:end])
}

type subscriptionFile struct {
	path  string
	id    string
	entry os.DirEntry
}

func (p *FileProvider) listSubscriptionFiles(filter SubscriptionFilter) ([]subscriptionFile, error) {
	dirs, err := p.subscriptionDirs(filter)
	if err != nil {
		return nil, err
	}
	files := make([]subscriptionFile, 0)
	for _, dir := range dirs {
		entries, err := os.ReadDir(dir)
		if err != nil {
			if errors.Is(err, os.ErrNotExist) {
				continue
			}
			return nil, err
		}
		for _, entry := range entries {
			if entry.IsDir() || filepath.Ext(entry.Name()) != ".json" {
				continue
			}
			files = append(files, subscriptionFile{
				path:  filepath.Join(dir, entry.Name()),
				id:    trimJSONSuffix(entry.Name()),
				entry: entry,
			})
		}
	}
	sort.SliceStable(files, func(i, j int) bool {
		if files[i].id != files[j].id {
			return files[i].id < files[j].id
		}
		return files[i].path < files[j].path
	})
	return files, nil
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

func isDMBetween(message fmail.Message, left string, right string) bool {
	target := strings.TrimPrefix(message.To, "@")
	if target == message.To {
		return false
	}
	return (strings.EqualFold(message.From, left) && strings.EqualFold(target, right)) ||
		(strings.EqualFold(message.From, right) && strings.EqualFold(target, left))
}

func normalizeViewerAgent(agent string, fallback string) (string, error) {
	trimmed := strings.TrimSpace(agent)
	if trimmed == "" {
		trimmed = strings.TrimSpace(fallback)
	}
	if trimmed == "" {
		return "", fmt.Errorf("viewer agent required")
	}
	return fmail.NormalizeAgentName(trimmed)
}

func dmPeer(viewer string, dmDir string, message fmail.Message) string {
	if message.To == "" || !strings.HasPrefix(message.To, "@") {
		return ""
	}
	target := strings.TrimPrefix(message.To, "@")
	if strings.EqualFold(message.From, viewer) {
		return target
	}
	if strings.EqualFold(target, viewer) {
		peer, err := fmail.NormalizeAgentName(message.From)
		if err != nil {
			return ""
		}
		return peer
	}
	if strings.EqualFold(dmDir, viewer) {
		peer, err := fmail.NormalizeAgentName(message.From)
		if err != nil {
			return ""
		}
		return peer
	}
	return ""
}

func (p *FileProvider) cachedTopics() ([]TopicInfo, bool) {
	p.mu.RLock()
	defer p.mu.RUnlock()
	if !p.topicsCache.ok || time.Now().After(p.topicsCache.expires) {
		return nil, false
	}
	return cloneTopics(p.topicsCache.value), true
}

func (p *FileProvider) storeTopics(topics []TopicInfo) {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.topicsCache = timedEntry[[]TopicInfo]{
		value:   cloneTopics(topics),
		expires: time.Now().Add(p.cacheTTL),
		ok:      true,
	}
}

func (p *FileProvider) cachedAgents() ([]fmail.AgentRecord, bool) {
	p.mu.RLock()
	defer p.mu.RUnlock()
	if !p.agentsCache.ok || time.Now().After(p.agentsCache.expires) {
		return nil, false
	}
	return cloneAgentRecords(p.agentsCache.value), true
}

func (p *FileProvider) storeAgents(records []fmail.AgentRecord) {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.agentsCache = timedEntry[[]fmail.AgentRecord]{
		value:   cloneAgentRecords(records),
		expires: time.Now().Add(p.cacheTTL),
		ok:      true,
	}
}

func (p *FileProvider) cachedTopicMessages(topic string) ([]fmail.Message, bool) {
	p.mu.RLock()
	defer p.mu.RUnlock()
	entry, ok := p.topicMsgCache[topic]
	if !ok || time.Now().After(entry.expires) {
		return nil, false
	}
	return cloneMessages(entry.value), true
}

func (p *FileProvider) storeTopicMessages(topic string, messages []fmail.Message) {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.topicMsgCache[topic] = timedEntry[[]fmail.Message]{
		value:   cloneMessages(messages),
		expires: time.Now().Add(p.cacheTTL),
		ok:      true,
	}
}

func (p *FileProvider) cachedDMMessages(agent string) ([]fmail.Message, bool) {
	p.mu.RLock()
	defer p.mu.RUnlock()
	entry, ok := p.dmMsgCache[agent]
	if !ok || time.Now().After(entry.expires) {
		return nil, false
	}
	return cloneMessages(entry.value), true
}

func (p *FileProvider) storeDMMessages(agent string, messages []fmail.Message) {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.dmMsgCache[agent] = timedEntry[[]fmail.Message]{
		value:   cloneMessages(messages),
		expires: time.Now().Add(p.cacheTTL),
		ok:      true,
	}
}
