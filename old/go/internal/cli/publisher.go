package cli

import (
	"strings"

	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/events"
	"github.com/tOgg1/forge/internal/hooks"
)

func newEventPublisher(database *db.DB) events.Publisher {
	if database == nil {
		return nil
	}

	repo := db.NewEventRepository(database)
	publisher := events.NewInMemoryPublisher(events.WithRepository(repo))

	store := hooks.NewStore(hookStorePath())
	manager := hooks.NewManager(store, nil)
	if err := manager.Attach(publisher); err != nil {
		logger.Warn().Err(err).Str("store", strings.TrimSpace(store.Path())).Msg("failed to load hooks")
	}

	return publisher
}
