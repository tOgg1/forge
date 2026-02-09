package data

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
)

type FileProvider struct {
	root            string
	store           *fmail.Store
	cacheTTL        time.Duration
	metadataTTL     time.Duration
	pollMin         time.Duration
	pollMax         time.Duration
	subscribeBuffer int
	selfAgent       string
	messageCache    *messageCache

	mu                   sync.RWMutex
	topicsCache          timedEntry[[]TopicInfo]
	agentsCache          timedEntry[[]fmail.AgentRecord]
	topicMsgCache        map[string]timedEntry[[]fmail.Message]
	dmMsgCache           map[string]timedEntry[[]fmail.Message]
	topicMetadataCache   map[string]topicMetadataEntry
	dmDirMetadataCache   map[string]dmDirMetadataEntry
	dmConversationsCache map[string]timedEntry[[]DMConversation]

	searchMu    sync.Mutex
	searchIndex *textSearchIndex

	messageReadLookups atomic.Int64
	messageDiskReads   atomic.Int64
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
	metadataTTL := cfg.MetadataTTL
	if metadataTTL <= 0 {
		metadataTTL = defaultMetadataTTL
	}
	pollMin := cfg.PollInterval
	if pollMin <= 0 {
		pollMin = defaultPollInterval
	}
	pollMax := cfg.PollMax
	if pollMax <= 0 {
		pollMax = defaultPollMax
	}
	if pollMax < pollMin {
		pollMax = pollMin
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
		root:                 root,
		store:                store,
		cacheTTL:             cacheTTL,
		metadataTTL:          metadataTTL,
		pollMin:              pollMin,
		pollMax:              pollMax,
		subscribeBuffer:      subscribeBuffer,
		selfAgent:            selfAgent,
		messageCache:         newMessageCache(cacheSize),
		topicMsgCache:        make(map[string]timedEntry[[]fmail.Message]),
		dmMsgCache:           make(map[string]timedEntry[[]fmail.Message]),
		topicMetadataCache:   make(map[string]topicMetadataEntry),
		dmDirMetadataCache:   make(map[string]dmDirMetadataEntry),
		dmConversationsCache: make(map[string]timedEntry[[]DMConversation]),
	}, nil
}

func (p *FileProvider) Topics() ([]TopicInfo, error) {
	if topics, ok := p.cachedTopics(); ok {
		return topics, nil
	}

	topics, err := p.buildTopicsFromMetadata()
	if err != nil {
		return nil, err
	}
	p.storeTopics(topics)
	return cloneTopics(topics), nil
}

func (p *FileProvider) Messages(topic string, opts MessageFilter) ([]fmail.Message, error) {
	normalized, err := fmail.NormalizeTopic(topic)
	if err != nil {
		return nil, err
	}

	if shouldUseWindowedRead(opts) {
		messages, err := p.readMessagesFromDir(p.store.TopicDir(normalized), opts.Since, opts.Until, opts.Limit)
		if err != nil {
			return nil, err
		}
		return applyMessageFilter(messages, opts), nil
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
	return p.buildDMConversationsFromMetadata(viewer)
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
		messages, err := p.dmMessagesByDir(target, opts)
		if err != nil {
			return nil, err
		}
		ranged := sliceMessagesByIDRange(messages, opts.Since, opts.Until)
		return applyMessageFilter(ranged, opts), nil
	}

	allMessages, err := p.dmConversationMessages(viewer, target, opts)
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
	if strings.TrimSpace(query.Text) != "" {
		return p.searchWithIndex(query)
	}
	return p.searchLinear(query)
}

func (p *FileProvider) searchLinear(query SearchQuery) ([]SearchResult, error) {
	topicNames, err := p.listTopicNames()
	if err != nil {
		return nil, err
	}
	dmDirs, err := p.listDMDirectoryNames()
	if err != nil {
		return nil, err
	}

	scope := strings.TrimSpace(query.In)
	if scope != "" {
		if strings.HasPrefix(scope, "@") {
			agent := strings.TrimPrefix(scope, "@")
			topicNames = nil
			dmDirs = []string{agent}
		} else {
			dmDirs = nil
			topicNames = []string{scope}
		}
	}

	results := make([]SearchResult, 0)
	for _, topic := range topicNames {
		messages, err := p.messagesForTopic(topic)
		if err != nil {
			return nil, err
		}
		ranged := sliceMessagesByIDRange(messages, query.Since, query.Until)
		var hasReplies map[string]struct{}
		if query.HasReply {
			hasReplies = make(map[string]struct{}, len(ranged))
			for i := range ranged {
				if parent := strings.TrimSpace(ranged[i].ReplyTo); parent != "" {
					hasReplies[parent] = struct{}{}
				}
			}
		}
		for i := range ranged {
			msg := ranged[i]
			ok, offset, length := searchMatches(&msg, query)
			if !ok {
				continue
			}
			if hasReplies != nil {
				if _, ok := hasReplies[strings.TrimSpace(msg.ID)]; !ok {
					continue
				}
			}
			var prev *fmail.Message
			var next *fmail.Message
			if i > 0 {
				pm := cloneMessage(ranged[i-1])
				prev = &pm
			}
			if i+1 < len(ranged) {
				nm := cloneMessage(ranged[i+1])
				next = &nm
			}
			results = append(results, SearchResult{
				Message:     cloneMessage(msg),
				Topic:       topic,
				MatchOffset: offset,
				MatchLength: length,
				Prev:        prev,
				Next:        next,
			})
		}
	}

	for _, dirAgent := range dmDirs {
		messages, err := p.messagesForDMDirectory(dirAgent)
		if err != nil {
			return nil, err
		}
		ranged := sliceMessagesByIDRange(messages, query.Since, query.Until)
		var hasReplies map[string]struct{}
		if query.HasReply {
			hasReplies = make(map[string]struct{}, len(ranged))
			for i := range ranged {
				if parent := strings.TrimSpace(ranged[i].ReplyTo); parent != "" {
					hasReplies[parent] = struct{}{}
				}
			}
		}
		for i := range ranged {
			msg := ranged[i]
			ok, offset, length := searchMatches(&msg, query)
			if !ok {
				continue
			}
			if hasReplies != nil {
				if _, ok := hasReplies[strings.TrimSpace(msg.ID)]; !ok {
					continue
				}
			}
			var prev *fmail.Message
			var next *fmail.Message
			if i > 0 {
				pm := cloneMessage(ranged[i-1])
				prev = &pm
			}
			if i+1 < len(ranged) {
				nm := cloneMessage(ranged[i+1])
				next = &nm
			}
			results = append(results, SearchResult{
				Message:     cloneMessage(msg),
				Topic:       "@" + dirAgent,
				MatchOffset: offset,
				MatchLength: length,
				Prev:        prev,
				Next:        next,
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

func (p *FileProvider) searchWithIndex(query SearchQuery) ([]SearchResult, error) {
	now := time.Now().UTC()
	idx, err := p.ensureTextIndex(now)
	if err != nil {
		return nil, err
	}

	scope := strings.TrimSpace(query.In)
	if scope != "" {
		if !strings.HasPrefix(scope, "@") {
			// Keep as-is for topic scope.
		} else {
			// Ensure DM scope matches index target keys.
			scope = "@" + strings.TrimPrefix(scope, "@")
		}
	}

	terms := tokenizeForIndex(strings.ToLower(query.Text))
	if len(terms) == 0 {
		// Fallback to linear scan for non-tokenizable input.
		query.Text = strings.TrimSpace(query.Text)
		query.In = scope
		return p.searchLinear(query)
	}

	// Build candidate set by intersecting postings for all query terms.
	candidates := make(map[searchRef]struct{})
	first := true
	for _, term := range terms {
		refs := idx.postings[term]
		if len(refs) == 0 {
			return nil, nil
		}
		if first {
			for _, r := range refs {
				candidates[r] = struct{}{}
			}
			first = false
			continue
		}
		next := make(map[searchRef]struct{}, len(candidates))
		for _, r := range refs {
			if _, ok := candidates[r]; ok {
				next[r] = struct{}{}
			}
		}
		candidates = next
		if len(candidates) == 0 {
			return nil, nil
		}
	}

	results := make([]SearchResult, 0, len(candidates))
	replyByTarget := make(map[string]map[string]struct{})

	for ref := range candidates {
		if scope != "" && ref.target != scope {
			continue
		}
		target, ok := idx.targets[ref.target]
		if !ok || ref.idx < 0 || ref.idx >= len(target.messages) {
			continue
		}
		msg := target.messages[ref.idx]

		// has:reply needs the full target slice; build lazily per target.
		if query.HasReply {
			set, ok := replyByTarget[ref.target]
			if !ok {
				set = make(map[string]struct{}, len(target.messages))
				for i := range target.messages {
					if parent := strings.TrimSpace(target.messages[i].ReplyTo); parent != "" {
						set[parent] = struct{}{}
					}
				}
				replyByTarget[ref.target] = set
			}
			if _, ok := set[strings.TrimSpace(msg.ID)]; !ok {
				continue
			}
		}

		ok, offset, length := searchMatches(&msg, query)
		if !ok {
			continue
		}

		var prev *fmail.Message
		var next *fmail.Message
		if ref.idx > 0 {
			pm := cloneMessage(target.messages[ref.idx-1])
			prev = &pm
		}
		if ref.idx+1 < len(target.messages) {
			nm := cloneMessage(target.messages[ref.idx+1])
			next = &nm
		}

		results = append(results, SearchResult{
			Message:     cloneMessage(msg),
			Topic:       ref.target,
			MatchOffset: offset,
			MatchLength: length,
			Prev:        prev,
			Next:        next,
		})
	}

	sort.SliceStable(results, func(i, j int) bool {
		if results[i].Message.ID != results[j].Message.ID {
			return results[i].Message.ID < results[j].Message.ID
		}
		return results[i].Topic < results[j].Topic
	})
	return results, nil
}

func (p *FileProvider) Send(req SendRequest) (fmail.Message, error) {
	msg, err := normalizeSendRequest(req, p.selfAgent)
	if err != nil {
		return fmail.Message{}, err
	}
	messageID, err := p.store.SaveMessage(&msg)
	if err != nil {
		return fmail.Message{}, err
	}
	msg.ID = messageID

	// Invalidate caches so the new message is visible without waiting for TTL expiry.
	p.invalidateCachesForMessage(msg)

	return msg, nil
}

func (p *FileProvider) messagesForTopic(topic string) ([]fmail.Message, error) {
	if messages, ok := p.cachedTopicMessages(topic); ok {
		return messages, nil
	}
	messages, err := p.readMessagesFromDir(p.store.TopicDir(topic), time.Time{}, time.Time{}, 0)
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
	messages, err := p.readMessagesFromDir(p.store.DMDir(agent), time.Time{}, time.Time{}, 0)
	if err != nil {
		return nil, err
	}
	p.storeDMMessages(agent, messages)
	return cloneMessages(messages), nil
}

func (p *FileProvider) dmConversationMessages(viewer string, peer string, opts MessageFilter) ([]fmail.Message, error) {
	readOpts := opts
	if hasMessageAttributeFilters(opts) {
		readOpts.Limit = 0
	}
	dmDirs := dedupeSortedStrings([]string{viewer, peer})

	collected := make([]fmail.Message, 0)
	for _, dirAgent := range dmDirs {
		messages, err := p.dmMessagesByDir(dirAgent, readOpts)
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
	if opts.Limit > 0 && len(collected) > opts.Limit {
		collected = collected[len(collected)-opts.Limit:]
	}
	return collected, nil
}

func (p *FileProvider) dmMessagesByDir(agent string, opts MessageFilter) ([]fmail.Message, error) {
	if shouldUseWindowedRead(opts) {
		messages, err := p.readMessagesFromDir(p.store.DMDir(agent), opts.Since, opts.Until, opts.Limit)
		if err != nil {
			return nil, err
		}
		return messages, nil
	}
	return p.messagesForDMDirectory(agent)
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

func (p *FileProvider) readMessagesFromDir(dir string, since time.Time, until time.Time, limit int) ([]fmail.Message, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return nil, nil
		}
		return nil, err
	}

	names, entryByName := sortedJSONNames(entries)
	names = selectNamesByIDRange(names, since, until)
	if limit > 0 && len(names) > limit {
		names = names[len(names)-limit:]
	}

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

func shouldUseWindowedRead(opts MessageFilter) bool {
	return (opts.Limit > 0 || !opts.Since.IsZero() || !opts.Until.IsZero()) && !hasMessageAttributeFilters(opts)
}

func hasMessageAttributeFilters(opts MessageFilter) bool {
	if strings.TrimSpace(opts.From) != "" {
		return true
	}
	if strings.TrimSpace(opts.Priority) != "" {
		return true
	}
	if strings.TrimSpace(opts.To) != "" {
		return true
	}
	return len(opts.Tags) > 0
}

func dedupeSortedStrings(values []string) []string {
	if len(values) == 0 {
		return nil
	}
	out := make([]string, 0, len(values))
	seen := make(map[string]struct{}, len(values))
	for _, value := range values {
		value = strings.TrimSpace(value)
		if value == "" {
			continue
		}
		if _, ok := seen[value]; ok {
			continue
		}
		seen[value] = struct{}{}
		out = append(out, value)
	}
	sort.Strings(out)
	return out
}

func (p *FileProvider) readMessageFile(path string, entry os.DirEntry) (fmail.Message, bool, error) {
	p.messageReadLookups.Add(1)

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
	p.messageDiskReads.Add(1)
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
