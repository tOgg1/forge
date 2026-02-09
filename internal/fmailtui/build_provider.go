package fmailtui

import (
	"os"
	"strings"
	"time"

	"github.com/tOgg1/forge/internal/fmailtui/data"
)

func buildProvider(root, forgedAddr string) (data.MessageProvider, error) {
	fileProvider, err := data.NewFileProvider(data.FileProviderConfig{
		Root:         root,
		SelfAgent:    strings.TrimSpace(os.Getenv("FMAIL_AGENT")),
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
		Agent:             strings.TrimSpace(os.Getenv("FMAIL_AGENT")),
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
