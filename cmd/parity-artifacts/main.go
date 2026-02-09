package main

import (
	"flag"
	"fmt"
	"os"

	"github.com/tOgg1/forge/internal/parity"
)

func main() {
	var expected string
	var actual string
	var out string

	flag.StringVar(&expected, "expected", "", "expected output directory")
	flag.StringVar(&actual, "actual", "", "actual output directory")
	flag.StringVar(&out, "out", "parity-artifacts", "artifact output directory")
	flag.Parse()

	if expected == "" || actual == "" {
		fmt.Fprintln(os.Stderr, "usage: parity-artifacts --expected <dir> --actual <dir> [--out <dir>]")
		os.Exit(2)
	}

	report, err := parity.WriteDiffArtifacts(expected, actual, out)
	if err != nil {
		fmt.Fprintf(os.Stderr, "write artifacts: %v\n", err)
		os.Exit(1)
	}

	fmt.Println(parity.DriftSummary(report))
	if report.HasDrift() {
		os.Exit(1)
	}
}
