// Package db provides SQLite database access for Swarm.
package db

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"time"

	"github.com/google/uuid"
	"github.com/opencode-ai/swarm/internal/models"
)

// Account repository errors.
var (
	ErrAccountNotFound      = errors.New("account not found")
	ErrAccountAlreadyExists = errors.New("account with this provider and profile already exists")
)

// AccountRepository handles account persistence.
type AccountRepository struct {
	db *DB
}

// NewAccountRepository creates a new AccountRepository.
func NewAccountRepository(db *DB) *AccountRepository {
	return &AccountRepository{db: db}
}

// Create adds a new account to the database.
func (r *AccountRepository) Create(ctx context.Context, account *models.Account) error {
	if err := account.Validate(); err != nil {
		return fmt.Errorf("invalid account: %w", err)
	}

	if account.ID == "" {
		account.ID = uuid.New().String()
	}

	now := time.Now().UTC()
	account.CreatedAt = now
	account.UpdatedAt = now

	var usageStatsJSON *string
	if account.UsageStats != nil {
		data, err := json.Marshal(account.UsageStats)
		if err != nil {
			return fmt.Errorf("failed to marshal usage stats: %w", err)
		}
		s := string(data)
		usageStatsJSON = &s
	}

	var cooldownUntil *string
	if account.CooldownUntil != nil {
		s := account.CooldownUntil.Format(time.RFC3339)
		cooldownUntil = &s
	}

	isActive := 0
	if account.IsActive {
		isActive = 1
	}

	_, err := r.db.ExecContext(ctx, `
		INSERT INTO accounts (
			id, provider, profile_name, credential_ref, is_active,
			cooldown_until, usage_stats_json, created_at, updated_at
		) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
	`,
		account.ID,
		string(account.Provider),
		account.ProfileName,
		account.CredentialRef,
		isActive,
		cooldownUntil,
		usageStatsJSON,
		account.CreatedAt.Format(time.RFC3339),
		account.UpdatedAt.Format(time.RFC3339),
	)

	if err != nil {
		if isUniqueConstraintError(err) {
			return ErrAccountAlreadyExists
		}
		return fmt.Errorf("failed to insert account: %w", err)
	}

	return nil
}

// List retrieves all accounts, optionally filtered by provider.
func (r *AccountRepository) List(ctx context.Context, provider *models.Provider) ([]*models.Account, error) {
	var rows *sql.Rows
	var err error

	if provider != nil {
		rows, err = r.db.QueryContext(ctx, `
			SELECT 
				id, provider, profile_name, credential_ref, is_active,
				cooldown_until, usage_stats_json, created_at, updated_at
			FROM accounts
			WHERE provider = ?
			ORDER BY profile_name
		`, string(*provider))
	} else {
		rows, err = r.db.QueryContext(ctx, `
			SELECT 
				id, provider, profile_name, credential_ref, is_active,
				cooldown_until, usage_stats_json, created_at, updated_at
			FROM accounts
			ORDER BY provider, profile_name
		`)
	}

	if err != nil {
		return nil, fmt.Errorf("failed to query accounts: %w", err)
	}
	defer rows.Close()

	var accounts []*models.Account
	for rows.Next() {
		account, err := r.scanAccountFromRows(rows)
		if err != nil {
			return nil, err
		}
		accounts = append(accounts, account)
	}

	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("error iterating accounts: %w", err)
	}

	return accounts, nil
}

// Get retrieves an account by ID.
func (r *AccountRepository) Get(ctx context.Context, id string) (*models.Account, error) {
	row := r.db.QueryRowContext(ctx, `
		SELECT
			id, provider, profile_name, credential_ref, is_active,
			cooldown_until, usage_stats_json, created_at, updated_at
		FROM accounts
		WHERE id = ?
	`, id)

	return r.scanAccount(row)
}

// Update modifies an existing account.
func (r *AccountRepository) Update(ctx context.Context, account *models.Account) error {
	if err := account.Validate(); err != nil {
		return fmt.Errorf("invalid account: %w", err)
	}

	account.UpdatedAt = time.Now().UTC()

	var usageStatsJSON *string
	if account.UsageStats != nil {
		data, err := json.Marshal(account.UsageStats)
		if err != nil {
			return fmt.Errorf("failed to marshal usage stats: %w", err)
		}
		s := string(data)
		usageStatsJSON = &s
	}

	var cooldownUntil *string
	if account.CooldownUntil != nil {
		s := account.CooldownUntil.UTC().Format(time.RFC3339)
		cooldownUntil = &s
	}

	result, err := r.db.ExecContext(ctx, `
		UPDATE accounts SET
			provider = ?,
			profile_name = ?,
			credential_ref = ?,
			is_active = ?,
			cooldown_until = ?,
			usage_stats_json = ?,
			updated_at = ?
		WHERE id = ?
	`,
		string(account.Provider),
		account.ProfileName,
		account.CredentialRef,
		boolToInt(account.IsActive),
		cooldownUntil,
		usageStatsJSON,
		account.UpdatedAt.Format(time.RFC3339),
		account.ID,
	)

	if err != nil {
		if isUniqueConstraintError(err) {
			return ErrAccountAlreadyExists
		}
		return fmt.Errorf("failed to update account: %w", err)
	}

	rowsAffected, err := result.RowsAffected()
	if err != nil {
		return fmt.Errorf("failed to get rows affected: %w", err)
	}
	if rowsAffected == 0 {
		return ErrAccountNotFound
	}

	return nil
}

// Delete removes an account by ID.
func (r *AccountRepository) Delete(ctx context.Context, id string) error {
	result, err := r.db.ExecContext(ctx, "DELETE FROM accounts WHERE id = ?", id)
	if err != nil {
		return fmt.Errorf("failed to delete account: %w", err)
	}

	rowsAffected, err := result.RowsAffected()
	if err != nil {
		return fmt.Errorf("failed to get rows affected: %w", err)
	}
	if rowsAffected == 0 {
		return ErrAccountNotFound
	}

	return nil
}

// SetCooldown updates cooldown_until for an account.
func (r *AccountRepository) SetCooldown(ctx context.Context, id string, until time.Time) error {
	if until.IsZero() {
		return fmt.Errorf("cooldown time is required")
	}

	now := time.Now().UTC()
	cooldownUntil := until.UTC().Format(time.RFC3339)

	result, err := r.db.ExecContext(ctx, `
		UPDATE accounts SET
			cooldown_until = ?,
			updated_at = ?
		WHERE id = ?
	`, cooldownUntil, now.Format(time.RFC3339), id)

	if err != nil {
		return fmt.Errorf("failed to set cooldown: %w", err)
	}

	rowsAffected, err := result.RowsAffected()
	if err != nil {
		return fmt.Errorf("failed to get rows affected: %w", err)
	}
	if rowsAffected == 0 {
		return ErrAccountNotFound
	}

	return nil
}

// ClearCooldown removes cooldown from an account.
func (r *AccountRepository) ClearCooldown(ctx context.Context, id string) error {
	now := time.Now().UTC()

	result, err := r.db.ExecContext(ctx, `
		UPDATE accounts SET
			cooldown_until = NULL,
			updated_at = ?
		WHERE id = ?
	`, now.Format(time.RFC3339), id)

	if err != nil {
		return fmt.Errorf("failed to clear cooldown: %w", err)
	}

	rowsAffected, err := result.RowsAffected()
	if err != nil {
		return fmt.Errorf("failed to get rows affected: %w", err)
	}
	if rowsAffected == 0 {
		return ErrAccountNotFound
	}

	return nil
}

// GetNextAvailable returns the next available account for a provider.
func (r *AccountRepository) GetNextAvailable(ctx context.Context, provider models.Provider) (*models.Account, error) {
	if provider == "" {
		return nil, models.ErrInvalidProvider
	}

	now := time.Now().UTC().Format(time.RFC3339)
	row := r.db.QueryRowContext(ctx, `
		SELECT
			id, provider, profile_name, credential_ref, is_active,
			cooldown_until, usage_stats_json, created_at, updated_at
		FROM accounts
		WHERE provider = ?
			AND is_active = 1
			AND (cooldown_until IS NULL OR cooldown_until <= ?)
		ORDER BY profile_name
		LIMIT 1
	`, string(provider), now)

	return r.scanAccount(row)
}

func (r *AccountRepository) scanAccount(row *sql.Row) (*models.Account, error) {
	var account models.Account
	var provider string
	var isActive int
	var cooldownUntil sql.NullString
	var usageStatsJSON sql.NullString
	var createdAt, updatedAt string

	err := row.Scan(
		&account.ID,
		&provider,
		&account.ProfileName,
		&account.CredentialRef,
		&isActive,
		&cooldownUntil,
		&usageStatsJSON,
		&createdAt,
		&updatedAt,
	)
	if err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, ErrAccountNotFound
		}
		return nil, fmt.Errorf("failed to scan account: %w", err)
	}

	if err := r.populateAccountFields(&account, provider, isActive, cooldownUntil, usageStatsJSON, createdAt, updatedAt); err != nil {
		return nil, err
	}

	return &account, nil
}

func (r *AccountRepository) scanAccountFromRows(rows *sql.Rows) (*models.Account, error) {
	var account models.Account
	var provider string
	var isActive int
	var cooldownUntil sql.NullString
	var usageStatsJSON sql.NullString
	var createdAt, updatedAt string

	if err := rows.Scan(
		&account.ID,
		&provider,
		&account.ProfileName,
		&account.CredentialRef,
		&isActive,
		&cooldownUntil,
		&usageStatsJSON,
		&createdAt,
		&updatedAt,
	); err != nil {
		return nil, fmt.Errorf("failed to scan account: %w", err)
	}

	if err := r.populateAccountFields(&account, provider, isActive, cooldownUntil, usageStatsJSON, createdAt, updatedAt); err != nil {
		return nil, err
	}

	return &account, nil
}

func (r *AccountRepository) populateAccountFields(
	account *models.Account,
	provider string,
	isActive int,
	cooldownUntil sql.NullString,
	usageStatsJSON sql.NullString,
	createdAt string,
	updatedAt string,
) error {
	account.Provider = models.Provider(provider)
	account.IsActive = isActive != 0

	if cooldownUntil.Valid && cooldownUntil.String != "" {
		parsed, err := time.Parse(time.RFC3339, cooldownUntil.String)
		if err != nil {
			return fmt.Errorf("failed to parse cooldown_until: %w", err)
		}
		account.CooldownUntil = &parsed
	}

	if usageStatsJSON.Valid && usageStatsJSON.String != "" {
		var stats models.UsageStats
		if err := json.Unmarshal([]byte(usageStatsJSON.String), &stats); err != nil {
			r.db.logger.Warn().Err(err).Str("account_id", account.ID).Msg("failed to parse usage stats")
		} else {
			account.UsageStats = &stats
		}
	}

	createdParsed, err := parseAccountTime(createdAt)
	if err != nil {
		return fmt.Errorf("failed to parse created_at: %w", err)
	}
	updatedParsed, err := parseAccountTime(updatedAt)
	if err != nil {
		return fmt.Errorf("failed to parse updated_at: %w", err)
	}
	account.CreatedAt = createdParsed
	account.UpdatedAt = updatedParsed

	return nil
}

func parseAccountTime(value string) (time.Time, error) {
	if value == "" {
		return time.Time{}, fmt.Errorf("empty time value")
	}

	if parsed, err := time.Parse(time.RFC3339, value); err == nil {
		return parsed, nil
	}

	if parsed, err := time.Parse("2006-01-02 15:04:05", value); err == nil {
		return parsed, nil
	}

	return time.Time{}, fmt.Errorf("unsupported time format: %s", value)
}
