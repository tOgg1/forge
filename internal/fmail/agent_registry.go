package fmail

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// AgentRecord tracks agent presence in the project.
type AgentRecord struct {
	Name      string    `json:"name"`
	Host      string    `json:"host,omitempty"`
	Status    string    `json:"status,omitempty"`
	FirstSeen time.Time `json:"first_seen"`
	LastSeen  time.Time `json:"last_seen"`
}

// UpdateAgentRecord creates or updates the agent registry entry.
func (s *Store) UpdateAgentRecord(name, host string) (*AgentRecord, error) {
	if s == nil {
		return nil, fmt.Errorf("store is nil")
	}
	normalized, err := NormalizeAgentName(name)
	if err != nil {
		return nil, err
	}
	if err := s.EnsureRoot(); err != nil {
		return nil, err
	}
	if err := os.MkdirAll(s.AgentsDir(), 0o755); err != nil {
		return nil, err
	}

	path := filepath.Join(s.AgentsDir(), normalized+".json")
	now := s.now()

	record := AgentRecord{
		Name:      normalized,
		FirstSeen: now,
		LastSeen:  now,
	}

	data, err := os.ReadFile(path)
	if err == nil {
		if err := json.Unmarshal(data, &record); err != nil {
			return nil, err
		}
		if record.Name == "" {
			record.Name = normalized
		}
		if record.FirstSeen.IsZero() {
			record.FirstSeen = now
		}
		record.LastSeen = now
	} else if !errors.Is(err, os.ErrNotExist) {
		return nil, err
	}

	host = strings.TrimSpace(host)
	if host != "" {
		record.Host = host
	}

	encoded, err := json.MarshalIndent(record, "", "  ")
	if err != nil {
		return nil, err
	}
	if err := os.WriteFile(path, encoded, 0o644); err != nil {
		return nil, err
	}
	return &record, nil
}
