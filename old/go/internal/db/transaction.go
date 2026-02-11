// Package db provides SQLite database access for Forge.
package db

import (
	"context"
	"database/sql"
	"errors"
	"strings"
	"time"
)

const (
	defaultRetryAttempts = 3
	defaultRetryBackoff  = 50 * time.Millisecond
)

// TransactionWithRetry runs a transaction with retry handling for busy database errors.
func (db *DB) TransactionWithRetry(ctx context.Context, maxAttempts int, baseBackoff time.Duration, fn func(*sql.Tx) error) error {
	if maxAttempts <= 0 {
		maxAttempts = defaultRetryAttempts
	}
	if baseBackoff <= 0 {
		baseBackoff = defaultRetryBackoff
	}

	return withRetry(ctx, maxAttempts, baseBackoff, func() error {
		return db.Transaction(ctx, fn)
	})
}

func withRetry(ctx context.Context, maxAttempts int, baseBackoff time.Duration, fn func() error) error {
	attempt := 0
	backoff := baseBackoff

	for {
		if ctx.Err() != nil {
			return ctx.Err()
		}

		err := fn()
		if err == nil {
			return nil
		}

		attempt++
		if !isBusyError(err) || attempt >= maxAttempts {
			return err
		}

		if err := sleepWithContext(ctx, backoff); err != nil {
			return err
		}

		backoff *= 2
	}
}

func isBusyError(err error) bool {
	if err == nil {
		return false
	}
	if errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) {
		return false
	}

	message := strings.ToLower(err.Error())
	return strings.Contains(message, "database is locked") ||
		strings.Contains(message, "database is busy") ||
		strings.Contains(message, "sqlite_busy")
}

func sleepWithContext(ctx context.Context, duration time.Duration) error {
	timer := time.NewTimer(duration)
	defer timer.Stop()

	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-timer.C:
		return nil
	}
}
