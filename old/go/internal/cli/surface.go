package cli

import (
	"encoding/json"
	"sort"
	"strings"

	"github.com/spf13/cobra"
	"github.com/spf13/pflag"
)

// SurfaceCommand represents one command in the CLI surface manifest.
type SurfaceCommand struct {
	Name        string           `json:"name"`
	Aliases     []string         `json:"aliases,omitempty"`
	Short       string           `json:"short"`
	Flags       []SurfaceFlag    `json:"flags,omitempty"`
	Subcommands []SurfaceCommand `json:"subcommands,omitempty"`
}

// SurfaceFlag represents a flag in the CLI surface manifest.
type SurfaceFlag struct {
	Long      string `json:"long"`
	Short     string `json:"short,omitempty"`
	Type      string `json:"type"`
	Default   string `json:"default,omitempty"`
	Inherited bool   `json:"inherited,omitempty"`
}

// SurfaceManifest is the top-level structure for the command surface.
type SurfaceManifest struct {
	CLI         string           `json:"cli"`
	GlobalFlags []SurfaceFlag    `json:"global_flags"`
	Commands    []SurfaceCommand `json:"commands"`
}

// CommandSurfaceJSON returns a JSON-encoded manifest of the Go forge CLI's
// command tree.  It walks rootCmd and captures every command, subcommand,
// flag, and alias.
func CommandSurfaceJSON() ([]byte, error) {
	manifest := extractManifest(rootCmd, "forge")
	return json.MarshalIndent(manifest, "", "  ")
}

func extractManifest(root *cobra.Command, name string) SurfaceManifest {
	m := SurfaceManifest{
		CLI:         name,
		GlobalFlags: extractFlags(root.PersistentFlags(), false),
		Commands:    extractSubcommands(root),
	}
	return m
}

func extractSubcommands(cmd *cobra.Command) []SurfaceCommand {
	var cmds []SurfaceCommand
	for _, c := range cmd.Commands() {
		if c.Hidden {
			continue
		}
		if c.Name() == "help" || c.Name() == "completion" {
			// completion is a standard Cobra auto-gen; include it
			// help is auto-generated; skip it unless explicitly registered
			if c.Name() == "help" {
				continue
			}
		}
		sc := SurfaceCommand{
			Name:        c.Name(),
			Aliases:     c.Aliases,
			Short:       c.Short,
			Flags:       extractFlags(c.LocalFlags(), false),
			Subcommands: extractSubcommands(c),
		}
		cmds = append(cmds, sc)
	}
	sort.Slice(cmds, func(i, j int) bool { return cmds[i].Name < cmds[j].Name })
	return cmds
}

func extractFlags(fs *pflag.FlagSet, inherited bool) []SurfaceFlag {
	var flags []SurfaceFlag
	fs.VisitAll(func(f *pflag.Flag) {
		// Skip help — auto-added by Cobra.
		if f.Name == "help" {
			return
		}
		sf := SurfaceFlag{
			Long:      f.Name,
			Type:      flagTypeName(f.Value.Type()),
			Default:   f.DefValue,
			Inherited: inherited,
		}
		if f.Shorthand != "" {
			sf.Short = f.Shorthand
		}
		flags = append(flags, sf)
	})
	sort.Slice(flags, func(i, j int) bool { return flags[i].Long < flags[j].Long })
	return flags
}

func flagTypeName(t string) string {
	// Normalize Cobra's type names to simple strings.
	switch strings.ToLower(t) {
	case "string":
		return "string"
	case "bool":
		return "bool"
	case "int":
		return "int"
	case "stringslice":
		return "stringSlice"
	case "duration":
		return "duration"
	default:
		return t
	}
}
