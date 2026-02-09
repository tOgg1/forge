package parity

import (
	"path/filepath"
	"testing"
)

func TestCLIGateSendInjectOracleBaseline(t *testing.T) {
	t.Parallel()

	expected := filepath.Join("testdata", "oracle", "expected", "forge", "send-inject")
	actual := filepath.Join("testdata", "oracle", "actual", "forge", "send-inject")

	report, err := CompareTrees(expected, actual)
	if err != nil {
		t.Fatalf("compare send/inject oracle trees: %v", err)
	}
	if report.HasDrift() {
		t.Fatalf("cli send/inject gate drift detected: %+v", report)
	}
}
