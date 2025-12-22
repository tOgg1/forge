package db

import (
	"context"
	"testing"
	"time"

	"github.com/opencode-ai/swarm/internal/models"
)

func TestAccountRepository_CreateAndList(t *testing.T) {
	db := setupTestDB(t)
	defer db.Close()

	repo := NewAccountRepository(db)
	ctx := context.Background()

	cooldown := time.Now().UTC().Add(10 * time.Minute)

	account1 := &models.Account{
		Provider:      models.ProviderOpenAI,
		ProfileName:   "primary",
		CredentialRef: "env:OPENAI_API_KEY",
		IsActive:      true,
		UsageStats: &models.UsageStats{
			TotalTokens:    1234,
			TotalCostCents: 456,
			RequestCount:   12,
		},
	}

	account2 := &models.Account{
		Provider:      models.ProviderAnthropic,
		ProfileName:   "work",
		CredentialRef: "/tmp/anthropic.json",
		IsActive:      true,
		CooldownUntil: &cooldown,
	}

	if err := repo.Create(ctx, account1); err != nil {
		t.Fatalf("Create account1 failed: %v", err)
	}
	if err := repo.Create(ctx, account2); err != nil {
		t.Fatalf("Create account2 failed: %v", err)
	}

	all, err := repo.List(ctx, nil)
	if err != nil {
		t.Fatalf("List failed: %v", err)
	}
	if len(all) != 2 {
		t.Fatalf("expected 2 accounts, got %d", len(all))
	}

	provider := models.ProviderOpenAI
	openaiAccounts, err := repo.List(ctx, &provider)
	if err != nil {
		t.Fatalf("List by provider failed: %v", err)
	}
	if len(openaiAccounts) != 1 {
		t.Fatalf("expected 1 openai account, got %d", len(openaiAccounts))
	}

	got := openaiAccounts[0]
	if got.Provider != models.ProviderOpenAI {
		t.Fatalf("unexpected provider: %s", got.Provider)
	}
	if got.ProfileName != "primary" {
		t.Fatalf("unexpected profile name: %s", got.ProfileName)
	}
	if got.CredentialRef != "env:OPENAI_API_KEY" {
		t.Fatalf("unexpected credential ref: %s", got.CredentialRef)
	}
	if got.UsageStats == nil || got.UsageStats.TotalTokens != 1234 {
		t.Fatalf("unexpected usage stats: %+v", got.UsageStats)
	}
}
