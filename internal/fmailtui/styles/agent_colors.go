package styles

import (
	"hash/fnv"
	"strconv"
	"strings"
	"sync"

	"github.com/charmbracelet/lipgloss"
)

// AgentColorPalette is a curated ANSI 256 palette for stable agent identity colors.
// Red/green slots are intentionally avoided for semantic priority/status colors.
var AgentColorPalette = []string{
	"33", "39", "45", "69", "75", "81", "87", "99",
	"111", "117", "123", "147", "153", "159", "183", "189",
}

// AgentColorMapper resolves deterministic per-agent styles and caches them.
type AgentColorMapper struct {
	palette []string

	mu         sync.RWMutex
	fgCache    map[string]lipgloss.Style
	bgCache    map[string]lipgloss.Style
	colorCache map[string]string
}

// NewAgentColorMapper returns a deterministic mapper with default palette.
func NewAgentColorMapper() *AgentColorMapper {
	paletteCopy := make([]string, len(AgentColorPalette))
	copy(paletteCopy, AgentColorPalette)

	return &AgentColorMapper{
		palette:    paletteCopy,
		fgCache:    make(map[string]lipgloss.Style, 64),
		bgCache:    make(map[string]lipgloss.Style, 64),
		colorCache: make(map[string]string, 64),
	}
}

// Foreground returns a cached foreground style for an agent.
func (m *AgentColorMapper) Foreground(agent string) lipgloss.Style {
	key := normalizeAgent(agent)

	m.mu.RLock()
	if style, ok := m.fgCache[key]; ok {
		m.mu.RUnlock()
		return style
	}
	m.mu.RUnlock()

	colorCode := m.ColorCode(key)
	style := lipgloss.NewStyle().Foreground(lipgloss.Color(colorCode)).Bold(true)

	m.mu.Lock()
	m.fgCache[key] = style
	m.mu.Unlock()

	return style
}

// Background returns a cached background style for an agent.
func (m *AgentColorMapper) Background(agent string) lipgloss.Style {
	key := normalizeAgent(agent)

	m.mu.RLock()
	if style, ok := m.bgCache[key]; ok {
		m.mu.RUnlock()
		return style
	}
	m.mu.RUnlock()

	colorCode := m.ColorCode(key)
	fgCode := contrastingTextColor(colorCode)
	style := lipgloss.NewStyle().Foreground(lipgloss.Color(fgCode)).Background(lipgloss.Color(colorCode)).Bold(true)

	m.mu.Lock()
	m.bgCache[key] = style
	m.mu.Unlock()

	return style
}

// ColorCode returns the ANSI-256 color code selected for agent.
func (m *AgentColorMapper) ColorCode(agent string) string {
	key := normalizeAgent(agent)

	m.mu.RLock()
	if colorCode, ok := m.colorCache[key]; ok {
		m.mu.RUnlock()
		return colorCode
	}
	m.mu.RUnlock()

	idx := hashAgentToPalette(key, len(m.palette))
	colorCode := m.palette[idx]

	m.mu.Lock()
	m.colorCache[key] = colorCode
	m.mu.Unlock()

	return colorCode
}

func normalizeAgent(agent string) string {
	normalized := strings.ToLower(strings.TrimSpace(agent))
	if normalized == "" {
		return "unknown"
	}
	return normalized
}

func hashAgentToPalette(agent string, paletteLen int) int {
	if paletteLen == 0 {
		return 0
	}

	h := fnv.New32a()
	_, _ = h.Write([]byte(agent))
	return int(h.Sum32() % uint32(paletteLen))
}

func contrastingTextColor(code string) string {
	index, err := strconv.Atoi(code)
	if err != nil {
		return "231"
	}

	r, g, b := ansi256ToRGB(index)
	brightness := (299*r + 587*g + 114*b) / 1000
	if brightness >= 150 {
		return "16"
	}
	return "231"
}

func ansi256ToRGB(index int) (int, int, int) {
	if index < 0 {
		return 255, 255, 255
	}

	if index < 16 {
		table := [16][3]int{
			{0, 0, 0}, {128, 0, 0}, {0, 128, 0}, {128, 128, 0},
			{0, 0, 128}, {128, 0, 128}, {0, 128, 128}, {192, 192, 192},
			{128, 128, 128}, {255, 0, 0}, {0, 255, 0}, {255, 255, 0},
			{0, 0, 255}, {255, 0, 255}, {0, 255, 255}, {255, 255, 255},
		}
		return table[index][0], table[index][1], table[index][2]
	}

	if index >= 16 && index <= 231 {
		cube := index - 16
		r := cube / 36
		g := (cube / 6) % 6
		b := cube % 6
		return channelValue(r), channelValue(g), channelValue(b)
	}

	if index <= 255 {
		gray := 8 + (index-232)*10
		return gray, gray, gray
	}

	return 255, 255, 255
}

func channelValue(v int) int {
	if v == 0 {
		return 0
	}
	return 55 + v*40
}
