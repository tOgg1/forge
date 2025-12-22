package db

import (
	"context"
	"errors"
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

func TestAccountRepository_GetUpdateDelete(t *testing.T) {
	db := setupTestDB(t)
	defer db.Close()

	repo := NewAccountRepository(db)
	ctx := context.Background()

	account := &models.Account{
		Provider:      models.ProviderOpenAI,
		ProfileName:   "primary",
		CredentialRef: "env:OPENAI_API_KEY",
		IsActive:      true,
	}

	if err := repo.Create(ctx, account); err != nil {
		t.Fatalf("Create failed: %v", err)
	}

	got, err := repo.Get(ctx, account.ID)
	if err != nil {
		t.Fatalf("Get failed: %v", err)
	}
	if got.ProfileName != "primary" {
		t.Fatalf("unexpected profile name: %s", got.ProfileName)
	}

	cooldown := time.Now().UTC().Add(5 * time.Minute)
	account.CredentialRef = "env:OTHER_KEY"
	account.IsActive = false
	account.CooldownUntil = &cooldown
	account.UsageStats = &models.UsageStats{
		TotalTokens: 10,
	}

	if err := repo.Update(ctx, account); err != nil {
		t.Fatalf("Update failed: %v", err)
	}

	updated, err := repo.Get(ctx, account.ID)
	if err != nil {
		t.Fatalf("Get after update failed: %v", err)
	}
	if updated.CredentialRef != "env:OTHER_KEY" {
		t.Fatalf("unexpected credential ref: %s", updated.CredentialRef)
	}
	if updated.IsActive {
		t.Fatal("expected account to be inactive")
	}
	if updated.CooldownUntil == nil {
		t.Fatal("expected cooldown to be set")
	}
	if updated.UsageStats == nil || updated.UsageStats.TotalTokens != 10 {
		t.Fatalf("unexpected usage stats: %+v", updated.UsageStats)
	}

	if err := repo.Delete(ctx, account.ID); err != nil {
		t.Fatalf("Delete failed: %v", err)
	}

	if _, err := repo.Get(ctx, account.ID); !errors.Is(err, ErrAccountNotFound) {
		t.Fatalf("expected ErrAccountNotFound, got %v", err)
	}
}

func TestAccountRepository_SetAndClearCooldown(t *testing.T) {
	db := setupTestDB(t)
	defer db.Close()

	repo := NewAccountRepository(db)
	ctx := context.Background()

	account := &models.Account{
		Provider:      models.ProviderOpenAI,
		ProfileName:   "cooldown",
		CredentialRef: "env:OPENAI_API_KEY",
		IsActive:      true,
	}

	if err := repo.Create(ctx, account); err != nil {
		t.Fatalf("Create failed: %v", err)
	}

	until := time.Now().UTC().Add(10 * time.Minute)
	if err := repo.SetCooldown(ctx, account.ID, until); err != nil {
		t.Fatalf("SetCooldown failed: %v", err)
	}

	got, err := repo.Get(ctx, account.ID)
	if err != nil {
		t.Fatalf("Get failed: %v", err)
	}
	if got.CooldownUntil == nil {
		t.Fatal("expected cooldown to be set")
	}
	if got.CooldownUntil.UTC().Format(time.RFC3339) != until.UTC().Format(time.RFC3339) {
		t.Fatalf("unexpected cooldown: %s", got.CooldownUntil.UTC().Format(time.RFC3339))
	}

	if err := repo.ClearCooldown(ctx, account.ID); err != nil {
		t.Fatalf("ClearCooldown failed: %v", err)
	}

	cleared, err := repo.Get(ctx, account.ID)
	if err != nil {
		t.Fatalf("Get after clear failed: %v", err)
	}
	if cleared.CooldownUntil != nil {
		t.Fatal("expected cooldown to be cleared")
	}
}

func TestAccountRepository_GetNextAvailable(t *testing.T) {
	db := setupTestDB(t)
	defer db.Close()

	repo := NewAccountRepository(db)
	ctx := context.Background()

	future := time.Now().UTC().Add(10 * time.Minute)

	account1 := &models.Account{
		Provider:      models.ProviderOpenAI,
		ProfileName:   "a",
		CredentialRef: "env:OPENAI_API_KEY",
		IsActive:      true,
		CooldownUntil: &future,
	}
	account2 := &models.Account{
		Provider:      models.ProviderOpenAI,
		ProfileName:   "b",
		CredentialRef: "env:OPENAI_API_KEY",
		IsActive:      true,
	}
	account3 := &models.Account{
		Provider:      models.ProviderOpenAI,
		ProfileName:   "c",
		CredentialRef: "env:OPENAI_API_KEY",
		IsActive:      false,
	}
	account4 := &models.Account{
		Provider:      models.ProviderAnthropic,
		ProfileName:   "x",
		CredentialRef: "env:ANTHROPIC_API_KEY",
		IsActive:      true,
	}

	for _, acct := range []*models.Account{account1, account2, account3, account4} {
		if err := repo.Create(ctx, acct); err != nil {
			t.Fatalf("Create failed: %v", err)
		}
	}

	got, err := repo.GetNextAvailable(ctx, models.ProviderOpenAI)
	if err != nil {
		t.Fatalf("GetNextAvailable failed: %v", err)
	}
	if got.ProfileName != "b" {
		t.Fatalf("expected profile b, got %s", got.ProfileName)
	}

	if err := repo.SetCooldown(ctx, account2.ID, future); err != nil {
		t.Fatalf("SetCooldown failed: %v", err)
	}

	if _, err := repo.GetNextAvailable(ctx, models.ProviderOpenAI); !errors.Is(err, ErrAccountNotFound) {
		t.Fatalf("expected ErrAccountNotFound, got %v", err)
	}

	if _, err := repo.GetNextAvailable(ctx, models.Provider("")); !errors.Is(err, models.ErrInvalidProvider) {
		t.Fatalf("expected ErrInvalidProvider, got %v", err)
	}
}
