package parity

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"

	"github.com/pmezard/go-difflib/difflib"
)

// WriteDiffArtifacts writes expected/actual trees and normalized drift diffs.
func WriteDiffArtifacts(expectedDir, actualDir, outputDir string) (Report, error) {
	report, err := CompareTrees(expectedDir, actualDir)
	if err != nil {
		return report, err
	}

	expectedOut := filepath.Join(outputDir, "expected")
	actualOut := filepath.Join(outputDir, "actual")
	normalizedOut := filepath.Join(outputDir, "normalized")
	diffsOut := filepath.Join(normalizedOut, "diffs")

	for _, dir := range []string{expectedOut, actualOut, normalizedOut, diffsOut} {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return report, err
		}
	}

	if err := copyTree(expectedDir, expectedOut); err != nil {
		return report, err
	}
	if err := copyTree(actualDir, actualOut); err != nil {
		return report, err
	}

	if err := writeManifest(filepath.Join(normalizedOut, "report.json"), report); err != nil {
		return report, err
	}

	expectedFiles, err := listFiles(expectedDir)
	if err != nil {
		return report, err
	}
	actualFiles, err := listFiles(actualDir)
	if err != nil {
		return report, err
	}

	for _, rel := range report.Mismatched {
		if err := writeMismatchDiff(diffsOut, rel, expectedFiles[rel], actualFiles[rel]); err != nil {
			return report, err
		}
	}
	for _, rel := range report.MissingExpected {
		if err := os.WriteFile(filepath.Join(diffsOut, slug(rel)+".missing.diff"), []byte("missing actual file: "+rel+"\n"), 0o644); err != nil {
			return report, err
		}
	}
	for _, rel := range report.Unexpected {
		if err := os.WriteFile(filepath.Join(diffsOut, slug(rel)+".unexpected.diff"), []byte("unexpected actual file: "+rel+"\n"), 0o644); err != nil {
			return report, err
		}
	}

	return report, nil
}

func writeMismatchDiff(outDir, rel, expectedPath, actualPath string) error {
	expectedBytes, err := os.ReadFile(expectedPath)
	if err != nil {
		return err
	}
	actualBytes, err := os.ReadFile(actualPath)
	if err != nil {
		return err
	}

	expectedLines := difflib.SplitLines(string(normalize(expectedBytes)))
	actualLines := difflib.SplitLines(string(normalize(actualBytes)))
	ud := difflib.UnifiedDiff{
		A:        expectedLines,
		B:        actualLines,
		FromFile: "expected/" + rel,
		ToFile:   "actual/" + rel,
		Context:  3,
	}
	text, err := difflib.GetUnifiedDiffString(ud)
	if err != nil {
		return err
	}

	return os.WriteFile(filepath.Join(outDir, slug(rel)+".diff"), []byte(text), 0o644)
}

func writeManifest(path string, report Report) error {
	data := struct {
		MissingExpected []string `json:"missing_expected"`
		Mismatched      []string `json:"mismatched"`
		Unexpected      []string `json:"unexpected"`
	}{
		MissingExpected: report.MissingExpected,
		Mismatched:      report.Mismatched,
		Unexpected:      report.Unexpected,
	}

	out, err := json.MarshalIndent(data, "", "  ")
	if err != nil {
		return err
	}
	out = append(out, '\n')
	return os.WriteFile(path, out, 0o644)
}

func copyTree(srcRoot, dstRoot string) error {
	files, err := listFiles(srcRoot)
	if err != nil {
		return err
	}
	for rel, src := range files {
		dst := filepath.Join(dstRoot, filepath.FromSlash(rel))
		if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
			return err
		}
		if err := copyFile(src, dst); err != nil {
			return err
		}
	}
	return nil
}

func copyFile(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()

	out, err := os.Create(dst)
	if err != nil {
		return err
	}
	defer out.Close()

	if _, err := io.Copy(out, in); err != nil {
		return err
	}
	return out.Close()
}

func slug(path string) string {
	replacer := strings.NewReplacer("/", "__", "\\", "__", " ", "_", ":", "_")
	return replacer.Replace(path)
}

// DriftSummary returns a compact text summary for CI logs.
func DriftSummary(report Report) string {
	return fmt.Sprintf("mismatched=%d missing=%d unexpected=%d", len(report.Mismatched), len(report.MissingExpected), len(report.Unexpected))
}
