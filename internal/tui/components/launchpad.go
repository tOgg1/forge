// Package components provides reusable TUI components.
package components

import (
	"fmt"
	"sort"
	"strconv"
	"strings"

	"github.com/opencode-ai/swarm/internal/models"
	"github.com/opencode-ai/swarm/internal/sequences"
	"github.com/opencode-ai/swarm/internal/tui/styles"
)

// LaunchpadStep identifies wizard steps.
type LaunchpadStep int

const (
	LaunchpadStepWorkspace LaunchpadStep = iota
	LaunchpadStepAgentType
	LaunchpadStepCount
	LaunchpadStepAccounts
	LaunchpadStepSequence
	LaunchpadStepConfirm
)

// AccountRotationMode defines how accounts are assigned to agents.
type AccountRotationMode string

const (
	AccountRotationRoundRobin AccountRotationMode = "round-robin"
	AccountRotationSingle     AccountRotationMode = "single"
	AccountRotationBalanced   AccountRotationMode = "balanced"
)

// LaunchpadWorkspace represents a workspace option.
type LaunchpadWorkspace struct {
	ID   string
	Name string
	Path string
}

// LaunchpadAgentType represents an agent type option.
type LaunchpadAgentType struct {
	Type        models.AgentType
	Name        string
	Description string
}

// LaunchpadAccount represents an account/profile option.
type LaunchpadAccount struct {
	ID       string
	Name     string
	Selected bool
}

// LaunchpadSequenceOption represents a sequence option.
type LaunchpadSequenceOption struct {
	Name        string
	Description string
	IsCustom    bool
}

// LaunchpadConfig holds the final spawn configuration.
type LaunchpadConfig struct {
	WorkspaceID      string
	WorkspaceName    string
	WorkspacePath    string
	AgentType        models.AgentType
	Count            int
	RotationMode     AccountRotationMode
	SelectedAccounts []string
	SequenceName     string
	CustomMessage    string
}

// Launchpad manages the multi-step wizard state.
type Launchpad struct {
	Step   LaunchpadStep
	Config LaunchpadConfig

	// Data sources
	Workspaces []LaunchpadWorkspace
	AgentTypes []LaunchpadAgentType
	Accounts   []LaunchpadAccount
	Sequences  []LaunchpadSequenceOption

	// Selection state per step
	WorkspaceIndex int
	AgentTypeIndex int
	CountInput     string
	RotationIndex  int
	SequenceIndex  int

	// Custom path input (for creating new workspace)
	CustomPathMode  bool
	CustomPathInput string

	// Error message (if any)
	Error string
}

// NewLaunchpad creates a new launchpad wizard.
func NewLaunchpad() *Launchpad {
	return &Launchpad{
		Step:       LaunchpadStepWorkspace,
		CountInput: "1",
		Config: LaunchpadConfig{
			Count:        1,
			RotationMode: AccountRotationRoundRobin,
		},
		AgentTypes: defaultAgentTypes(),
	}
}

func defaultAgentTypes() []LaunchpadAgentType {
	return []LaunchpadAgentType{
		{Type: models.AgentTypeOpenCode, Name: "OpenCode", Description: "Fast, hackable"},
		{Type: models.AgentTypeClaudeCode, Name: "Claude Code", Description: "Multi-file, agentic"},
		{Type: models.AgentTypeCodex, Name: "Codex", Description: "OpenAI reasoning"},
		{Type: models.AgentTypeGemini, Name: "Gemini", Description: "Google multimodal"},
		{Type: models.AgentTypeGeneric, Name: "Generic", Description: "Custom CLI wrapper"},
	}
}

// SetWorkspaces updates the workspace list.
func (l *Launchpad) SetWorkspaces(workspaces []LaunchpadWorkspace) {
	l.Workspaces = workspaces
	l.ClampWorkspaceIndex()
}

// SetAccounts updates the account list.
func (l *Launchpad) SetAccounts(accounts []LaunchpadAccount) {
	l.Accounts = accounts
}

// SetSequences updates the sequence list.
func (l *Launchpad) SetSequences(seqs []*sequences.Sequence) {
	options := []LaunchpadSequenceOption{
		{Name: "none", Description: "Start with no prompt", IsCustom: false},
	}
	for _, seq := range seqs {
		options = append(options, LaunchpadSequenceOption{
			Name:        seq.Name,
			Description: seq.Description,
			IsCustom:    false,
		})
	}
	options = append(options, LaunchpadSequenceOption{
		Name:        "custom",
		Description: "Enter message now",
		IsCustom:    true,
	})
	l.Sequences = options
	l.ClampSequenceIndex()
}

// Reset resets the wizard to initial state.
func (l *Launchpad) Reset() {
	l.Step = LaunchpadStepWorkspace
	l.WorkspaceIndex = 0
	l.AgentTypeIndex = 0
	l.CountInput = "1"
	l.RotationIndex = 0
	l.SequenceIndex = 0
	l.CustomPathMode = false
	l.CustomPathInput = ""
	l.Error = ""
	l.Config = LaunchpadConfig{
		Count:        1,
		RotationMode: AccountRotationRoundRobin,
	}
}

// ClampWorkspaceIndex ensures workspace index is valid.
func (l *Launchpad) ClampWorkspaceIndex() {
	if len(l.Workspaces) == 0 {
		l.WorkspaceIndex = 0
		return
	}
	maxIdx := len(l.Workspaces)
	if l.WorkspaceIndex < 0 {
		l.WorkspaceIndex = 0
	}
	if l.WorkspaceIndex > maxIdx {
		l.WorkspaceIndex = maxIdx
	}
}

// ClampSequenceIndex ensures sequence index is valid.
func (l *Launchpad) ClampSequenceIndex() {
	if len(l.Sequences) == 0 {
		l.SequenceIndex = 0
		return
	}
	if l.SequenceIndex < 0 {
		l.SequenceIndex = 0
	}
	if l.SequenceIndex >= len(l.Sequences) {
		l.SequenceIndex = len(l.Sequences) - 1
	}
}

// MoveSelection moves selection within current step.
func (l *Launchpad) MoveSelection(delta int) {
	switch l.Step {
	case LaunchpadStepWorkspace:
		maxIdx := len(l.Workspaces)
		l.WorkspaceIndex = clampIndex(l.WorkspaceIndex+delta, 0, maxIdx)
	case LaunchpadStepAgentType:
		l.AgentTypeIndex = clampIndex(l.AgentTypeIndex+delta, 0, len(l.AgentTypes)-1)
	case LaunchpadStepAccounts:
		l.RotationIndex = clampIndex(l.RotationIndex+delta, 0, 2)
	case LaunchpadStepSequence:
		l.SequenceIndex = clampIndex(l.SequenceIndex+delta, 0, len(l.Sequences)-1)
	}
}

// NextStep advances to the next wizard step.
func (l *Launchpad) NextStep() bool {
	l.Error = ""

	switch l.Step {
	case LaunchpadStepWorkspace:
		if l.CustomPathMode {
			if strings.TrimSpace(l.CustomPathInput) == "" {
				l.Error = "Path is required"
				return false
			}
			l.Config.WorkspacePath = strings.TrimSpace(l.CustomPathInput)
			l.Config.WorkspaceName = ""
			l.Config.WorkspaceID = ""
		} else if l.WorkspaceIndex < len(l.Workspaces) {
			ws := l.Workspaces[l.WorkspaceIndex]
			l.Config.WorkspaceID = ws.ID
			l.Config.WorkspaceName = ws.Name
			l.Config.WorkspacePath = ws.Path
		} else {
			l.CustomPathMode = true
			return false
		}
		l.Step = LaunchpadStepAgentType

	case LaunchpadStepAgentType:
		if l.AgentTypeIndex < len(l.AgentTypes) {
			l.Config.AgentType = l.AgentTypes[l.AgentTypeIndex].Type
		}
		l.Step = LaunchpadStepCount

	case LaunchpadStepCount:
		count, err := strconv.Atoi(strings.TrimSpace(l.CountInput))
		if err != nil || count < 1 {
			l.Error = "Enter a valid count (1 or more)"
			return false
		}
		if count > 50 {
			l.Error = "Maximum 50 agents at once"
			return false
		}
		l.Config.Count = count
		l.Step = LaunchpadStepAccounts

	case LaunchpadStepAccounts:
		switch l.RotationIndex {
		case 0:
			l.Config.RotationMode = AccountRotationRoundRobin
		case 1:
			l.Config.RotationMode = AccountRotationSingle
		case 2:
			l.Config.RotationMode = AccountRotationBalanced
		}
		selected := []string{}
		for _, acc := range l.Accounts {
			if acc.Selected {
				selected = append(selected, acc.ID)
			}
		}
		if len(selected) == 0 && len(l.Accounts) > 0 {
			selected = []string{l.Accounts[0].ID}
		}
		l.Config.SelectedAccounts = selected
		l.Step = LaunchpadStepSequence

	case LaunchpadStepSequence:
		if l.SequenceIndex < len(l.Sequences) {
			seq := l.Sequences[l.SequenceIndex]
			if seq.Name == "none" {
				l.Config.SequenceName = ""
			} else if seq.IsCustom {
				l.Config.SequenceName = ""
			} else {
				l.Config.SequenceName = seq.Name
			}
		}
		l.Step = LaunchpadStepConfirm

	case LaunchpadStepConfirm:
		return true
	}

	return false
}

// PrevStep goes back to the previous step.
func (l *Launchpad) PrevStep() bool {
	l.Error = ""

	if l.CustomPathMode {
		l.CustomPathMode = false
		return false
	}

	switch l.Step {
	case LaunchpadStepWorkspace:
		return true
	case LaunchpadStepAgentType:
		l.Step = LaunchpadStepWorkspace
	case LaunchpadStepCount:
		l.Step = LaunchpadStepAgentType
	case LaunchpadStepAccounts:
		l.Step = LaunchpadStepCount
	case LaunchpadStepSequence:
		l.Step = LaunchpadStepAccounts
	case LaunchpadStepConfirm:
		l.Step = LaunchpadStepSequence
	}
	return false
}

// ToggleAccountSelection toggles selection on an account.
func (l *Launchpad) ToggleAccountSelection(index int) {
	if index >= 0 && index < len(l.Accounts) {
		l.Accounts[index].Selected = !l.Accounts[index].Selected
	}
}

// AppendCountInput appends a character to count input.
func (l *Launchpad) AppendCountInput(ch rune) {
	if ch >= '0' && ch <= '9' {
		l.CountInput += string(ch)
	}
}

// BackspaceCountInput removes the last character from count input.
func (l *Launchpad) BackspaceCountInput() {
	if len(l.CountInput) > 0 {
		l.CountInput = l.CountInput[:len(l.CountInput)-1]
	}
}

// SetCountPreset sets the count to a preset value.
func (l *Launchpad) SetCountPreset(count int) {
	l.CountInput = strconv.Itoa(count)
}

// AppendCustomPath appends a character to custom path input.
func (l *Launchpad) AppendCustomPath(ch rune) {
	l.CustomPathInput += string(ch)
}

// BackspaceCustomPath removes the last character from custom path.
func (l *Launchpad) BackspaceCustomPath() {
	if len(l.CustomPathInput) > 0 {
		l.CustomPathInput = l.CustomPathInput[:len(l.CustomPathInput)-1]
	}
}

// Render renders the launchpad wizard.
func (l *Launchpad) Render(styleSet styles.Styles, width int) []string {
	lines := []string{}

	stepNum := int(l.Step) + 1
	totalSteps := 6
	title := fmt.Sprintf("Launchpad: Spawn Agents (Step %d/%d)", stepNum, totalSteps)
	lines = append(lines, styleSet.Accent.Render(title))
	lines = append(lines, strings.Repeat("-", minInt(width, 60)))
	lines = append(lines, "")

	switch l.Step {
	case LaunchpadStepWorkspace:
		lines = append(lines, l.renderWorkspaceStep(styleSet)...)
	case LaunchpadStepAgentType:
		lines = append(lines, l.renderAgentTypeStep(styleSet)...)
	case LaunchpadStepCount:
		lines = append(lines, l.renderCountStep(styleSet)...)
	case LaunchpadStepAccounts:
		lines = append(lines, l.renderAccountsStep(styleSet)...)
	case LaunchpadStepSequence:
		lines = append(lines, l.renderSequenceStep(styleSet)...)
	case LaunchpadStepConfirm:
		lines = append(lines, l.renderConfirmStep(styleSet)...)
	}

	if l.Error != "" {
		lines = append(lines, "")
		lines = append(lines, styleSet.Error.Render("Error: "+l.Error))
	}

	lines = append(lines, "")
	lines = append(lines, l.renderFooter(styleSet))

	return lines
}

func (l *Launchpad) renderWorkspaceStep(styleSet styles.Styles) []string {
	lines := []string{styleSet.Text.Render("Select or create workspace:")}
	lines = append(lines, "")

	if l.CustomPathMode {
		lines = append(lines, styleSet.Text.Render("Enter workspace path:"))
		lines = append(lines, styleSet.Focus.Render(fmt.Sprintf("> %s_", l.CustomPathInput)))
		return lines
	}

	for idx, ws := range l.Workspaces {
		marker := "o"
		style := styleSet.Muted
		if idx == l.WorkspaceIndex {
			marker = ">"
			style = styleSet.Focus
		}
		line := fmt.Sprintf("%s %-16s %s", marker, ws.Name, ws.Path)
		lines = append(lines, style.Render(line))
	}

	marker := "o"
	style := styleSet.Muted
	if l.WorkspaceIndex == len(l.Workspaces) {
		marker = ">"
		style = styleSet.Focus
	}
	lines = append(lines, style.Render(fmt.Sprintf("%s [Create new from path...]", marker)))

	return lines
}

func (l *Launchpad) renderAgentTypeStep(styleSet styles.Styles) []string {
	lines := []string{styleSet.Text.Render("Select agent type:")}
	lines = append(lines, "")

	for idx, at := range l.AgentTypes {
		marker := "o"
		style := styleSet.Muted
		if idx == l.AgentTypeIndex {
			marker = ">"
			style = styleSet.Focus
		}
		line := fmt.Sprintf("%s %-14s %s", marker, at.Name, at.Description)
		lines = append(lines, style.Render(line))
	}

	return lines
}

func (l *Launchpad) renderCountStep(styleSet styles.Styles) []string {
	lines := []string{styleSet.Text.Render("How many agents?")}
	lines = append(lines, "")
	lines = append(lines, styleSet.Focus.Render(fmt.Sprintf("Count: [%s_]", l.CountInput)))
	lines = append(lines, "")
	lines = append(lines, styleSet.Muted.Render("Presets: [1] [2] [4] [8] [16]"))

	return lines
}

func (l *Launchpad) renderAccountsStep(styleSet styles.Styles) []string {
	lines := []string{styleSet.Text.Render("Account rotation mode:")}
	lines = append(lines, "")

	modes := []struct {
		name string
		desc string
	}{
		{"Round-robin", "Cycle through profiles"},
		{"Single", "Use one profile for all"},
		{"Balanced", "Prefer least-used profiles"},
	}

	for idx, mode := range modes {
		marker := "o"
		style := styleSet.Muted
		if idx == l.RotationIndex {
			marker = ">"
			style = styleSet.Focus
		}
		line := fmt.Sprintf("%s %-14s %s", marker, mode.name, mode.desc)
		lines = append(lines, style.Render(line))
	}

	if len(l.Accounts) > 0 {
		lines = append(lines, "")
		lines = append(lines, styleSet.Text.Render("Profiles to use (Space to toggle):"))
		for _, acc := range l.Accounts {
			check := "[ ]"
			if acc.Selected {
				check = "[x]"
			}
			lines = append(lines, styleSet.Muted.Render(fmt.Sprintf("  %s %s", check, acc.Name)))
		}
	}

	return lines
}

func (l *Launchpad) renderSequenceStep(styleSet styles.Styles) []string {
	lines := []string{styleSet.Text.Render("Initial sequence (optional):")}
	lines = append(lines, "")

	for idx, seq := range l.Sequences {
		marker := "o"
		style := styleSet.Muted
		if idx == l.SequenceIndex {
			marker = ">"
			style = styleSet.Focus
		}
		name := seq.Name
		if seq.IsCustom {
			name = "[Custom...]"
		}
		line := fmt.Sprintf("%s %-14s %s", marker, name, seq.Description)
		lines = append(lines, style.Render(line))
	}

	return lines
}

func (l *Launchpad) renderConfirmStep(styleSet styles.Styles) []string {
	lines := []string{styleSet.Text.Render("Ready to spawn:")}
	lines = append(lines, "")

	wsDisplay := l.Config.WorkspaceName
	if wsDisplay == "" {
		wsDisplay = l.Config.WorkspacePath
	}
	lines = append(lines, styleSet.Muted.Render(fmt.Sprintf("  Workspace:  %s", wsDisplay)))
	lines = append(lines, styleSet.Muted.Render(fmt.Sprintf("  Agents:     %d x %s", l.Config.Count, l.Config.AgentType)))

	accountsDisplay := strings.Join(l.Config.SelectedAccounts, ", ")
	if accountsDisplay == "" {
		accountsDisplay = "(default)"
	}
	lines = append(lines, styleSet.Muted.Render(fmt.Sprintf("  Rotation:   %s (%s)", l.Config.RotationMode, accountsDisplay)))

	seqDisplay := l.Config.SequenceName
	if seqDisplay == "" {
		seqDisplay = "(none)"
	}
	lines = append(lines, styleSet.Muted.Render(fmt.Sprintf("  Sequence:   %s", seqDisplay)))

	return lines
}

func (l *Launchpad) renderFooter(styleSet styles.Styles) string {
	if l.CustomPathMode {
		return styleSet.Muted.Render("[Enter] Continue  [Esc] Cancel  [Backspace] Edit")
	}

	switch l.Step {
	case LaunchpadStepWorkspace:
		return styleSet.Muted.Render("[Enter] Continue  [Esc] Cancel  [j/k] Navigate")
	case LaunchpadStepCount:
		return styleSet.Muted.Render("[Enter] Continue  [Backspace] Go Back  [1-9] Presets")
	case LaunchpadStepConfirm:
		return styleSet.Muted.Render("[Enter] Spawn  [Backspace] Go Back  [Esc] Cancel")
	default:
		return styleSet.Muted.Render("[Enter] Continue  [Backspace] Go Back  [j/k] Navigate")
	}
}

// GetConfig returns the final configuration.
func (l *Launchpad) GetConfig() LaunchpadConfig {
	return l.Config
}

// StepName returns the current step name for display.
func (l *Launchpad) StepName() string {
	switch l.Step {
	case LaunchpadStepWorkspace:
		return "Workspace"
	case LaunchpadStepAgentType:
		return "Agent Type"
	case LaunchpadStepCount:
		return "Count"
	case LaunchpadStepAccounts:
		return "Accounts"
	case LaunchpadStepSequence:
		return "Sequence"
	case LaunchpadStepConfirm:
		return "Confirm"
	default:
		return "Unknown"
	}
}

func clampIndex(idx, minVal, maxVal int) int {
	if idx < minVal {
		return minVal
	}
	if idx > maxVal {
		return maxVal
	}
	return idx
}

func minInt(a, b int) int {
	if a < b {
		return a
	}
	return b
}

// SortWorkspaces sorts workspaces by name.
func SortWorkspaces(workspaces []LaunchpadWorkspace) {
	sort.Slice(workspaces, func(i, j int) bool {
		return strings.ToLower(workspaces[i].Name) < strings.ToLower(workspaces[j].Name)
	})
}
