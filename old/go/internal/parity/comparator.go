package parity

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"regexp"
	"sort"
)

// Format describes fixture content type.
type Format string

const (
	FormatText Format = "text"
	FormatJSON Format = "json"
)

// CompareOptions controls comparator normalization behavior.
type CompareOptions struct {
	Format              Format
	NormalizeTimestamps bool
	NormalizeIDs        bool
	NormalizePaths      bool
	NormalizeOrder      bool
}

// DefaultCompareOptions returns stable defaults for parity comparisons.
func DefaultCompareOptions(format Format) CompareOptions {
	return CompareOptions{
		Format:              format,
		NormalizeTimestamps: true,
		NormalizeIDs:        true,
		NormalizePaths:      true,
		NormalizeOrder:      true,
	}
}

// FixtureComparison is the result of comparing one fixture pair.
type FixtureComparison struct {
	Name               string
	Equal              bool
	NormalizedExpected []byte
	NormalizedActual   []byte
}

// FixtureSetReport captures full fixture-run results.
type FixtureSetReport struct {
	Comparisons []FixtureComparison
}

// HasDrift reports whether any fixture comparison mismatched.
func (r FixtureSetReport) HasDrift() bool {
	for _, c := range r.Comparisons {
		if !c.Equal {
			return true
		}
	}
	return false
}

// CompareBytes compares expected/actual fixture payloads using normalizers.
func CompareBytes(expected, actual []byte, opts CompareOptions) (FixtureComparison, error) {
	nExpected, err := applyNormalizers(expected, opts)
	if err != nil {
		return FixtureComparison{}, err
	}
	nActual, err := applyNormalizers(actual, opts)
	if err != nil {
		return FixtureComparison{}, err
	}

	return FixtureComparison{
		Equal:              bytes.Equal(nExpected, nActual),
		NormalizedExpected: nExpected,
		NormalizedActual:   nActual,
	}, nil
}

// LoadFixturePair loads one fixture file from expected/actual directories.
func LoadFixturePair(root, relPath string) ([]byte, []byte, error) {
	expected, err := os.ReadFile(filepath.Join(root, "expected", filepath.FromSlash(relPath)))
	if err != nil {
		return nil, nil, err
	}
	actual, err := os.ReadFile(filepath.Join(root, "actual", filepath.FromSlash(relPath)))
	if err != nil {
		return nil, nil, err
	}
	return expected, actual, nil
}

// RunFixtureSet compares all files under root/expected against root/actual.
func RunFixtureSet(root string, opts CompareOptions) (FixtureSetReport, error) {
	expectedRoot := filepath.Join(root, "expected")
	expectedFiles, err := listFiles(expectedRoot)
	if err != nil {
		return FixtureSetReport{}, err
	}

	report := FixtureSetReport{
		Comparisons: make([]FixtureComparison, 0, len(expectedFiles)),
	}
	for rel := range expectedFiles {
		expected, actual, err := LoadFixturePair(root, rel)
		if err != nil {
			return FixtureSetReport{}, err
		}
		comparison, err := CompareBytes(expected, actual, opts)
		if err != nil {
			return FixtureSetReport{}, err
		}
		comparison.Name = rel
		report.Comparisons = append(report.Comparisons, comparison)
	}
	sort.Slice(report.Comparisons, func(i, j int) bool {
		return report.Comparisons[i].Name < report.Comparisons[j].Name
	})
	return report, nil
}

func applyNormalizers(in []byte, opts CompareOptions) ([]byte, error) {
	out := normalize(in)

	if opts.NormalizeTimestamps {
		out = reTimestamp.ReplaceAll(out, []byte("<ts>"))
	}
	if opts.NormalizeIDs {
		out = reID.ReplaceAll(out, []byte("<id>"))
	}
	if opts.NormalizePaths {
		out = reUnixHomePath.ReplaceAll(out, []byte("<path>"))
		out = reWindowsPath.ReplaceAll(out, []byte("<path>"))
	}
	if opts.Format == FormatJSON && opts.NormalizeOrder {
		var err error
		out, err = canonicalizeJSON(out)
		if err != nil {
			return nil, err
		}
	}

	return out, nil
}

func canonicalizeJSON(in []byte) ([]byte, error) {
	var v any
	if err := json.Unmarshal(in, &v); err != nil {
		return nil, err
	}
	v = canonicalValue(v)
	return json.Marshal(v)
}

func canonicalValue(v any) any {
	switch tv := v.(type) {
	case map[string]any:
		for k, inner := range tv {
			tv[k] = canonicalValue(inner)
		}
		return tv
	case []any:
		for i, inner := range tv {
			tv[i] = canonicalValue(inner)
		}
		sort.Slice(tv, func(i, j int) bool {
			return stableJSON(tv[i]) < stableJSON(tv[j])
		})
		return tv
	default:
		return v
	}
}

func stableJSON(v any) string {
	out, _ := json.Marshal(v)
	return string(out)
}

var (
	reTimestamp    = regexp.MustCompile(`\b\d{4}-\d{2}-\d{2}[T ][0-2]\d:[0-5]\d:[0-5]\d(?:\.\d+)?(?:Z|[+-][0-2]\d:[0-5]\d)?\b`)
	reID           = regexp.MustCompile(`\b\d{8}-\d{6}-\d{4,}\b`)
	reUnixHomePath = regexp.MustCompile(`/Users/[A-Za-z0-9._-]+/[A-Za-z0-9._/\-]+`)
	reWindowsPath  = regexp.MustCompile(`[A-Za-z]:\\[^\s"]+`)
)
