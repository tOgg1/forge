// Package workspace provides helpers for workspace lifecycle management.
package workspace

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"strconv"
	"strings"

	"github.com/opencode-ai/swarm/internal/models"
)

// ValidateRepoPath checks that a repository path exists and is a directory.
func ValidateRepoPath(repoPath string) error {
	if repoPath == "" {
		return models.ErrInvalidRepoPath
	}

	info, err := os.Stat(repoPath)
	if err != nil {
		if os.IsNotExist(err) {
			return fmt.Errorf("repository path does not exist: %s", repoPath)
		}
		return fmt.Errorf("failed to stat repository path %s: %w", repoPath, err)
	}

	if !info.IsDir() {
		return fmt.Errorf("repository path is not a directory: %s", repoPath)
	}

	return nil
}

// DetectGitInfo inspects a repo path and returns git metadata if available.
func DetectGitInfo(repoPath string) (*models.GitInfo, error) {
	if err := ValidateRepoPath(repoPath); err != nil {
		return nil, err
	}

	isRepo, err := isGitRepo(repoPath)
	if err != nil {
		return nil, err
	}

	info := &models.GitInfo{IsRepo: isRepo}
	if !isRepo {
		return info, nil
	}

	if branch, err := runGitTrim(repoPath, "rev-parse", "--abbrev-ref", "HEAD"); err == nil {
		info.Branch = branch
	}

	if lastCommit, err := runGitTrim(repoPath, "rev-parse", "HEAD"); err == nil {
		info.LastCommit = lastCommit
	}

	if remote, err := runGitTrim(repoPath, "config", "--get", "remote.origin.url"); err == nil {
		info.RemoteURL = remote
	}

	if status, err := runGitTrim(repoPath, "status", "--porcelain"); err == nil {
		info.IsDirty = status != ""
	}

	if counts, err := runGitTrim(repoPath, "rev-list", "--left-right", "--count", "HEAD...@{upstream}"); err == nil {
		parts := strings.Fields(counts)
		if len(parts) == 2 {
			if ahead, err := strconv.Atoi(parts[0]); err == nil {
				info.Ahead = ahead
			}
			if behind, err := strconv.Atoi(parts[1]); err == nil {
				info.Behind = behind
			}
		}
	}

	return info, nil
}

func isGitRepo(repoPath string) (bool, error) {
	out, _, err := runGit(repoPath, "rev-parse", "--is-inside-work-tree")
	if err != nil {
		if errors.Is(err, exec.ErrNotFound) {
			return false, err
		}
		var exitErr *exec.ExitError
		if errors.As(err, &exitErr) {
			return false, nil
		}
		return false, err
	}

	return strings.TrimSpace(out) == "true", nil
}

func runGitTrim(repoPath string, args ...string) (string, error) {
	stdout, _, err := runGit(repoPath, args...)
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(stdout), nil
}

func runGit(repoPath string, args ...string) (string, string, error) {
	cmd := exec.Command("git", args...)
	cmd.Dir = repoPath

	var stdout bytes.Buffer
	var stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	err := cmd.Run()
	return stdout.String(), stderr.String(), err
}
