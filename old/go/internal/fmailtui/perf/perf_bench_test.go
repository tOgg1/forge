//go:build perf

package perf

import (
	"testing"
	"time"

	"github.com/tOgg1/forge/internal/fmailtui/data"
)

func BenchmarkPerf_FileProvider_TopicsCold(b *testing.B) {
	b.ReportAllocs()
	cfg := datasetConfig{
		topics:         200,
		topicMessages:  20,
		dmPeers:        50,
		dmMessagesEach: 20,
		agents:         30,
	}

	b.StopTimer()
	for i := 0; i < b.N; i++ {
		projectRoot := b.TempDir()
		writeSyntheticMailbox(b, projectRoot, cfg)

		provider, err := data.NewFileProvider(data.FileProviderConfig{
			Root:      projectRoot,
			CacheTTL:  30 * time.Second,
			SelfAgent: "viewer",
		})
		if err != nil {
			b.Fatalf("new provider: %v", err)
		}

		b.StartTimer()
		_, err = provider.Topics()
		b.StopTimer()
		if err != nil {
			b.Fatalf("Topics: %v", err)
		}
	}
}

func BenchmarkPerf_FileProvider_TopicsWarm(b *testing.B) {
	b.ReportAllocs()
	projectRoot := b.TempDir()
	writeSyntheticMailbox(b, projectRoot, datasetConfig{
		topics:         200,
		topicMessages:  20,
		dmPeers:        50,
		dmMessagesEach: 20,
		agents:         30,
	})

	provider, err := data.NewFileProvider(data.FileProviderConfig{
		Root:      projectRoot,
		CacheTTL:  30 * time.Second,
		SelfAgent: "viewer",
	})
	if err != nil {
		b.Fatalf("new provider: %v", err)
	}
	if _, err := provider.Topics(); err != nil {
		b.Fatalf("warmup Topics: %v", err)
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		if _, err := provider.Topics(); err != nil {
			b.Fatalf("Topics: %v", err)
		}
	}
}

func BenchmarkPerf_FileProvider_SearchCold(b *testing.B) {
	b.ReportAllocs()
	cfg := datasetConfig{
		topics:         200,
		topicMessages:  20,
		dmPeers:        50,
		dmMessagesEach: 20,
		agents:         30,
	}

	b.StopTimer()
	for i := 0; i < b.N; i++ {
		projectRoot := b.TempDir()
		writeSyntheticMailbox(b, projectRoot, cfg)

		provider, err := data.NewFileProvider(data.FileProviderConfig{
			Root:      projectRoot,
			CacheTTL:  30 * time.Second,
			SelfAgent: "viewer",
		})
		if err != nil {
			b.Fatalf("new provider: %v", err)
		}

		b.StartTimer()
		_, err = provider.Search(data.SearchQuery{Text: "needle"})
		b.StopTimer()
		if err != nil {
			b.Fatalf("Search(cold): %v", err)
		}
	}
}

func BenchmarkPerf_FileProvider_SearchWarm(b *testing.B) {
	b.ReportAllocs()
	projectRoot := b.TempDir()
	writeSyntheticMailbox(b, projectRoot, datasetConfig{
		topics:         200,
		topicMessages:  20,
		dmPeers:        50,
		dmMessagesEach: 20,
		agents:         30,
	})

	provider, err := data.NewFileProvider(data.FileProviderConfig{
		Root:      projectRoot,
		CacheTTL:  30 * time.Second,
		SelfAgent: "viewer",
	})
	if err != nil {
		b.Fatalf("new provider: %v", err)
	}
	if _, err := provider.Search(data.SearchQuery{Text: "needle"}); err != nil {
		b.Fatalf("warmup Search: %v", err)
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		if _, err := provider.Search(data.SearchQuery{Text: "needle"}); err != nil {
			b.Fatalf("Search(warm): %v", err)
		}
	}
}
