package main

import (
	"fmt"
	"os"

	"github.com/tOgg1/forge/internal/fmailtui"
)

var (
	version = "dev"
	commit  = "none"
	date    = "unknown"
)

var _ = []string{commit, date}

func main() {
	if err := fmailtui.Execute(version); err != nil {
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}
}
