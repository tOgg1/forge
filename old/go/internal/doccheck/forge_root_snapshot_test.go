package doccheck

import (
	"bytes"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

func TestForgeRootSnapshotsCurrent(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)
	helpSnapshot := readFile(t, filepath.Join(root, "docs/forge/help/forge-root-help.txt"))
	versionSnapshot := readFile(t, filepath.Join(root, "docs/forge/help/forge-root-version.txt"))
	invalidStdoutSnapshot := readFile(t, filepath.Join(root, "docs/forge/help/forge-root-invalid-flag.stdout.txt"))
	invalidStderrSnapshot := readFile(t, filepath.Join(root, "docs/forge/help/forge-root-invalid-flag.stderr.txt"))
	invalidExitCodeSnapshot := readFile(t, filepath.Join(root, "docs/forge/help/forge-root-invalid-flag.exit-code.txt"))
	globalFlagsSnapshot := readFile(t, filepath.Join(root, "docs/forge/help/forge-root-global-flags.txt"))

	forgeBin := buildGoBinary(t, root, "./cmd/forge")

	help := runBinary(t, root, forgeBin, "--help")
	if help.exitCode != 0 {
		t.Fatalf("forge --help exit code = %d, want 0", help.exitCode)
	}
	if normalize(help.stdout) != normalize(helpSnapshot) {
		t.Fatalf("forge root help snapshot drift; regenerate docs/forge/help/forge-root-help.txt")
	}

	version := runBinary(t, root, forgeBin, "--version")
	if version.exitCode != 0 {
		t.Fatalf("forge --version exit code = %d, want 0", version.exitCode)
	}
	if normalize(version.stdout) != normalize(versionSnapshot) {
		t.Fatalf("forge root version snapshot drift; regenerate docs/forge/help/forge-root-version.txt")
	}

	invalid := runBinary(t, root, forgeBin, "--definitely-not-a-real-flag")
	if normalize(invalid.stdout) != normalize(invalidStdoutSnapshot) {
		t.Fatalf("forge invalid-flag stdout snapshot drift; regenerate docs/forge/help/forge-root-invalid-flag.stdout.txt")
	}
	if normalize(invalid.stderr) != normalize(invalidStderrSnapshot) {
		t.Fatalf("forge invalid-flag stderr snapshot drift; regenerate docs/forge/help/forge-root-invalid-flag.stderr.txt")
	}
	if strings.TrimSpace(strconv.Itoa(invalid.exitCode)) != strings.TrimSpace(invalidExitCodeSnapshot) {
		t.Fatalf("forge invalid-flag exit code drift: got %d, want %s", invalid.exitCode, strings.TrimSpace(invalidExitCodeSnapshot))
	}

	flags := strings.Join(extractLongFlags(help.stdout), "\n")
	if flags != "" {
		flags += "\n"
	}
	if normalize(flags) != normalize(globalFlagsSnapshot) {
		t.Fatalf("forge global flags snapshot drift; regenerate docs/forge/help/forge-root-global-flags.txt")
	}
}

type binaryResult struct {
	stdout   string
	stderr   string
	exitCode int
}

func buildGoBinary(t *testing.T, root, pkg string) string {
	t.Helper()
	bin := filepath.Join(t.TempDir(), filepath.Base(pkg))
	cmd := exec.Command("go", "build", "-o", bin, pkg)
	cmd.Dir = root
	cmd.Env = withoutToolchainOverrides(os.Environ())
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("go build %s: %v\n%s", pkg, err, string(out))
	}
	return bin
}

func runBinary(t *testing.T, root, bin string, args ...string) binaryResult {
	t.Helper()

	cmd := exec.Command(bin, args...)
	cmd.Dir = root
	cmd.Env = withoutToolchainOverrides(os.Environ())

	var stdout bytes.Buffer
	var stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	result := binaryResult{
		exitCode: 0,
	}
	if err := cmd.Run(); err != nil {
		exitErr, ok := err.(*exec.ExitError)
		if !ok {
			t.Fatalf("run %s %v: %v", bin, args, err)
		}
		result.exitCode = exitErr.ExitCode()
	}
	result.stdout = stdout.String()
	result.stderr = stderr.String()
	return result
}
