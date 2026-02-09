package doccheck

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestForgeHelperFamilySnapshotsCurrent(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	forgeBin := buildGoBinary(t, root, "./cmd/forge")

	cases := []struct {
		name         string
		subcommand   string
		snapshotPath string
		required     []string
	}{
		{
			name:         "profile",
			subcommand:   "profile",
			snapshotPath: "docs/forge/help/forge-profile-help.txt",
			required:     []string{"add", "edit", "ls", "rm"},
		},
		{
			name:         "pool",
			subcommand:   "pool",
			snapshotPath: "docs/forge/help/forge-pool-help.txt",
			required:     []string{"create", "add", "ls", "show"},
		},
		{
			name:         "prompt",
			subcommand:   "prompt",
			snapshotPath: "docs/forge/help/forge-prompt-help.txt",
			required:     []string{"add", "edit", "ls", "set-default"},
		},
		{
			name:         "template",
			subcommand:   "template",
			snapshotPath: "docs/forge/help/forge-template-help.txt",
			required:     []string{"add", "edit", "run", "show", "delete"},
		},
		{
			name:         "seq",
			subcommand:   "seq",
			snapshotPath: "docs/forge/help/forge-seq-help.txt",
			required:     []string{"add", "edit", "run", "show", "delete"},
		},
	}

	for _, tc := range cases {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			snapshot := readFile(t, filepath.Join(root, tc.snapshotPath))
			out := runBinary(t, root, forgeBin, tc.subcommand, "--help")
			if out.exitCode != 0 {
				t.Fatalf("forge %s --help exit code = %d, want 0", tc.subcommand, out.exitCode)
			}

			normalized := normalize(out.stdout)
			if normalized != normalize(snapshot) {
				t.Fatalf("forge %s help snapshot drift; regenerate %s", tc.subcommand, tc.snapshotPath)
			}

			for _, token := range tc.required {
				if !strings.Contains(normalized, token) {
					t.Fatalf("forge %s help missing required token %q", tc.subcommand, token)
				}
			}
		})
	}
}
