package data

import (
	"context"
	"strings"
	"time"

	"github.com/tOgg1/forge/internal/fmail"
)

type HybridProvider struct {
	root              string
	reconnectInterval time.Duration
	subscribeBuffer   int
	file              *FileProvider
	forged            *ForgedProvider
}

type subscribeSource int

const (
	sourceFile subscribeSource = iota
	sourceForged
)

func NewHybridProvider(cfg HybridProviderConfig) (*HybridProvider, error) {
	root, err := normalizeRoot(cfg.Root)
	if err != nil {
		return nil, err
	}

	fileProvider := cfg.FileProvider
	if fileProvider == nil {
		fileProvider, err = NewFileProvider(FileProviderConfig{Root: root})
		if err != nil {
			return nil, err
		}
	}

	forgedProvider := cfg.ForgedProvider
	if forgedProvider == nil {
		forgedProvider, err = NewForgedProvider(ForgedProviderConfig{
			Root:     root,
			Fallback: fileProvider,
		})
		if err != nil {
			return nil, err
		}
	}

	reconnectInterval := cfg.ReconnectInterval
	if reconnectInterval <= 0 {
		reconnectInterval = defaultReconnectInterval
	}
	subscribeBuffer := cfg.SubscribeBuffer
	if subscribeBuffer <= 0 {
		subscribeBuffer = defaultSubscribeBufferSize
	}

	return &HybridProvider{
		root:              root,
		reconnectInterval: reconnectInterval,
		subscribeBuffer:   subscribeBuffer,
		file:              fileProvider,
		forged:            forgedProvider,
	}, nil
}

func (p *HybridProvider) Topics() ([]TopicInfo, error) {
	return p.file.Topics()
}

func (p *HybridProvider) Messages(topic string, opts MessageFilter) ([]fmail.Message, error) {
	return p.file.Messages(topic, opts)
}

func (p *HybridProvider) DMConversations(agent string) ([]DMConversation, error) {
	return p.file.DMConversations(agent)
}

func (p *HybridProvider) DMs(agent string, opts MessageFilter) ([]fmail.Message, error) {
	return p.file.DMs(agent, opts)
}

func (p *HybridProvider) Agents() ([]fmail.AgentRecord, error) {
	return p.file.Agents()
}

func (p *HybridProvider) Search(query SearchQuery) ([]SearchResult, error) {
	return p.file.Search(query)
}

func (p *HybridProvider) Subscribe(filter SubscriptionFilter) (<-chan fmail.Message, func()) {
	ctx, cancel := context.WithCancel(context.Background())
	out := make(chan fmail.Message, p.subscribeBuffer)
	go p.subscribeLoop(ctx, out, filter)
	return out, cancel
}

func (p *HybridProvider) subscribeLoop(ctx context.Context, out chan<- fmail.Message, filter SubscriptionFilter) {
	defer close(out)

	lastSeenID := strings.TrimSpace(filter.SinceID)
	source := p.initialSource()
	sourceCh, sourceCancel := p.startSource(source, filter, lastSeenID)
	defer sourceCancel()

	ticker := time.NewTicker(p.reconnectInterval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return
		case message, ok := <-sourceCh:
			if !ok {
				source = sourceFile
				sourceCh, sourceCancel = p.startSource(source, filter, lastSeenID)
				continue
			}
			if message.ID != "" && message.ID > lastSeenID {
				lastSeenID = message.ID
			}
			select {
			case <-ctx.Done():
				return
			case out <- cloneMessage(message):
			}
		case <-ticker.C:
			if source == sourceFile && p.forgedAvailable() {
				nextCh, nextCancel := p.startSource(sourceForged, filter, lastSeenID)
				sourceCancel()
				source = sourceForged
				sourceCh = nextCh
				sourceCancel = nextCancel
				continue
			}
			if source == sourceForged && !p.forgedAvailable() {
				nextCh, nextCancel := p.startSource(sourceFile, filter, lastSeenID)
				sourceCancel()
				source = sourceFile
				sourceCh = nextCh
				sourceCancel = nextCancel
			}
		}
	}
}

func (p *HybridProvider) startSource(source subscribeSource, filter SubscriptionFilter, lastSeenID string) (<-chan fmail.Message, func()) {
	nextFilter := filter
	nextFilter.SinceID = strings.TrimSpace(lastSeenID)

	if source == sourceForged {
		return p.forged.Subscribe(nextFilter)
	}
	return p.file.Subscribe(nextFilter)
}

func (p *HybridProvider) initialSource() subscribeSource {
	if p.forgedAvailable() {
		return sourceForged
	}
	return sourceFile
}

func (p *HybridProvider) forgedAvailable() bool {
	if forgedSocketExists(p.root) {
		return true
	}
	if strings.TrimSpace(p.forged.addr) == "" {
		return false
	}
	return p.forged.canConnect(context.Background())
}
