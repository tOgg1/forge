package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

var depLineRE = regexp.MustCompile(`^\s*([A-Za-z0-9_-]+)\s*=`)

func main() {
	var policyPath string

	flag.StringVar(&policyPath, "policy", "docs/rust-crate-boundaries.json", "path to crate boundary policy json")
	flag.Parse()

	policy, err := readPolicy(policyPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "rust-boundary-check: %v\n", err)
		os.Exit(2)
	}

	manifests, err := filepath.Glob(filepath.Join("rust", "crates", "*", "Cargo.toml"))
	if err != nil {
		fmt.Fprintf(os.Stderr, "rust-boundary-check: glob manifests: %v\n", err)
		os.Exit(2)
	}

	type crateInfo struct {
		name string
		deps []string
	}

	crates := make([]crateInfo, 0, len(manifests))
	local := make(map[string]struct{}, len(manifests))
	violations := 0

	for _, manifest := range manifests {
		name, deps, err := parseManifest(manifest)
		if err != nil {
			fmt.Fprintf(os.Stderr, "boundary violation: %v\n", err)
			violations++
			continue
		}
		crates = append(crates, crateInfo{name: name, deps: deps})
		local[name] = struct{}{}
	}

	for _, crate := range crates {
		crateLayer, ok := policy[crate.name]
		if !ok {
			fmt.Fprintf(os.Stderr, "boundary violation: local crate %q missing from %s\n", crate.name, policyPath)
			violations++
			continue
		}

		for _, dep := range crate.deps {
			if _, ok := local[dep]; !ok {
				continue
			}
			depLayer, ok := policy[dep]
			if !ok {
				fmt.Fprintf(os.Stderr, "boundary violation: dependency crate %q missing from %s\n", dep, policyPath)
				violations++
				continue
			}
			if depLayer > crateLayer {
				fmt.Fprintf(os.Stderr, "boundary violation: %s(layer=%d) -> %s(layer=%d) is upward\n", crate.name, crateLayer, dep, depLayer)
				violations++
			}
		}
	}

	if violations > 0 {
		fmt.Fprintf(os.Stderr, "rust-boundary-check: FAIL (%d violation(s))\n", violations)
		os.Exit(1)
	}

	fmt.Printf("rust-boundary-check: PASS (%d local crate(s) checked)\n", len(crates))
}

func readPolicy(path string) (map[string]int, error) {
	body, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read policy %s: %w", path, err)
	}
	var out map[string]int
	if err := json.Unmarshal(body, &out); err != nil {
		return nil, fmt.Errorf("parse policy %s: %w", path, err)
	}
	return out, nil
}

func parseManifest(path string) (string, []string, error) {
	body, err := os.ReadFile(path)
	if err != nil {
		return "", nil, fmt.Errorf("read %s: %w", path, err)
	}

	section := ""
	crateName := ""
	depSet := map[string]struct{}{}

	lines := strings.Split(string(body), "\n")
	for _, line := range lines {
		trimmed := stripComment(strings.TrimSpace(line))
		if trimmed == "" {
			continue
		}
		if strings.HasPrefix(trimmed, "[") && strings.HasSuffix(trimmed, "]") {
			section = strings.Trim(trimmed, "[]")
			continue
		}
		if section == "package" && strings.HasPrefix(trimmed, "name") && strings.Contains(trimmed, "=") {
			value := strings.TrimSpace(strings.SplitN(trimmed, "=", 2)[1])
			crateName = strings.Trim(value, "\"")
			continue
		}
		if section == "dependencies" || strings.HasPrefix(section, "dependencies.") ||
			section == "build-dependencies" || strings.HasPrefix(section, "build-dependencies.") {
			matches := depLineRE.FindStringSubmatch(trimmed)
			if len(matches) == 2 {
				depSet[matches[1]] = struct{}{}
			}
		}
	}

	if crateName == "" {
		return "", nil, fmt.Errorf("parse crate name from %s", path)
	}

	deps := make([]string, 0, len(depSet))
	for dep := range depSet {
		deps = append(deps, dep)
	}
	sort.Strings(deps)
	return crateName, deps, nil
}

func stripComment(line string) string {
	for i := 0; i < len(line); i++ {
		if line[i] == '#' {
			return strings.TrimSpace(line[:i])
		}
	}
	return line
}
