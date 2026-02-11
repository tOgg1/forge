package parity

import (
	"bytes"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// Report summarizes parity drift between expected and actual trees.
type Report struct {
	MissingExpected []string
	Mismatched      []string
	Unexpected      []string
}

// HasDrift reports whether any parity drift was detected.
func (r Report) HasDrift() bool {
	return len(r.MissingExpected) > 0 || len(r.Mismatched) > 0 || len(r.Unexpected) > 0
}

// CompareTrees compares files in expectedDir and actualDir for parity.
func CompareTrees(expectedDir, actualDir string) (Report, error) {
	report := Report{}

	expected, err := listFiles(expectedDir)
	if err != nil {
		return report, err
	}
	actual, err := listFiles(actualDir)
	if err != nil {
		return report, err
	}

	for rel, expectedPath := range expected {
		actualPath, ok := actual[rel]
		if !ok {
			report.MissingExpected = append(report.MissingExpected, rel)
			continue
		}
		if equal, err := filesEqual(expectedPath, actualPath); err != nil {
			return report, err
		} else if !equal {
			report.Mismatched = append(report.Mismatched, rel)
		}
	}

	for rel := range actual {
		if _, ok := expected[rel]; !ok {
			report.Unexpected = append(report.Unexpected, rel)
		}
	}

	sort.Strings(report.MissingExpected)
	sort.Strings(report.Mismatched)
	sort.Strings(report.Unexpected)

	return report, nil
}

func listFiles(root string) (map[string]string, error) {
	files := make(map[string]string)
	err := filepath.WalkDir(root, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			return nil
		}
		rel, err := filepath.Rel(root, path)
		if err != nil {
			return err
		}
		files[filepath.ToSlash(rel)] = path
		return nil
	})
	return files, err
}

func filesEqual(expectedPath, actualPath string) (bool, error) {
	expected, err := os.ReadFile(expectedPath)
	if err != nil {
		return false, err
	}
	actual, err := os.ReadFile(actualPath)
	if err != nil {
		return false, err
	}
	return bytes.Equal(normalize(expected), normalize(actual)), nil
}

func normalize(in []byte) []byte {
	s := strings.ReplaceAll(string(in), "\r\n", "\n")
	lines := strings.Split(s, "\n")
	for i, line := range lines {
		lines[i] = strings.TrimRight(line, " \t")
	}
	return []byte(strings.Join(lines, "\n"))
}
