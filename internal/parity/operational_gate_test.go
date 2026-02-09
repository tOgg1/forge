package parity

import (
	"path/filepath"
	"testing"
)

func TestOperationalGateBaseline(t *testing.T) {
	t.Parallel()

	expected := filepath.Join("testdata", "oracle", "expected", "forge", "operational")
	actual := filepath.Join("testdata", "oracle", "actual", "forge", "operational")

	report, err := CompareTrees(expected, actual)
	if err != nil {
		t.Fatalf("compare operational oracle trees: %v", err)
	}
	if report.HasDrift() {
		t.Fatalf("operational gate drift detected: %+v", report)
	}
}
