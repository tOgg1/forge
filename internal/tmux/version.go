package tmux

import (
	"fmt"
	"strconv"
	"strings"
	"unicode"
)

// MinVersion is the minimum tmux version required for Forge.
var MinVersion = Version{Major: 2, Minor: 4}

// Version represents a tmux version (major.minor).
type Version struct {
	Major int
	Minor int
}

// ParseVersion parses a tmux version string (e.g., "tmux 3.3a").
func ParseVersion(output string) (Version, error) {
	trimmed := strings.TrimSpace(output)
	trimmed = strings.TrimPrefix(trimmed, "tmux")
	trimmed = strings.TrimSpace(trimmed)
	if trimmed == "" {
		return Version{}, fmt.Errorf("empty version")
	}

	major, rest := parseIntPrefix(trimmed)
	if rest == "" {
		return Version{Major: major, Minor: 0}, nil
	}
	if rest[0] != '.' {
		return Version{}, fmt.Errorf("invalid version format: %q", trimmed)
	}

	minor, _ := parseIntPrefix(rest[1:])
	return Version{Major: major, Minor: minor}, nil
}

// LessThan reports whether v is older than other.
func (v Version) LessThan(other Version) bool {
	if v.Major != other.Major {
		return v.Major < other.Major
	}
	return v.Minor < other.Minor
}

func (v Version) String() string {
	return fmt.Sprintf("%d.%d", v.Major, v.Minor)
}

func parseIntPrefix(value string) (int, string) {
	value = strings.TrimSpace(value)
	if value == "" {
		return 0, ""
	}
	i := 0
	for i < len(value) && unicode.IsDigit(rune(value[i])) {
		i++
	}
	if i == 0 {
		return 0, value
	}
	num, _ := strconv.Atoi(value[:i])
	return num, value[i:]
}
