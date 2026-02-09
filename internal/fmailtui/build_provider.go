package fmailtui

import (
	"strings"
	"time"

	"github.com/tOgg1/forge/internal/fmailtui/data"
)

const defaultFileProviderPollInterval = 100 * time.Millisecond

func buildProvider(root, forgedAddr, selfAgent string) (data.MessageProvider, error) {
	selfAgent = strings.TrimSpace(selfAgent)
	fileProvider, err := data.NewFileProvider(data.FileProviderConfig{
		Root:         root,
		SelfAgent:    selfAgent,
		PollInterval: 100 * time.Millisecond,
	})
	if err != nil {
		return nil, err
	}

	trimmedAddr := strings.TrimSpace(forgedAddr)
	if trimmedAddr == "" && !forgedSocketExists(root) {
		return fileProvider, nil
	}

	forgedProvider, err := data.NewForgedProvider(data.ForgedProviderConfig{
		Root:              root,
		Addr:              trimmedAddr,
		Agent:             selfAgent,
		ReconnectInterval: 2 * time.Second,
		SubscribeBuffer:   512,
		Fallback:          fileProvider,
	})
	if err != nil {
		if trimmedAddr != "" {
			return nil, err
		}
		return fileProvider, nil
	}

	return data.NewHybridProvider(data.HybridProviderConfig{
		Root:              root,
		ReconnectInterval: 2 * time.Second,
		SubscribeBuffer:   512,
		FileProvider:      fileProvider,
		ForgedProvider:    forgedProvider,
	})
}
