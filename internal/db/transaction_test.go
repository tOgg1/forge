package db

import (
	"context"
	"database/sql"
	"errors"
	"testing"
	"time"
)

func TestWithRetry_RetriesOnBusy(t *testing.T) {
	ctx := context.Background()
	attempts := 0

	err := withRetry(ctx, 3, time.Millisecond, func() error {
		attempts++
		if attempts < 3 {
			return errors.New("database is locked")
		}
		return nil
	})

	if err != nil {
		t.Fatalf("expected no error, got %v", err)
	}
	if attempts != 3 {
		t.Fatalf("expected 3 attempts, got %d", attempts)
	}
}

func TestWithRetry_StopsOnNonBusy(t *testing.T) {
	ctx := context.Background()
	attempts := 0

	err := withRetry(ctx, 3, time.Millisecond, func() error {
		attempts++
		return errors.New("boom")
	})

	if err == nil {
		t.Fatal("expected error")
	}
	if attempts != 1 {
		t.Fatalf("expected 1 attempt, got %d", attempts)
	}
}

func TestWithRetry_StopsAfterMaxAttempts(t *testing.T) {
	ctx := context.Background()
	attempts := 0

	err := withRetry(ctx, 2, time.Millisecond, func() error {
		attempts++
		return errors.New("database is busy")
	})

	if err == nil {
		t.Fatal("expected error")
	}
	if attempts != 2 {
		t.Fatalf("expected 2 attempts, got %d", attempts)
	}
}

func TestTransactionWithRetry(t *testing.T) {
	db := setupTestDB(t)
	defer db.Close()

	ctx := context.Background()
	attempts := 0

	err := db.TransactionWithRetry(ctx, 3, time.Millisecond, func(tx *sql.Tx) error {
		attempts++
		if attempts < 2 {
			return errors.New("database is locked")
		}
		return nil
	})

	if err != nil {
		t.Fatalf("expected no error, got %v", err)
	}
	if attempts != 2 {
		t.Fatalf("expected 2 attempts, got %d", attempts)
	}
}
