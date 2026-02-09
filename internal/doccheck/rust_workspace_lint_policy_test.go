package doccheck

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestRustWorkspaceLintPolicyPinned(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	policy := readFile(t, filepath.Join(root, "docs/rust-workspace-lint-policy.md"))
	scriptPath := filepath.Join(root, "scripts/rust-quality-check.sh")
	script := readFile(t, scriptPath)
	cargoWorkspace := readFile(t, filepath.Join(root, "rust/Cargo.toml"))
	rustfmt := readFile(t, filepath.Join(root, "rust/rustfmt.toml"))
	clippy := readFile(t, filepath.Join(root, "rust/clippy.toml"))

	for _, want := range []string{
		"`rust/rustfmt.toml`",
		"`rust/clippy.toml`",
		"`scripts/rust-quality-check.sh`",
		"`-D warnings`",
	} {
		if !strings.Contains(policy, want) {
			t.Fatalf("rust workspace lint policy missing %q", want)
		}
	}

	for _, want := range []string{
		"cargo fmt --all --check",
		"cargo clippy --workspace --all-targets -- -D warnings",
		"cargo test --workspace",
	} {
		if !strings.Contains(script, want) {
			t.Fatalf("rust quality script drift: missing %q", want)
		}
	}

	info, err := os.Stat(scriptPath)
	if err != nil {
		t.Fatalf("stat rust quality script: %v", err)
	}
	if info.Mode()&0o111 == 0 {
		t.Fatalf("scripts/rust-quality-check.sh must be executable")
	}

	for _, want := range []string{
		`"crates/fmail-core"`,
		`"crates/fmail-tui"`,
		`"crates/forge-core"`,
		`"crates/forge-parity-stub"`,
		`"crates/forge-tui"`,
	} {
		if !strings.Contains(cargoWorkspace, want) {
			t.Fatalf("workspace members missing %s", want)
		}
	}

	if strings.TrimSpace(rustfmt) == "" {
		t.Fatalf("rustfmt config drift: file is empty")
	}
	if !strings.Contains(rustfmt, `newline_style = "Unix"`) {
		t.Fatalf("rustfmt config drift: newline style policy missing")
	}
	if !strings.Contains(clippy, "warn-on-all-wildcard-imports = true") {
		t.Fatalf("clippy config drift: wildcard import warning policy missing")
	}

	for _, crate := range []string{
		"fmail-core",
		"fmail-tui",
		"forge-core",
		"forge-parity-stub",
		"forge-tui",
	} {
		manifest := readFile(t, filepath.Join(root, "rust/crates", crate, "Cargo.toml"))
		for _, want := range []string{
			"edition.workspace = true",
			"license.workspace = true",
			"publish.workspace = true",
		} {
			if !strings.Contains(manifest, want) {
				t.Fatalf("%s Cargo.toml missing %q", crate, want)
			}
		}
	}
}
