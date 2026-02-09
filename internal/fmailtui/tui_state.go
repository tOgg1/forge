package fmailtui

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
)

type tuiStateFile struct {
	ReadMarkers   map[string]string `json:"read_markers,omitempty"`
	StarredTopics []string          `json:"starred_topics,omitempty"`
}

func loadTUIState(path string) (tuiStateFile, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return tuiStateFile{}, nil
		}
		return tuiStateFile{}, err
	}
	var state tuiStateFile
	if err := json.Unmarshal(data, &state); err != nil {
		return tuiStateFile{}, err
	}
	return state, nil
}

func saveTUIState(path string, state tuiStateFile) error {
	payload, err := json.MarshalIndent(state, "", "  ")
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, payload, 0o644); err != nil {
		return err
	}
	return os.Rename(tmp, path)
}
