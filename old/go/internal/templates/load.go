package templates

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"gopkg.in/yaml.v3"
)

// LoadTemplate reads a single template from disk.
func LoadTemplate(path string) (*Template, error) {
	if strings.TrimSpace(path) == "" {
		return nil, fmt.Errorf("template path is required")
	}

	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read template %s: %w", path, err)
	}

	tmpl, err := parseTemplate(data)
	if err != nil {
		return nil, fmt.Errorf("parse template %s: %w", path, err)
	}
	tmpl.Source = path
	return tmpl, nil
}

// LoadTemplatesFromDir loads all templates from a directory.
func LoadTemplatesFromDir(dir string) ([]*Template, error) {
	if strings.TrimSpace(dir) == "" {
		return []*Template{}, nil
	}

	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return []*Template{}, nil
		}
		return nil, fmt.Errorf("read templates dir %s: %w", dir, err)
	}

	templates := make([]*Template, 0)
	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}
		name := entry.Name()
		ext := strings.ToLower(filepath.Ext(name))
		if ext != ".yaml" && ext != ".yml" {
			continue
		}
		path := filepath.Join(dir, name)
		tmpl, err := LoadTemplate(path)
		if err != nil {
			return nil, err
		}
		templates = append(templates, tmpl)
	}

	sort.Slice(templates, func(i, j int) bool {
		return templates[i].Name < templates[j].Name
	})

	return templates, nil
}

func parseTemplate(data []byte) (*Template, error) {
	var tmpl Template
	if err := yaml.Unmarshal(data, &tmpl); err != nil {
		return nil, err
	}

	tmpl.Name = strings.TrimSpace(tmpl.Name)
	if tmpl.Name == "" {
		return nil, fmt.Errorf("template name is required")
	}

	if strings.TrimSpace(tmpl.Message) == "" {
		return nil, fmt.Errorf("template message is required")
	}

	seen := make(map[string]struct{})
	for i := range tmpl.Variables {
		name := strings.TrimSpace(tmpl.Variables[i].Name)
		if name == "" {
			return nil, fmt.Errorf("template variable name is required")
		}
		if _, exists := seen[name]; exists {
			return nil, fmt.Errorf("duplicate template variable %q", name)
		}
		seen[name] = struct{}{}
		tmpl.Variables[i].Name = name
	}

	return &tmpl, nil
}
