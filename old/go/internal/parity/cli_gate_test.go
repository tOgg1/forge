package parity

import (
	"path/filepath"
	"testing"
)

func TestCLIGateRootOracleBaseline(t *testing.T) {
	t.Parallel()

	expected := filepath.Join("testdata", "oracle", "expected", "forge", "root")
	actual := filepath.Join("testdata", "oracle", "actual", "forge", "root")

	report, err := CompareTrees(expected, actual)
	if err != nil {
		t.Fatalf("compare root oracle trees: %v", err)
	}
	if report.HasDrift() {
		t.Fatalf("cli gate drift detected: %+v", report)
	}
}
