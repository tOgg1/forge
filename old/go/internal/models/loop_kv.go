package models

import (
	"strings"
	"time"
)

type LoopKV struct {
	ID        string    `json:"id"`
	LoopID    string    `json:"loop_id"`
	Key       string    `json:"key"`
	Value     string    `json:"value"`
	CreatedAt time.Time `json:"created_at"`
	UpdatedAt time.Time `json:"updated_at"`
}

func (kv *LoopKV) Validate() error {
	validation := &ValidationErrors{}
	if strings.TrimSpace(kv.LoopID) == "" {
		validation.AddMessage("loop_id", "loop_id is required")
	}
	if strings.TrimSpace(kv.Key) == "" {
		validation.AddMessage("key", "key is required")
	}
	if kv.Value == "" {
		validation.AddMessage("value", "value is required")
	}
	return validation.Err()
}
