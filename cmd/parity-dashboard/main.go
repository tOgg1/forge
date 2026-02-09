package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"time"

	"github.com/tOgg1/forge/internal/paritydash"
)

func main() {
	var inputPath string
	var outDir string
	var writeMD bool

	flag.StringVar(&inputPath, "input", "", "input JSON file produced by CI")
	flag.StringVar(&outDir, "out", "parity-dashboard", "output directory")
	flag.BoolVar(&writeMD, "md", true, "write parity-dashboard.md")
	flag.Parse()

	if inputPath == "" {
		fmt.Fprintln(os.Stderr, "usage: parity-dashboard --input <file> [--out <dir>] [--md=true|false]")
		os.Exit(2)
	}

	b, err := os.ReadFile(inputPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "read input: %v\n", err)
		os.Exit(1)
	}

	var in paritydash.Input
	if err := json.Unmarshal(b, &in); err != nil {
		fmt.Fprintf(os.Stderr, "parse input: %v\n", err)
		os.Exit(1)
	}

	d, err := paritydash.Build(in, time.Now())
	if err != nil {
		fmt.Fprintf(os.Stderr, "build: %v\n", err)
		os.Exit(1)
	}

	if err := paritydash.WriteFiles(outDir, d, writeMD); err != nil {
		fmt.Fprintf(os.Stderr, "write: %v\n", err)
		os.Exit(1)
	}

	fmt.Printf("wrote %s\n", outDir)
}

