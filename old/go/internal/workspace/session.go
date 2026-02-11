// Package workspace provides helpers for workspace lifecycle management.
package workspace

import (
	"crypto/sha1"
	"encoding/hex"
	"fmt"
	"path/filepath"
	"strings"
)

// GenerateTmuxSessionName builds a tmux-safe session name for a repo path.
func GenerateTmuxSessionName(prefix, repoPath string) (string, error) {
	if err := ValidateRepoPath(repoPath); err != nil {
		return "", err
	}

	base := filepath.Base(repoPath)
	slug := sanitizeTmuxName(base)
	if slug == "" {
		slug = "workspace"
	}

	hash := shortHash(repoPath)
	prefix = sanitizeTmuxName(prefix)
	if prefix != "" {
		return fmt.Sprintf("%s-%s-%s", prefix, slug, hash), nil
	}
	return fmt.Sprintf("%s-%s", slug, hash), nil
}

func sanitizeTmuxName(value string) string {
	value = strings.ToLower(value)
	var b strings.Builder
	lastDash := false

	for _, r := range value {
		if (r >= 'a' && r <= 'z') || (r >= '0' && r <= '9') {
			b.WriteRune(r)
			lastDash = false
			continue
		}

		if !lastDash {
			b.WriteByte('-')
			lastDash = true
		}
	}

	return strings.Trim(b.String(), "-")
}

func shortHash(value string) string {
	sum := sha1.Sum([]byte(value))
	return hex.EncodeToString(sum[:])[:8]
}
