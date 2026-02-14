package doccheck

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestRustBaselineSnapshotScriptSupportsAbsoluteOutDir(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	script := readFile(t, filepath.Join(root, "scripts", "rust-baseline-snapshot.sh"))

	for _, want := range []string{
		"if [[ \"$out_dir\" = /* ]]",
		"out_dir_abs=\"$out_dir\"",
		"out_dir_abs=\"$repo_root/$out_dir\"",
		"$out_dir_abs/forge-help.txt",
	} {
		if !strings.Contains(script, want) {
			t.Fatalf("rust baseline snapshot script missing absolute out-dir contract %q", want)
		}
	}

	if strings.Contains(script, "$repo_root/$out_dir/forge-help.txt") {
		t.Fatal("rust baseline snapshot script still hard-codes repo_root prefix for forge-help output")
	}
}
