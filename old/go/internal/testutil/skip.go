package testutil

import (
	"os"
	"testing"
)

// SkipIfNoNetwork skips the test if FORGE_TEST_SKIP_NETWORK is set.
// Use this for tests that require TCP/network connectivity which may
// not be available in sandboxed agent environments.
//
// Note: Some packages (like forged) have import cycles that prevent using
// this helper. In those cases, define a local skipIfNoNetwork function.
func SkipIfNoNetwork(t *testing.T) {
	t.Helper()
	if os.Getenv("FORGE_TEST_SKIP_NETWORK") != "" {
		t.Skip("skipping network test: FORGE_TEST_SKIP_NETWORK is set")
	}
}
