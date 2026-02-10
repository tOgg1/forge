package parity

import (
	"encoding/json"
	"fmt"
	"sort"
	"strings"
)

// SurfaceManifest mirrors cli.SurfaceManifest for JSON deserialization
// without importing internal/cli (which triggers Cobra init side-effects).
type SurfaceManifest struct {
	CLI         string           `json:"cli"`
	GlobalFlags []SurfaceFlag    `json:"global_flags"`
	Commands    []SurfaceCommand `json:"commands"`
}

// SurfaceCommand mirrors cli.SurfaceCommand.
type SurfaceCommand struct {
	Name        string           `json:"name"`
	Aliases     []string         `json:"aliases,omitempty"`
	Short       string           `json:"short"`
	Flags       []SurfaceFlag    `json:"flags,omitempty"`
	Subcommands []SurfaceCommand `json:"subcommands,omitempty"`
}

// SurfaceFlag mirrors cli.SurfaceFlag.
type SurfaceFlag struct {
	Long      string `json:"long"`
	Short     string `json:"short,omitempty"`
	Type      string `json:"type"`
	Default   string `json:"default,omitempty"`
	Inherited bool   `json:"inherited,omitempty"`
}

// SurfaceDrift describes a single parity gap between Go and Rust surfaces.
type SurfaceDrift struct {
	Kind    string `json:"kind"`    // "missing_command", "missing_flag", "missing_alias", "missing_subcommand"
	Path    string `json:"path"`    // dotted path e.g. "forge.up" or "forge.up.--count"
	GoValue string `json:"go_value,omitempty"`
}

// SurfaceReport holds the comparison result.
type SurfaceReport struct {
	GoCommands   int            `json:"go_commands"`
	RustCommands int            `json:"rust_commands"`
	Drifts       []SurfaceDrift `json:"drifts"`
}

// HasDrift returns true if any parity drift was detected.
func (r SurfaceReport) HasDrift() bool {
	return len(r.Drifts) > 0
}

// Summary returns a compact text summary.
func (r SurfaceReport) Summary() string {
	return fmt.Sprintf("go_commands=%d rust_commands=%d drifts=%d",
		r.GoCommands, r.RustCommands, len(r.Drifts))
}

// CompareSurfaces compares Go and Rust command surface manifests and reports
// any commands, subcommands, flags, or aliases present in Go but missing in Rust.
func CompareSurfaces(goManifest, rustManifest SurfaceManifest) SurfaceReport {
	report := SurfaceReport{
		GoCommands:   countCommands(goManifest.Commands),
		RustCommands: countCommands(rustManifest.Commands),
	}

	// Compare global flags.
	rustGlobalFlags := flagSet(rustManifest.GlobalFlags)
	for _, gf := range goManifest.GlobalFlags {
		if _, ok := rustGlobalFlags[gf.Long]; !ok {
			report.Drifts = append(report.Drifts, SurfaceDrift{
				Kind:    "missing_global_flag",
				Path:    goManifest.CLI + ".--" + gf.Long,
				GoValue: gf.Long,
			})
		}
	}

	// Compare commands recursively.
	compareCommands(goManifest.CLI, goManifest.Commands, rustManifest.Commands, &report)

	sort.Slice(report.Drifts, func(i, j int) bool {
		return report.Drifts[i].Path < report.Drifts[j].Path
	})
	return report
}

func compareCommands(parentPath string, goCmds, rustCmds []SurfaceCommand, report *SurfaceReport) {
	rustIndex := commandIndex(rustCmds)

	for _, goCmd := range goCmds {
		path := parentPath + "." + goCmd.Name
		rustCmd, ok := rustIndex[goCmd.Name]
		if !ok {
			report.Drifts = append(report.Drifts, SurfaceDrift{
				Kind:    "missing_command",
				Path:    path,
				GoValue: goCmd.Name,
			})
			continue
		}

		// Compare aliases.
		rustAliases := stringSet(rustCmd.Aliases)
		for _, alias := range goCmd.Aliases {
			if _, ok := rustAliases[alias]; !ok {
				report.Drifts = append(report.Drifts, SurfaceDrift{
					Kind:    "missing_alias",
					Path:    path + "~" + alias,
					GoValue: alias,
				})
			}
		}

		// Compare local flags.
		rustFlags := flagSet(rustCmd.Flags)
		for _, gf := range goCmd.Flags {
			if _, ok := rustFlags[gf.Long]; !ok {
				report.Drifts = append(report.Drifts, SurfaceDrift{
					Kind:    "missing_flag",
					Path:    path + ".--" + gf.Long,
					GoValue: gf.Long,
				})
			}
		}

		// Recurse into subcommands.
		compareCommands(path, goCmd.Subcommands, rustCmd.Subcommands, report)
	}
}

func commandIndex(cmds []SurfaceCommand) map[string]SurfaceCommand {
	idx := make(map[string]SurfaceCommand, len(cmds))
	for _, c := range cmds {
		idx[c.Name] = c
	}
	return idx
}

func flagSet(flags []SurfaceFlag) map[string]SurfaceFlag {
	m := make(map[string]SurfaceFlag, len(flags))
	for _, f := range flags {
		m[f.Long] = f
	}
	return m
}

func stringSet(s []string) map[string]struct{} {
	m := make(map[string]struct{}, len(s))
	for _, v := range s {
		m[v] = struct{}{}
	}
	return m
}

func countCommands(cmds []SurfaceCommand) int {
	n := len(cmds)
	for _, c := range cmds {
		n += countCommands(c.Subcommands)
	}
	return n
}

// ParseSurfaceManifestJSON unmarshals a SurfaceManifest from JSON bytes.
func ParseSurfaceManifestJSON(data []byte) (SurfaceManifest, error) {
	var m SurfaceManifest
	if err := json.Unmarshal(data, &m); err != nil {
		return m, fmt.Errorf("parse surface manifest: %w", err)
	}
	return m, nil
}

// FormatDriftReport returns a human-readable report of surface drifts.
func FormatDriftReport(report SurfaceReport) string {
	if !report.HasDrift() {
		return "No parity drift detected."
	}
	var b strings.Builder
	fmt.Fprintf(&b, "Surface parity drift: %d issue(s)\n\n", len(report.Drifts))
	for _, d := range report.Drifts {
		fmt.Fprintf(&b, "  [%s] %s\n", d.Kind, d.Path)
	}
	return b.String()
}

// ---------- help-text parser (Cobra-style output) ----------

// ParseRootHelp extracts command names from a Cobra-like --help root output.
// It looks for a "Commands:" section and extracts name/short pairs.
func ParseRootHelp(helpText string) []SurfaceCommand {
	var cmds []SurfaceCommand
	lines := strings.Split(helpText, "\n")
	inCommands := false
	for _, line := range lines {
		trimmed := strings.TrimSpace(line)
		if trimmed == "Commands:" || trimmed == "Available Commands:" {
			inCommands = true
			continue
		}
		if inCommands {
			if trimmed == "" || (!strings.HasPrefix(line, "  ") && !strings.HasPrefix(line, "\t")) {
				if trimmed != "" {
					inCommands = false
				}
				continue
			}
			parts := strings.Fields(trimmed)
			if len(parts) >= 1 {
				cmds = append(cmds, SurfaceCommand{
					Name:  parts[0],
					Short: strings.Join(parts[1:], " "),
				})
			}
		}
	}
	sort.Slice(cmds, func(i, j int) bool { return cmds[i].Name < cmds[j].Name })
	return cmds
}

// ParseGlobalFlagsFromHelp extracts global flags from a Cobra-like --help output.
// It looks for a "Global Flags:" section and parses flag lines.
func ParseGlobalFlagsFromHelp(helpText string) []SurfaceFlag {
	return parseFlagSection(helpText, "Global Flags:")
}

// ParseCommandHelp extracts flags and subcommands from a subcommand's --help output.
func ParseCommandHelp(helpText string) (flags []SurfaceFlag, subcommands []SurfaceCommand) {
	flags = parseFlagSection(helpText, "Flags:")
	subcommands = ParseRootHelp(helpText)
	return
}

// parseFlagSection extracts flags from a named section in help output.
func parseFlagSection(helpText, sectionHeader string) []SurfaceFlag {
	var flags []SurfaceFlag
	lines := strings.Split(helpText, "\n")
	inSection := false
	for _, line := range lines {
		trimmed := strings.TrimSpace(line)
		if trimmed == sectionHeader {
			inSection = true
			continue
		}
		if inSection {
			if trimmed == "" || (!strings.HasPrefix(line, "  ") && !strings.HasPrefix(line, "\t")) {
				if trimmed != "" {
					inSection = false
				}
				continue
			}
			if f, ok := parseFlagLine(trimmed); ok {
				flags = append(flags, f)
			}
		}
	}
	sort.Slice(flags, func(i, j int) bool { return flags[i].Long < flags[j].Long })
	return flags
}

// parseFlagLine parses a single flag line like:
//
//	"-n, --count int   number of loops (default 1)"
//	"    --json        output in JSON format"
func parseFlagLine(line string) (SurfaceFlag, bool) {
	var f SurfaceFlag
	parts := strings.Fields(line)
	if len(parts) == 0 {
		return f, false
	}

	idx := 0
	// Check for short flag first: "-x," or "-x"
	if strings.HasPrefix(parts[idx], "-") && !strings.HasPrefix(parts[idx], "--") {
		short := strings.TrimRight(parts[idx], ",")
		short = strings.TrimPrefix(short, "-")
		f.Short = short
		idx++
	}

	if idx >= len(parts) {
		return f, false
	}

	// Next should be the long flag.
	if !strings.HasPrefix(parts[idx], "--") {
		return f, false
	}
	f.Long = strings.TrimPrefix(parts[idx], "--")
	idx++

	// If next token doesn't start with a letter or isn't a known type,
	// assume it's a description (bool flag).
	f.Type = "bool"
	if idx < len(parts) {
		candidate := strings.ToLower(parts[idx])
		switch candidate {
		case "string", "int", "duration", "stringslice":
			f.Type = candidate
			idx++
		default:
			// No type specified → bool.
		}
	}

	return f, true
}
