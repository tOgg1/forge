package models

import (
	"time"
)

// Provider identifies an AI provider.
type Provider string

const (
	ProviderAnthropic Provider = "anthropic"
	ProviderOpenAI    Provider = "openai"
	ProviderGoogle    Provider = "google"
	ProviderCustom    Provider = "custom"
)

// Account represents a provider account/profile for authentication.
type Account struct {
	// ID is the unique identifier for the account.
	ID string `json:"id"`

	// Provider identifies the AI provider.
	Provider Provider `json:"provider"`

	// ProfileName is the human-friendly name for this account.
	ProfileName string `json:"profile_name"`

	// CredentialRef is a reference to the credential (env var, file path, or vault key).
	CredentialRef string `json:"credential_ref"`

	// IsActive indicates if this account is enabled for use.
	IsActive bool `json:"is_active"`

	// CooldownUntil is when the cooldown expires (if rate-limited).
	CooldownUntil *time.Time `json:"cooldown_until,omitempty"`

	// UsageStats contains usage information for this account.
	UsageStats *UsageStats `json:"usage_stats,omitempty"`

	// CreatedAt is when the account was added.
	CreatedAt time.Time `json:"created_at"`

	// UpdatedAt is when the account was last updated.
	UpdatedAt time.Time `json:"updated_at"`
}

// UsageStats contains usage metrics for an account.
type UsageStats struct {
	// TotalTokens is the total tokens used.
	TotalTokens int64 `json:"total_tokens"`

	// TotalCost is the estimated total cost (in cents).
	TotalCostCents int64 `json:"total_cost_cents"`

	// LastUsed is when the account was last used.
	LastUsed *time.Time `json:"last_used,omitempty"`

	// RequestCount is the number of API requests made.
	RequestCount int64 `json:"request_count"`

	// RateLimitCount is how many times this account hit rate limits.
	RateLimitCount int64 `json:"rate_limit_count"`
}

// Validate checks if the account configuration is valid.
func (a *Account) Validate() error {
	validation := &ValidationErrors{}
	if a.Provider == "" {
		validation.Add("provider", ErrInvalidProvider)
	}
	if a.ProfileName == "" {
		validation.Add("profile_name", ErrInvalidProfileName)
	}
	return validation.Err()
}

// IsOnCooldown returns true if the account is currently on cooldown.
func (a *Account) IsOnCooldown() bool {
	if a.CooldownUntil == nil {
		return false
	}
	return time.Now().Before(*a.CooldownUntil)
}

// CooldownRemaining returns the remaining cooldown duration.
func (a *Account) CooldownRemaining() time.Duration {
	if !a.IsOnCooldown() {
		return 0
	}
	return time.Until(*a.CooldownUntil)
}

// IsAvailable returns true if the account can be used.
func (a *Account) IsAvailable() bool {
	return a.IsActive && !a.IsOnCooldown()
}
