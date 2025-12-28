// Package cli provides tests for template CLI commands.
package cli

import (
	"testing"

	"github.com/tOgg1/forge/internal/templates"
)

func TestFilterTemplates(t *testing.T) {
	items := []*templates.Template{
		{Name: "a", Tags: []string{"git", "code"}},
		{Name: "b", Tags: []string{"review"}},
		{Name: "c", Tags: []string{"git"}},
		{Name: "d", Tags: nil},
	}

	tests := []struct {
		name     string
		tags     []string
		expected int
	}{
		{"no filter", nil, 4},
		{"filter git", []string{"git"}, 2},
		{"filter review", []string{"review"}, 1},
		{"filter multiple", []string{"git", "review"}, 3},
		{"filter nonexistent", []string{"nonexistent"}, 0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := filterTemplates(items, tt.tags)
			if len(result) != tt.expected {
				t.Errorf("filterTemplates() = %d items, want %d", len(result), tt.expected)
			}
		})
	}
}

func TestFindTemplateByName(t *testing.T) {
	items := []*templates.Template{
		{Name: "continue"},
		{Name: "commit"},
		{Name: "review"},
	}

	tests := []struct {
		name    string
		search  string
		wantNil bool
	}{
		{"exact match", "continue", false},
		{"case insensitive", "CONTINUE", false},
		{"not found", "nonexistent", true},
		{"partial match fails", "cont", true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := findTemplateByName(items, tt.search)
			if (result == nil) != tt.wantNil {
				t.Errorf("findTemplateByName(%q) nil = %v, want nil = %v", tt.search, result == nil, tt.wantNil)
			}
		})
	}
}

func TestParseTemplateVars(t *testing.T) {
	tests := []struct {
		name    string
		input   []string
		wantLen int
		wantErr bool
	}{
		{"single var", []string{"key=value"}, 1, false},
		{"multiple vars", []string{"k1=v1", "k2=v2"}, 2, false},
		{"comma separated", []string{"k1=v1,k2=v2"}, 2, false},
		{"empty value", []string{"key="}, 1, false},
		{"missing equals", []string{"invalid"}, 0, true},
		{"empty key", []string{"=value"}, 0, true},
		{"empty input", nil, 0, false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := parseTemplateVars(tt.input)
			if (err != nil) != tt.wantErr {
				t.Errorf("parseTemplateVars() error = %v, wantErr = %v", err, tt.wantErr)
				return
			}
			if !tt.wantErr && len(result) != tt.wantLen {
				t.Errorf("parseTemplateVars() = %d vars, want %d", len(result), tt.wantLen)
			}
		})
	}
}

func TestNormalizeTemplateName(t *testing.T) {
	tests := []struct {
		name    string
		input   string
		wantErr bool
	}{
		{"simple name", "mytemplate", false},
		{"with dashes", "my-template", false},
		{"with underscores", "my_template", false},
		{"empty", "", true},
		{"whitespace only", "   ", true},
		{"with slash", "foo/bar", true},
		// Note: backslash is only rejected on Windows where it's the path separator
		{"with dots", "foo..bar", true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := normalizeTemplateName(tt.input)
			if (err != nil) != tt.wantErr {
				t.Errorf("normalizeTemplateName(%q) error = %v, wantErr = %v", tt.input, err, tt.wantErr)
			}
		})
	}
}

func TestTemplateSourceLabel(t *testing.T) {
	tests := []struct {
		name       string
		source     string
		userDir    string
		projectDir string
		want       string
	}{
		{"builtin", "builtin", "/home/user/.config/swarm/templates", "/project/.swarm/templates", "builtin"},
		{"user template", "/home/user/.config/swarm/templates/foo.yaml", "/home/user/.config/swarm/templates", "", "user"},
		{"project template", "/project/.swarm/templates/bar.yaml", "", "/project/.swarm/templates", "project"},
		{"other file", "/some/other/path.yaml", "/home/user/.config/swarm/templates", "/project/.swarm/templates", "file"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := templateSourceLabel(tt.source, tt.userDir, tt.projectDir)
			if result != tt.want {
				t.Errorf("templateSourceLabel() = %q, want %q", result, tt.want)
			}
		})
	}
}

func TestIndentBlock(t *testing.T) {
	tests := []struct {
		name   string
		text   string
		prefix string
		want   string
	}{
		{"single line", "hello", "  ", "  hello"},
		{"multi line", "line1\nline2\nline3", ">> ", ">> line1\n>> line2\n>> line3"},
		{"trailing newline stripped", "line1\nline2\n", "  ", "  line1\n  line2"},
		{"empty", "", "  ", "  "},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := indentBlock(tt.text, tt.prefix)
			if result != tt.want {
				t.Errorf("indentBlock() = %q, want %q", result, tt.want)
			}
		})
	}
}
