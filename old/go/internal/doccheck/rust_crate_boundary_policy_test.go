package doccheck

import (
	"encoding/json"
	"path/filepath"
	"strings"
	"testing"
)

func TestRustCrateBoundaryPolicyCoversActiveWorkspaceCrates(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	cargoWorkspace := readFile(t, filepath.Join(root, "Cargo.toml"))
	boundariesDoc := readFile(t, filepath.Join(root, "docs/rust-crate-boundary-policy.md"))
	policyJSON := readFile(t, filepath.Join(root, "docs/rust-crate-boundaries.json"))

	workspaceCrates := workspaceCrateNames(cargoWorkspace)
	if len(workspaceCrates) == 0 {
		t.Fatalf("no rust workspace crates discovered in Cargo.toml")
	}

	var policy map[string]int
	if err := json.Unmarshal([]byte(policyJSON), &policy); err != nil {
		t.Fatalf("parse docs/rust-crate-boundaries.json: %v", err)
	}

	for _, crate := range workspaceCrates {
		if _, ok := policy[crate]; !ok {
			t.Fatalf("boundary policy missing active workspace crate %q", crate)
		}
	}

	if _, ok := policy["forge-rpc"]; !ok {
		t.Fatalf("boundary policy missing forge-rpc")
	}

	if !strings.Contains(boundariesDoc, "active workspace crate") {
		t.Fatalf("boundary policy doc missing active workspace crate coverage rule")
	}
}

func workspaceCrateNames(cargoWorkspace string) []string {
	seen := map[string]struct{}{}
	for _, line := range strings.Split(cargoWorkspace, "\n") {
		trimmed := strings.TrimSpace(line)
		if !strings.HasPrefix(trimmed, "\"crates/") {
			continue
		}
		entry := strings.Trim(trimmed, "\",")
		parts := strings.Split(entry, "/")
		if len(parts) != 2 || parts[0] != "crates" || parts[1] == "" {
			continue
		}
		seen[parts[1]] = struct{}{}
	}

	out := make([]string, 0, len(seen))
	for crate := range seen {
		out = append(out, crate)
	}
	return out
}
