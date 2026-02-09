package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"path/filepath"

	"github.com/tOgg1/forge/internal/parity"
)

func main() {
	var outDir string

	flag.StringVar(&outDir, "out-dir", "", "optional output directory for schema-fingerprint.txt and schema-fingerprint.sha256")
	flag.Parse()

	fingerprint, err := parity.ComputeSchemaFingerprint(context.Background())
	if err != nil {
		fmt.Fprintf(os.Stderr, "compute schema fingerprint: %v\n", err)
		os.Exit(1)
	}

	if outDir == "" {
		fmt.Printf("sha256 %s\n", fingerprint.SHA256)
		fmt.Print(fingerprint.Dump)
		return
	}

	if err := os.MkdirAll(outDir, 0o755); err != nil {
		fmt.Fprintf(os.Stderr, "mkdir %s: %v\n", outDir, err)
		os.Exit(1)
	}

	dumpPath := filepath.Join(outDir, "schema-fingerprint.txt")
	hashPath := filepath.Join(outDir, "schema-fingerprint.sha256")
	if err := os.WriteFile(dumpPath, []byte(fingerprint.Dump), 0o644); err != nil {
		fmt.Fprintf(os.Stderr, "write %s: %v\n", dumpPath, err)
		os.Exit(1)
	}
	if err := os.WriteFile(hashPath, []byte(fingerprint.SHA256+"\n"), 0o644); err != nil {
		fmt.Fprintf(os.Stderr, "write %s: %v\n", hashPath, err)
		os.Exit(1)
	}

	fmt.Printf("wrote %s and %s\n", dumpPath, hashPath)
}
