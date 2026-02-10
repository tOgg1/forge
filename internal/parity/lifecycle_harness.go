package parity

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

// LifecycleScenario describes a sequence of CLI commands to compare.
type LifecycleScenario struct {
	Name  string            `json:"name,omitempty"`
	Env   map[string]string `json:"env,omitempty"`
	Steps []LifecycleStep   `json:"steps"`
}

// LifecycleStep describes one command invocation.
type LifecycleStep struct {
	Name         string   `json:"name"`
	Args         []string `json:"args"`
	StdoutFormat Format   `json:"stdout_format,omitempty"`
	StderrFormat Format   `json:"stderr_format,omitempty"`
}

// LifecycleHarnessConfig configures side-by-side CLI execution.
type LifecycleHarnessConfig struct {
	GoBinary   string
	RustBinary string
	FixtureDir string
	Scenario   LifecycleScenario
	ExtraEnv   map[string]string
	Timeout    time.Duration
}

// LifecycleCommandResult captures one command execution.
type LifecycleCommandResult struct {
	ExitCode int    `json:"exit_code"`
	Stdout   string `json:"stdout"`
	Stderr   string `json:"stderr"`
}

// StreamComparison captures normalized comparison output for one stream.
type StreamComparison struct {
	Equal          bool   `json:"equal"`
	GoNormalized   string `json:"go_normalized"`
	RustNormalized string `json:"rust_normalized"`
}

// LifecycleStepReport captures parity for one step.
type LifecycleStepReport struct {
	Name          string                 `json:"name"`
	Args          []string               `json:"args"`
	Go            LifecycleCommandResult `json:"go"`
	Rust          LifecycleCommandResult `json:"rust"`
	ExitCodeMatch bool                   `json:"exit_code_match"`
	Stdout        StreamComparison       `json:"stdout"`
	Stderr        StreamComparison       `json:"stderr"`
	HasDrift      bool                   `json:"has_drift"`
}

// LifecycleHarnessReport is the full run output.
type LifecycleHarnessReport struct {
	Scenario    string                `json:"scenario"`
	GoBinary    string                `json:"go_binary"`
	RustBinary  string                `json:"rust_binary"`
	FixtureDir  string                `json:"fixture_dir,omitempty"`
	GeneratedAt string                `json:"generated_at"`
	Steps       []LifecycleStepReport `json:"steps"`
}

// HasDrift reports whether any step contains parity drift.
func (r LifecycleHarnessReport) HasDrift() bool {
	for _, step := range r.Steps {
		if step.HasDrift {
			return true
		}
	}
	return false
}

// DriftCount returns number of drifted steps.
func (r LifecycleHarnessReport) DriftCount() int {
	count := 0
	for _, step := range r.Steps {
		if step.HasDrift {
			count++
		}
	}
	return count
}

// LoadLifecycleScenario reads and validates a scenario file.
func LoadLifecycleScenario(path string) (LifecycleScenario, error) {
	body, err := os.ReadFile(path)
	if err != nil {
		return LifecycleScenario{}, err
	}
	var scenario LifecycleScenario
	if err := json.Unmarshal(body, &scenario); err != nil {
		return LifecycleScenario{}, err
	}
	if err := validateLifecycleScenario(scenario); err != nil {
		return LifecycleScenario{}, err
	}
	return scenario, nil
}

// WriteLifecycleHarnessReport writes an indented JSON report.
func WriteLifecycleHarnessReport(path string, report LifecycleHarnessReport) error {
	body, err := json.MarshalIndent(report, "", "  ")
	if err != nil {
		return err
	}
	body = append(body, '\n')
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	return os.WriteFile(path, body, 0o644)
}

// RunLoopLifecycleHarness executes a scenario against Go and Rust binaries and compares normalized outputs.
func RunLoopLifecycleHarness(ctx context.Context, cfg LifecycleHarnessConfig) (LifecycleHarnessReport, error) {
	if err := validateHarnessConfig(cfg); err != nil {
		return LifecycleHarnessReport{}, err
	}

	tempRoot, err := os.MkdirTemp("", "forge-loop-lifecycle-harness-*")
	if err != nil {
		return LifecycleHarnessReport{}, err
	}
	defer os.RemoveAll(tempRoot)

	goDir := filepath.Join(tempRoot, "go-fixture")
	rustDir := filepath.Join(tempRoot, "rust-fixture")
	if err := os.MkdirAll(goDir, 0o755); err != nil {
		return LifecycleHarnessReport{}, err
	}
	if err := os.MkdirAll(rustDir, 0o755); err != nil {
		return LifecycleHarnessReport{}, err
	}
	if cfg.FixtureDir != "" {
		if err := copyTree(cfg.FixtureDir, goDir); err != nil {
			return LifecycleHarnessReport{}, fmt.Errorf("copy fixture for go: %w", err)
		}
		if err := copyTree(cfg.FixtureDir, rustDir); err != nil {
			return LifecycleHarnessReport{}, fmt.Errorf("copy fixture for rust: %w", err)
		}
	}

	env := buildHarnessEnv(cfg.Scenario.Env, cfg.ExtraEnv)
	report := LifecycleHarnessReport{
		Scenario:    cfg.Scenario.Name,
		GoBinary:    cfg.GoBinary,
		RustBinary:  cfg.RustBinary,
		FixtureDir:  cfg.FixtureDir,
		GeneratedAt: time.Now().UTC().Format(time.RFC3339),
		Steps:       make([]LifecycleStepReport, 0, len(cfg.Scenario.Steps)),
	}

	for _, step := range cfg.Scenario.Steps {
		goResult, err := runHarnessCommand(ctx, cfg.Timeout, cfg.GoBinary, step.Args, goDir, env)
		if err != nil {
			return LifecycleHarnessReport{}, fmt.Errorf("go step %q: %w", step.Name, err)
		}
		rustResult, err := runHarnessCommand(ctx, cfg.Timeout, cfg.RustBinary, step.Args, rustDir, env)
		if err != nil {
			return LifecycleHarnessReport{}, fmt.Errorf("rust step %q: %w", step.Name, err)
		}

		stdoutCmp, err := compareStreams(goResult.Stdout, rustResult.Stdout, normalizeStepFormat(step.StdoutFormat))
		if err != nil {
			return LifecycleHarnessReport{}, fmt.Errorf("compare stdout for step %q: %w", step.Name, err)
		}
		stderrCmp, err := compareStreams(goResult.Stderr, rustResult.Stderr, normalizeStepFormat(step.StderrFormat))
		if err != nil {
			return LifecycleHarnessReport{}, fmt.Errorf("compare stderr for step %q: %w", step.Name, err)
		}

		stepReport := LifecycleStepReport{
			Name:          step.Name,
			Args:          append([]string(nil), step.Args...),
			Go:            goResult,
			Rust:          rustResult,
			ExitCodeMatch: goResult.ExitCode == rustResult.ExitCode,
			Stdout:        stdoutCmp,
			Stderr:        stderrCmp,
		}
		stepReport.HasDrift = !stepReport.ExitCodeMatch || !stepReport.Stdout.Equal || !stepReport.Stderr.Equal
		report.Steps = append(report.Steps, stepReport)
	}

	return report, nil
}

func validateHarnessConfig(cfg LifecycleHarnessConfig) error {
	if strings.TrimSpace(cfg.GoBinary) == "" {
		return errors.New("go binary path is required")
	}
	if strings.TrimSpace(cfg.RustBinary) == "" {
		return errors.New("rust binary path is required")
	}
	if err := validateLifecycleScenario(cfg.Scenario); err != nil {
		return err
	}
	return nil
}

func validateLifecycleScenario(scenario LifecycleScenario) error {
	if len(scenario.Steps) == 0 {
		return errors.New("scenario must include at least one step")
	}
	for i, step := range scenario.Steps {
		if strings.TrimSpace(step.Name) == "" {
			return fmt.Errorf("step %d: name is required", i)
		}
		if len(step.Args) == 0 {
			return fmt.Errorf("step %d (%s): args are required", i, step.Name)
		}
		if !isValidFormat(normalizeStepFormat(step.StdoutFormat)) {
			return fmt.Errorf("step %d (%s): invalid stdout_format %q", i, step.Name, step.StdoutFormat)
		}
		if !isValidFormat(normalizeStepFormat(step.StderrFormat)) {
			return fmt.Errorf("step %d (%s): invalid stderr_format %q", i, step.Name, step.StderrFormat)
		}
	}
	return nil
}

func isValidFormat(format Format) bool {
	return format == FormatText || format == FormatJSON
}

func normalizeStepFormat(format Format) Format {
	if strings.TrimSpace(string(format)) == "" {
		return FormatText
	}
	return format
}

func buildHarnessEnv(scenarioEnv, extraEnv map[string]string) []string {
	env := make(map[string]string)
	for _, raw := range os.Environ() {
		parts := strings.SplitN(raw, "=", 2)
		key := parts[0]
		value := ""
		if len(parts) == 2 {
			value = parts[1]
		}
		env[key] = value
	}

	// Keep command output stable across shells/runners.
	env["FORGE_NON_INTERACTIVE"] = "1"
	env["NO_COLOR"] = "1"

	for key, value := range scenarioEnv {
		env[key] = value
	}
	for key, value := range extraEnv {
		env[key] = value
	}

	keys := make([]string, 0, len(env))
	for key := range env {
		keys = append(keys, key)
	}
	sort.Strings(keys)

	out := make([]string, 0, len(keys))
	for _, key := range keys {
		out = append(out, key+"="+env[key])
	}
	return out
}

func runHarnessCommand(ctx context.Context, timeout time.Duration, binary string, args []string, workdir string, env []string) (LifecycleCommandResult, error) {
	runCtx := ctx
	cancel := func() {}
	if timeout > 0 {
		runCtx, cancel = context.WithTimeout(ctx, timeout)
	}
	defer cancel()

	cmd := exec.CommandContext(runCtx, binary, args...)
	cmd.Dir = workdir
	cmd.Env = append([]string(nil), env...)

	var stdout bytes.Buffer
	var stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	result := LifecycleCommandResult{}
	err := cmd.Run()
	result.Stdout = stdout.String()
	result.Stderr = stderr.String()

	if err == nil {
		result.ExitCode = 0
		return result, nil
	}

	var exitErr *exec.ExitError
	if errors.As(err, &exitErr) {
		result.ExitCode = exitErr.ExitCode()
		return result, nil
	}
	if runCtx.Err() != nil {
		return result, fmt.Errorf("run command %q timed out/canceled: %w", strings.Join(append([]string{binary}, args...), " "), runCtx.Err())
	}
	return result, fmt.Errorf("run command %q: %w", strings.Join(append([]string{binary}, args...), " "), err)
}

func compareStreams(goOut, rustOut string, format Format) (StreamComparison, error) {
	comparison, err := CompareBytes([]byte(goOut), []byte(rustOut), DefaultCompareOptions(format))
	if err != nil {
		return StreamComparison{}, err
	}
	return StreamComparison{
		Equal:          comparison.Equal,
		GoNormalized:   string(comparison.NormalizedExpected),
		RustNormalized: string(comparison.NormalizedActual),
	}, nil
}
