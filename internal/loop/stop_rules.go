package loop

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os/exec"
	"regexp"
	"strings"
	"time"

	"github.com/tOgg1/forge/internal/models"
)

const (
	loopStopConfigKey = "stop_config"
	loopStopStateKey  = "stop_state"

	stopWhenBefore = "before"
	stopWhenAfter  = "after"
	stopWhenBoth   = "both"

	stopDecisionStop     = "stop"
	stopDecisionContinue = "continue"
)

type commandResult struct {
	exitCode int
	stdout   string
	stderr   string
	err      error
}

type runCommandFunc func(ctx context.Context, workDir, cmd string, timeout time.Duration) commandResult

func defaultRunCommand(ctx context.Context, workDir, cmd string, timeout time.Duration) commandResult {
	if strings.TrimSpace(cmd) == "" {
		return commandResult{exitCode: -1, err: errors.New("empty command")}
	}

	runCtx := ctx
	var cancel context.CancelFunc
	if timeout > 0 {
		runCtx, cancel = context.WithTimeout(ctx, timeout)
		defer cancel()
	}

	c := exec.CommandContext(runCtx, "bash", "-lc", cmd)
	c.Dir = workDir

	var stdout bytes.Buffer
	var stderr bytes.Buffer
	c.Stdout = &stdout
	c.Stderr = &stderr

	err := c.Run()
	exitCode := exitCodeFromError(err)

	// If context timed out/canceled, preserve exit code (-1) rather than the child code.
	if runCtx.Err() != nil {
		exitCode = -1
	}

	return commandResult{
		exitCode: exitCode,
		stdout:   stdout.String(),
		stderr:   stderr.String(),
		err:      err,
	}
}

func loadStopConfig(loopEntry *models.Loop) (models.LoopStopConfig, bool) {
	if loopEntry == nil || loopEntry.Metadata == nil {
		return models.LoopStopConfig{}, false
	}
	raw, ok := loopEntry.Metadata[loopStopConfigKey]
	if !ok || raw == nil {
		return models.LoopStopConfig{}, false
	}
	data, err := json.Marshal(raw)
	if err != nil {
		return models.LoopStopConfig{}, false
	}
	var cfg models.LoopStopConfig
	if err := json.Unmarshal(data, &cfg); err != nil {
		return models.LoopStopConfig{}, false
	}
	return cfg, true
}

func loadStopState(loopEntry *models.Loop) models.LoopStopState {
	if loopEntry == nil || loopEntry.Metadata == nil {
		return models.LoopStopState{}
	}
	raw, ok := loopEntry.Metadata[loopStopStateKey]
	if !ok || raw == nil {
		return models.LoopStopState{}
	}
	data, err := json.Marshal(raw)
	if err != nil {
		return models.LoopStopState{}
	}
	var state models.LoopStopState
	if err := json.Unmarshal(data, &state); err != nil {
		return models.LoopStopState{}
	}
	return state
}

func saveStopState(loopEntry *models.Loop, state models.LoopStopState) {
	if loopEntry == nil {
		return
	}
	if loopEntry.Metadata == nil {
		loopEntry.Metadata = make(map[string]any)
	}
	loopEntry.Metadata[loopStopStateKey] = state
}

func resetStopState(loopEntry *models.Loop) {
	saveStopState(loopEntry, models.LoopStopState{})
}

func quantWhenMatches(when string, afterRun bool) bool {
	when = strings.ToLower(strings.TrimSpace(when))
	if when == "" {
		when = stopWhenBefore
	}
	switch when {
	case stopWhenBefore:
		return !afterRun
	case stopWhenAfter:
		return afterRun
	case stopWhenBoth:
		return true
	default:
		// default to before
		return !afterRun
	}
}

func quantEveryMatches(everyN int, iterationIndex int) bool {
	if everyN <= 0 {
		return false
	}
	if iterationIndex <= 0 {
		return false
	}
	return iterationIndex%everyN == 0
}

func normalizeDecision(value string) string {
	switch strings.ToLower(strings.TrimSpace(value)) {
	case stopDecisionContinue:
		return stopDecisionContinue
	default:
		return stopDecisionStop
	}
}

func normalizeStreamMode(value string) string {
	switch strings.ToLower(strings.TrimSpace(value)) {
	case "empty", "nonempty", "any":
		return strings.ToLower(strings.TrimSpace(value))
	default:
		return "any"
	}
}

func matchesStreamMode(mode string, s string) bool {
	mode = normalizeStreamMode(mode)
	empty := strings.TrimSpace(s) == ""
	switch mode {
	case "empty":
		return empty
	case "nonempty":
		return !empty
	default:
		return true
	}
}

func matchExitCodes(exitCode int, codes []int, invert bool) bool {
	if len(codes) == 0 {
		return true
	}
	in := false
	for _, c := range codes {
		if exitCode == c {
			in = true
			break
		}
	}
	if invert {
		return !in
	}
	return in
}

func compileRegex(pattern string) (*regexp.Regexp, error) {
	pattern = strings.TrimSpace(pattern)
	if pattern == "" {
		return nil, nil
	}
	return regexp.Compile(pattern)
}

func quantRuleMatches(cfg models.LoopQuantStopConfig, res commandResult) (bool, string) {
	if strings.TrimSpace(cfg.Cmd) == "" {
		return false, "empty cmd"
	}

	stdoutMode := normalizeStreamMode(cfg.StdoutMode)
	stderrMode := normalizeStreamMode(cfg.StderrMode)
	noExit := len(cfg.ExitCodes) == 0
	noStreamMode := stdoutMode == "any" && stderrMode == "any"
	noRegex := strings.TrimSpace(cfg.StdoutRegex) == "" && strings.TrimSpace(cfg.StderrRegex) == ""
	if noExit && noStreamMode && noRegex {
		return false, "no match criteria configured"
	}

	if !matchExitCodes(res.exitCode, cfg.ExitCodes, cfg.ExitInvert) {
		return false, fmt.Sprintf("exit_code=%d not matched", res.exitCode)
	}
	if !matchesStreamMode(stdoutMode, res.stdout) {
		return false, fmt.Sprintf("stdout_mode=%s not matched", stdoutMode)
	}
	if !matchesStreamMode(stderrMode, res.stderr) {
		return false, fmt.Sprintf("stderr_mode=%s not matched", stderrMode)
	}

	stdoutRE, err := compileRegex(cfg.StdoutRegex)
	if err != nil {
		return false, fmt.Sprintf("invalid stdout_regex: %v", err)
	}
	if stdoutRE != nil && !stdoutRE.MatchString(res.stdout) {
		return false, "stdout_regex not matched"
	}

	stderrRE, err := compileRegex(cfg.StderrRegex)
	if err != nil {
		return false, fmt.Sprintf("invalid stderr_regex: %v", err)
	}
	if stderrRE != nil && !stderrRE.MatchString(res.stderr) {
		return false, "stderr_regex not matched"
	}

	return true, "matched"
}

func parseQualSignal(output string) (int, bool) {
	fields := strings.Fields(output)
	if len(fields) == 0 {
		return 0, false
	}
	switch fields[0] {
	case "0":
		return 0, true
	case "1":
		return 1, true
	default:
		return 0, false
	}
}

func qualDue(cfg *models.LoopQualStopConfig, state models.LoopStopState) bool {
	if cfg == nil || cfg.EveryN <= 0 {
		return false
	}
	// Trigger based on completed main iterations (not total iterations).
	if state.MainIterationCount <= 0 {
		return false
	}
	if state.MainIterationCount%cfg.EveryN != 0 {
		return false
	}
	// Run at most once per milestone.
	return state.QualLastMainCount != state.MainIterationCount
}

func normalizeOnInvalid(value string) string {
	switch strings.ToLower(strings.TrimSpace(value)) {
	case stopDecisionStop:
		return stopDecisionStop
	default:
		return stopDecisionContinue
	}
}
