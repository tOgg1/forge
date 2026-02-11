// Package main is the entry point for the fmail CLI.
package main

import (
	"errors"
	"fmt"
	"os"

	"github.com/tOgg1/forge/internal/fmail"
	"github.com/tOgg1/forge/internal/fmailtui"
)

// Version information (set by goreleaser)
var (
	version = "dev"
	commit  = "none"
	date    = "unknown"
)

var _ = []string{commit, date}

func main() {
	// Default entrypoint: launch the TUI when invoked with no args.
	if len(os.Args) == 1 {
		if err := fmailtui.Execute(version); err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			os.Exit(1)
		}
		return
	}

	if err := fmail.Execute(version); err != nil {
		var exitErr *fmail.ExitError
		if errors.As(err, &exitErr) {
			if !exitErr.Printed {
				fmt.Fprintf(os.Stderr, "Error: %v\n", exitErr.Err)
			}
			os.Exit(exitErr.Code)
		}

		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}
}
