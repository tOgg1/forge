package parity

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"strings"

	"github.com/tOgg1/forge/internal/db"
)

// SchemaFingerprint captures a deterministic schema dump plus digest.
type SchemaFingerprint struct {
	Dump   string
	SHA256 string
}

// ComputeSchemaFingerprint migrates an in-memory DB and fingerprints sqlite schema objects.
func ComputeSchemaFingerprint(ctx context.Context) (SchemaFingerprint, error) {
	fingerprint := SchemaFingerprint{}

	database, err := db.OpenInMemory()
	if err != nil {
		return fingerprint, fmt.Errorf("open in-memory db: %w", err)
	}
	defer database.Close()

	if _, err := database.MigrateUp(ctx); err != nil {
		return fingerprint, fmt.Errorf("migrate up: %w", err)
	}

	rows, err := database.QueryContext(ctx, `
		SELECT type, name, tbl_name, COALESCE(sql, '')
		FROM sqlite_master
		WHERE type IN ('table', 'index', 'trigger', 'view')
		  AND name NOT LIKE 'sqlite_%'
		ORDER BY type, name
	`)
	if err != nil {
		return fingerprint, fmt.Errorf("query sqlite_master: %w", err)
	}
	defer rows.Close()

	var b strings.Builder
	for rows.Next() {
		var objType string
		var name string
		var tableName string
		var sqlText string
		if err := rows.Scan(&objType, &name, &tableName, &sqlText); err != nil {
			return fingerprint, fmt.Errorf("scan sqlite_master row: %w", err)
		}
		fmt.Fprintf(&b, "%s|%s|%s|%s\n", objType, name, tableName, canonicalSQL(sqlText))
	}
	if err := rows.Err(); err != nil {
		return fingerprint, fmt.Errorf("iterate sqlite_master rows: %w", err)
	}

	dump := b.String()
	sum := sha256.Sum256([]byte(dump))

	fingerprint.Dump = dump
	fingerprint.SHA256 = hex.EncodeToString(sum[:])
	return fingerprint, nil
}

func canonicalSQL(in string) string {
	fields := strings.Fields(strings.ReplaceAll(in, "\r\n", "\n"))
	return strings.Join(fields, " ")
}
