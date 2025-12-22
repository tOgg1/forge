// Package cli provides account management CLI commands.
package cli

import (
	"context"
	"fmt"
	"os"
	"strings"
	"text/tabwriter"
	"time"

	"github.com/opencode-ai/swarm/internal/db"
	"github.com/opencode-ai/swarm/internal/models"
	"github.com/spf13/cobra"
)

var (
	accountsListProvider string
)

func init() {
	rootCmd.AddCommand(accountsCmd)
	accountsCmd.AddCommand(accountsListCmd)

	accountsListCmd.Flags().StringVar(&accountsListProvider, "provider", "", "filter by provider (anthropic, openai, google, custom)")
}

var accountsCmd = &cobra.Command{
	Use:   "accounts",
	Short: "Manage accounts",
	Long:  "Manage provider accounts and profiles used by agents.",
}

var accountsListCmd = &cobra.Command{
	Use:   "list",
	Short: "List accounts",
	Long:  "List available provider accounts and their status.",
	RunE: func(cmd *cobra.Command, args []string) error {
		ctx := context.Background()

		database, err := openDatabase()
		if err != nil {
			return err
		}
		defer database.Close()

		repo := db.NewAccountRepository(database)

		var provider *models.Provider
		if strings.TrimSpace(accountsListProvider) != "" {
			parsed, err := parseProvider(accountsListProvider)
			if err != nil {
				return err
			}
			provider = &parsed
		}

		accounts, err := repo.List(ctx, provider)
		if err != nil {
			return fmt.Errorf("failed to list accounts: %w", err)
		}

		if IsJSONOutput() || IsJSONLOutput() {
			return WriteOutput(os.Stdout, accounts)
		}

		if len(accounts) == 0 {
			fmt.Fprintln(os.Stdout, "No accounts found.")
			return nil
		}

		writer := tabwriter.NewWriter(os.Stdout, 0, 8, 2, ' ', 0)
		fmt.Fprintln(writer, "PROVIDER\tPROFILE\tSTATUS\tCOOLDOWN")
		for _, account := range accounts {
			fmt.Fprintf(
				writer,
				"%s\t%s\t%s\t%s\n",
				account.Provider,
				account.ProfileName,
				formatAccountStatus(account),
				formatAccountCooldown(account),
			)
		}
		return writer.Flush()
	},
}

func parseProvider(value string) (models.Provider, error) {
	switch strings.ToLower(strings.TrimSpace(value)) {
	case string(models.ProviderAnthropic):
		return models.ProviderAnthropic, nil
	case string(models.ProviderOpenAI):
		return models.ProviderOpenAI, nil
	case string(models.ProviderGoogle):
		return models.ProviderGoogle, nil
	case string(models.ProviderCustom):
		return models.ProviderCustom, nil
	default:
		return "", fmt.Errorf("invalid provider: %s", value)
	}
}

func formatAccountStatus(account *models.Account) string {
	if !account.IsActive {
		return "inactive"
	}
	if account.IsOnCooldown() {
		return "cooldown"
	}
	return "active"
}

func formatAccountCooldown(account *models.Account) string {
	if account.CooldownUntil == nil {
		return "-"
	}
	if !account.IsOnCooldown() {
		return "expired"
	}
	remaining := account.CooldownRemaining()
	if remaining < time.Second {
		return "<1s"
	}
	return remaining.Round(time.Second).String()
}
