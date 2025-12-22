package testutil

import (
	"os"
	"path/filepath"
	"runtime"
	"testing"

	"github.com/stretchr/testify/require"
)

// FixturePath returns an absolute path to a shared fixture in this package.
func FixturePath(t *testing.T, elements ...string) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	require.True(t, ok, "failed to resolve fixture path")
	base := filepath.Join(filepath.Dir(file), "testdata")
	parts := append([]string{base}, elements...)
	return filepath.Join(parts...)
}

// ReadFixture loads a fixture from the shared testdata directory.
func ReadFixture(t *testing.T, elements ...string) []byte {
	t.Helper()
	path := FixturePath(t, elements...)
	data, err := os.ReadFile(path)
	require.NoError(t, err, "failed to read fixture %s", path)
	return data
}
