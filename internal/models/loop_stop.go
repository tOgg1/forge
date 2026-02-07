package models

// LoopStopConfig configures optional "smart stop" behavior for a loop.
//
// Stored inside Loop.Metadata as JSON under the "stop_config" key.
type LoopStopConfig struct {
	Quant *LoopQuantStopConfig `json:"quant,omitempty"`
	Qual  *LoopQualStopConfig  `json:"qual,omitempty"`
}

// LoopQuantStopConfig runs a shell command and matches its exit code/stdout/stderr.
type LoopQuantStopConfig struct {
	// Cmd is executed via `bash -lc <cmd>` with workdir set to the repo root.
	Cmd string `json:"cmd,omitempty"`

	// EveryN controls cadence (<= 0 disables).
	EveryN int `json:"every_n,omitempty"`

	// When controls whether to evaluate before/after a run: "before", "after", "both".
	When string `json:"when,omitempty"`

	// Decision is applied when the match criteria are satisfied: "stop" or "continue".
	Decision string `json:"decision,omitempty"`

	// ExitCodes matches when the command exits with any of these codes (empty = ignore).
	ExitCodes []int `json:"exit_codes,omitempty"`

	// ExitInvert inverts ExitCodes matching (i.e. match when exit not in ExitCodes).
	ExitInvert bool `json:"exit_invert,omitempty"`

	// StdoutMode and StderrMode: "any", "empty", "nonempty".
	StdoutMode string `json:"stdout_mode,omitempty"`
	StderrMode string `json:"stderr_mode,omitempty"`

	// StdoutRegex/StderrRegex are RE2 regexes applied to stdout/stderr (empty = ignore).
	StdoutRegex string `json:"stdout_regex,omitempty"`
	StderrRegex string `json:"stderr_regex,omitempty"`

	// TimeoutSeconds caps command runtime (0 = no extra timeout).
	TimeoutSeconds int `json:"timeout_seconds,omitempty"`
}

// LoopQualStopConfig triggers a specialized "judge" iteration where the agent must output 0 or 1.
type LoopQualStopConfig struct {
	// EveryN controls cadence in terms of main iterations (<= 0 disables).
	EveryN int `json:"every_n,omitempty"`

	// Prompt defines the qualitative prompt for the judge iteration.
	Prompt NextPromptOverridePayload `json:"prompt"`

	// OnInvalid controls behavior if judge output is not "0" or "1": "stop" or "continue".
	OnInvalid string `json:"on_invalid,omitempty"`
}

// LoopStopState tracks runtime counters for stop behavior.
//
// Stored inside Loop.Metadata as JSON under the "stop_state" key.
type LoopStopState struct {
	MainIterationCount int `json:"main_iteration_count,omitempty"`
	QualIterationCount int `json:"qual_iteration_count,omitempty"`

	// QualLastMainCount is the main iteration count we last ran a qualitative check for.
	QualLastMainCount int `json:"qual_last_main_count,omitempty"`
}
