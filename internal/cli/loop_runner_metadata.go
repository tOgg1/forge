package cli

import (
	"context"
	"strings"

	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/models"
)

const (
	loopMetadataRunnerOwnerKey      = "runner_owner"
	loopMetadataRunnerInstanceIDKey = "runner_instance_id"
	loopMetadataRunnerLivenessKey   = "runner_liveness"
	loopStaleRunnerReason           = "stale_runner"
)

func setLoopRunnerMetadata(ctx context.Context, loopRepo *db.LoopRepository, loopID string, owner loopSpawnOwner, instanceID string) error {
	loopEntry, err := loopRepo.Get(ctx, loopID)
	if err != nil {
		return err
	}
	if loopEntry.Metadata == nil {
		loopEntry.Metadata = make(map[string]any)
	}

	loopEntry.Metadata[loopMetadataRunnerOwnerKey] = string(owner)
	if instanceID == "" {
		delete(loopEntry.Metadata, loopMetadataRunnerInstanceIDKey)
	} else {
		loopEntry.Metadata[loopMetadataRunnerInstanceIDKey] = instanceID
	}

	return loopRepo.Update(ctx, loopEntry)
}

func loopRunnerOwner(loopEntry *models.Loop) string {
	if loopEntry == nil || loopEntry.Metadata == nil {
		return ""
	}
	value, ok := loopEntry.Metadata[loopMetadataRunnerOwnerKey]
	if !ok {
		return ""
	}
	return strings.TrimSpace(toString(value))
}

func loopRunnerInstanceID(loopEntry *models.Loop) string {
	if loopEntry == nil || loopEntry.Metadata == nil {
		return ""
	}
	value, ok := loopEntry.Metadata[loopMetadataRunnerInstanceIDKey]
	if !ok {
		return ""
	}
	return strings.TrimSpace(toString(value))
}

func toString(value any) string {
	switch v := value.(type) {
	case string:
		return v
	default:
		return ""
	}
}
