package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"time"

	"github.com/tOgg1/forge/internal/parity"
)

func main() {
	var scenarioPath string
	var fixtureDir string
	var goBinary string
	var rustBinary string
	var outPath string
	var timeout time.Duration

	flag.StringVar(&scenarioPath, "scenario", "", "path to lifecycle scenario json")
	flag.StringVar(&fixtureDir, "fixture", "", "fixture repository directory copied for each runtime")
	flag.StringVar(&goBinary, "go-bin", "", "path to Go forge binary")
	flag.StringVar(&rustBinary, "rust-bin", "", "path to Rust forge binary")
	flag.StringVar(&outPath, "out", "", "optional path to write JSON report")
	flag.DurationVar(&timeout, "timeout", 30*time.Second, "per-command timeout")
	flag.Parse()

	if scenarioPath == "" || goBinary == "" || rustBinary == "" {
		fmt.Fprintln(os.Stderr, "usage: parity-loop-lifecycle --scenario <file> --go-bin <path> --rust-bin <path> [--fixture <dir>] [--out <file>] [--timeout 30s]")
		os.Exit(2)
	}

	scenario, err := parity.LoadLifecycleScenario(scenarioPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "load scenario: %v\n", err)
		os.Exit(1)
	}

	report, err := parity.RunLoopLifecycleHarness(context.Background(), parity.LifecycleHarnessConfig{
		GoBinary:   goBinary,
		RustBinary: rustBinary,
		FixtureDir: fixtureDir,
		Scenario:   scenario,
		Timeout:    timeout,
	})
	if err != nil {
		fmt.Fprintf(os.Stderr, "run harness: %v\n", err)
		os.Exit(1)
	}

	if outPath != "" {
		if err := parity.WriteLifecycleHarnessReport(outPath, report); err != nil {
			fmt.Fprintf(os.Stderr, "write report: %v\n", err)
			os.Exit(1)
		}
	}

	fmt.Printf("scenario=%s steps=%d drift=%d\n", report.Scenario, len(report.Steps), report.DriftCount())
	for _, step := range report.Steps {
		if !step.HasDrift {
			continue
		}
		fmt.Printf("drift step=%s exit_match=%t stdout_equal=%t stderr_equal=%t\n",
			step.Name,
			step.ExitCodeMatch,
			step.Stdout.Equal,
			step.Stderr.Equal,
		)
	}

	if report.HasDrift() {
		os.Exit(1)
	}
}
