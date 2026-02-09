package cli

import (
	"encoding/json"
	"os"
	"path/filepath"
	"runtime"
	"sort"
	"strings"
	"testing"

	"github.com/spf13/cobra"
	"github.com/spf13/pflag"
)

func TestOperationalOracleScenarioMatchesFixture(t *testing.T) {
	repo := t.TempDir()
	home := filepath.Join(repo, "home")
	t.Setenv("HOME", home)
	t.Setenv("XDG_CONFIG_HOME", filepath.Join(home, ".config"))
	t.Setenv("FMAIL_AGENT", "")

	cleanupConfig := withTempConfig(t, repo)
	defer cleanupConfig()

	withWorkingDir(t, repo, func() {
		restore := snapshotOperationalGlobals()
		defer restore()

		jsonOutput = true
		jsonlOutput = false
		quiet = true
		noColor = true
		yesFlag = true
		nonInteractive = true
		watchMode = false
		sinceDur = ""
		initForce = false
		initPromptsFrom = ""
		initNoCreatePrompt = false
		configInitForce = false

		initOut, err := captureStdout(func() error { return initCmd.RunE(initCmd, nil) })
		if err != nil {
			t.Fatalf("init: %v", err)
		}
		initPayload := decodeJSONMap(t, initOut)
		initCreated := len(jsonArray(t, initPayload, "created"))

		configPathOut, err := captureStdout(func() error { return configPathCmd.RunE(configPathCmd, nil) })
		if err != nil {
			t.Fatalf("config path: %v", err)
		}
		configPathPayload := decodeJSONMap(t, configPathOut)
		configPath := toStringValue(configPathPayload["path"])

		configInitOut, err := captureStdout(func() error { return configInitCmd.RunE(configInitCmd, nil) })
		if err != nil {
			t.Fatalf("config init: %v", err)
		}
		configInitPayload := decodeJSONMap(t, configInitOut)

		contextOut, err := captureStdout(func() error { return contextCmd.RunE(contextCmd, nil) })
		if err != nil {
			t.Fatalf("context: %v", err)
		}
		contextPayload := decodeJSONMap(t, contextOut)

		jsonOutput = false
		useShow = true
		useOut, err := captureStdout(func() error { return useCmd.RunE(useCmd, nil) })
		if err != nil {
			t.Fatalf("use --show: %v", err)
		}
		useShow = false
		jsonOutput = true

		auditOut, err := captureStdout(func() error { return auditCmd.RunE(auditCmd, nil) })
		if err != nil {
			t.Fatalf("audit: %v", err)
		}

		exportStatusOut, err := captureStdout(func() error { return exportStatusCmd.RunE(exportStatusCmd, nil) })
		if err != nil {
			t.Fatalf("export status: %v", err)
		}
		exportStatusPayload := decodeJSONMap(t, exportStatusOut)

		exportEventsOut, err := captureStdout(func() error { return exportEventsCmd.RunE(exportEventsCmd, nil) })
		if err != nil {
			t.Fatalf("export events: %v", err)
		}

		statusOut, err := captureStdout(func() error { return statusCmd.RunE(statusCmd, nil) })
		if err != nil {
			t.Fatalf("status: %v", err)
		}
		statusPayload := decodeJSONMap(t, statusOut)

		doctorOut, err := captureStdout(func() error { return doctorCmd.RunE(doctorCmd, nil) })
		if err != nil {
			t.Fatalf("doctor: %v", err)
		}
		doctorPayload := decodeJSONMap(t, doctorOut)

		waitUntil = "definitely-invalid"
		waitErr := waitCmd.RunE(waitCmd, nil)

		explainErr := explainCmd.RunE(explainCmd, nil)
		hookErr := hookOnEventCmd.RunE(hookOnEventCmd, nil)
		lockErr := lockClaimCmd.RunE(lockClaimCmd, nil)

		jsonOutput = false
		completionOut, err := captureStdout(func() error {
			return completionCmd.RunE(completionCmd, []string{"bash"})
		})
		if err != nil {
			t.Fatalf("completion bash: %v", err)
		}
		jsonOutput = true

		summary := map[string]any{
			"command_shapes": map[string]any{
				"audit":      commandShape(auditCmd),
				"completion": commandShape(completionCmd),
				"config":     commandShape(configCmd),
				"context":    commandShape(contextCmd),
				"doctor":     commandShape(doctorCmd),
				"explain":    commandShape(explainCmd),
				"export":     commandShape(exportCmd),
				"hook":       commandShape(hookCmd),
				"init":       commandShape(initCmd),
				"lock":       commandShape(lockCmd),
				"skills":     commandShape(skillsCmd),
				"status":     commandShape(statusCmd),
				"use":        commandShape(useCmd),
				"wait":       commandShape(waitCmd),
			},
			"runtime_smoke": map[string]any{
				"audit_json_kind":                jsonKind(t, auditOut),
				"completion_bash_has_start":      strings.Contains(completionOut, "__start_forge"),
				"config_init_created":            boolValue(configInitPayload["created"]),
				"config_path_suffix":             strings.TrimPrefix(configPath, home),
				"context_keys":                   sortedMapKeys(contextPayload),
				"doctor_keys":                    sortedMapKeys(doctorPayload),
				"doctor_summary_keys":            sortedNestedMapKeys(t, doctorPayload, "summary"),
				"doctor_unique_categories":       doctorCategories(t, doctorPayload),
				"explain_no_context_error":       errorString(explainErr),
				"export_events_json_kind":        jsonKind(t, exportEventsOut),
				"export_status_keys":             sortedMapKeys(exportStatusPayload),
				"hook_missing_target_error":      errorString(hookErr),
				"init_created_count":             initCreated,
				"lock_claim_missing_path_error":  errorString(lockErr),
				"status_keys":                    sortedMapKeys(statusPayload),
				"use_show_reports_empty_context": strings.Contains(useOut, "No context set."),
				"wait_invalid_condition_error":   errorString(waitErr),
			},
		}

		got := prettyJSON(t, summary)
		if maybeUpdateOperationalFixture(t, got) {
			return
		}
		want := readOperationalFixture(t)
		if got != want {
			t.Fatalf("operational oracle fixture drift\nwant:\n%s\ngot:\n%s", want, got)
		}
	})
}

func commandShape(cmd *cobra.Command) map[string]any {
	subcommands := make([]string, 0, len(cmd.Commands()))
	for _, sub := range cmd.Commands() {
		if sub.Hidden {
			continue
		}
		subcommands = append(subcommands, sub.Name())
	}
	sort.Strings(subcommands)

	flags := make([]string, 0)
	cmd.Flags().VisitAll(func(flag *pflag.Flag) {
		flags = append(flags, flag.Name)
	})
	sort.Strings(flags)

	return map[string]any{
		"use":         cmd.Use,
		"subcommands": subcommands,
		"flags":       flags,
	}
}

func jsonKind(t *testing.T, raw string) string {
	t.Helper()
	trimmed := strings.TrimSpace(raw)
	var value any
	if err := json.Unmarshal([]byte(trimmed), &value); err != nil {
		t.Fatalf("decode json kind: %v\nraw:\n%s", err, raw)
	}
	switch value.(type) {
	case []any:
		return "array"
	case map[string]any:
		return "object"
	default:
		return "other"
	}
}

func jsonArray(t *testing.T, payload map[string]any, key string) []any {
	t.Helper()
	raw, ok := payload[key]
	if !ok {
		return nil
	}
	values, ok := raw.([]any)
	if !ok {
		t.Fatalf("expected %s to be []any, got %T", key, raw)
	}
	return values
}

func sortedMapKeys(payload map[string]any) []string {
	keys := make([]string, 0, len(payload))
	for key := range payload {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}

func sortedNestedMapKeys(t *testing.T, payload map[string]any, key string) []string {
	t.Helper()
	raw, ok := payload[key]
	if !ok {
		return nil
	}
	nested, ok := raw.(map[string]any)
	if !ok {
		t.Fatalf("expected nested map at %s, got %T", key, raw)
	}
	return sortedMapKeys(nested)
}

func doctorCategories(t *testing.T, payload map[string]any) []string {
	t.Helper()
	rawChecks, ok := payload["checks"]
	if !ok {
		return nil
	}
	checks, ok := rawChecks.([]any)
	if !ok {
		t.Fatalf("expected checks []any, got %T", rawChecks)
	}
	set := map[string]struct{}{}
	for _, check := range checks {
		row, ok := check.(map[string]any)
		if !ok {
			t.Fatalf("expected check row map, got %T", check)
		}
		category := toStringValue(row["category"])
		if category != "" {
			set[category] = struct{}{}
		}
	}
	categories := make([]string, 0, len(set))
	for category := range set {
		categories = append(categories, category)
	}
	sort.Strings(categories)
	return categories
}

func snapshotOperationalGlobals() func() {
	prev := struct {
		jsonOutput         bool
		jsonlOutput        bool
		quiet              bool
		noColor            bool
		yesFlag            bool
		nonInteractive     bool
		watchMode          bool
		sinceDur           string
		useAgent           string
		useWorkspace       string
		useClear           bool
		useShow            bool
		waitUntil          string
		waitAgent          string
		waitWorkspace      string
		waitQuiet          bool
		hookCommand        string
		hookURL            string
		hookHeaders        []string
		hookTypes          string
		hookEntity         string
		hookEntityID       string
		hookTimeout        string
		hookDisabled       bool
		lockClaimAgent     string
		lockClaimPaths     []string
		lockClaimReason    string
		lockClaimForce     bool
		configInitForce    bool
		initForce          bool
		initPromptsFrom    string
		initNoCreatePrompt bool
	}{
		jsonOutput:         jsonOutput,
		jsonlOutput:        jsonlOutput,
		quiet:              quiet,
		noColor:            noColor,
		yesFlag:            yesFlag,
		nonInteractive:     nonInteractive,
		watchMode:          watchMode,
		sinceDur:           sinceDur,
		useAgent:           useAgent,
		useWorkspace:       useWorkspace,
		useClear:           useClear,
		useShow:            useShow,
		waitUntil:          waitUntil,
		waitAgent:          waitAgent,
		waitWorkspace:      waitWorkspace,
		waitQuiet:          waitQuiet,
		hookCommand:        hookCommand,
		hookURL:            hookURL,
		hookHeaders:        append([]string(nil), hookHeaders...),
		hookTypes:          hookTypes,
		hookEntity:         hookEntity,
		hookEntityID:       hookEntityID,
		hookTimeout:        hookTimeout,
		hookDisabled:       hookDisabled,
		lockClaimAgent:     lockClaimAgent,
		lockClaimPaths:     append([]string(nil), lockClaimPaths...),
		lockClaimReason:    lockClaimReason,
		lockClaimForce:     lockClaimForce,
		configInitForce:    configInitForce,
		initForce:          initForce,
		initPromptsFrom:    initPromptsFrom,
		initNoCreatePrompt: initNoCreatePrompt,
	}

	return func() {
		jsonOutput = prev.jsonOutput
		jsonlOutput = prev.jsonlOutput
		quiet = prev.quiet
		noColor = prev.noColor
		yesFlag = prev.yesFlag
		nonInteractive = prev.nonInteractive
		watchMode = prev.watchMode
		sinceDur = prev.sinceDur
		useAgent = prev.useAgent
		useWorkspace = prev.useWorkspace
		useClear = prev.useClear
		useShow = prev.useShow
		waitUntil = prev.waitUntil
		waitAgent = prev.waitAgent
		waitWorkspace = prev.waitWorkspace
		waitQuiet = prev.waitQuiet
		hookCommand = prev.hookCommand
		hookURL = prev.hookURL
		hookHeaders = append([]string(nil), prev.hookHeaders...)
		hookTypes = prev.hookTypes
		hookEntity = prev.hookEntity
		hookEntityID = prev.hookEntityID
		hookTimeout = prev.hookTimeout
		hookDisabled = prev.hookDisabled
		lockClaimAgent = prev.lockClaimAgent
		lockClaimPaths = append([]string(nil), prev.lockClaimPaths...)
		lockClaimReason = prev.lockClaimReason
		lockClaimForce = prev.lockClaimForce
		configInitForce = prev.configInitForce
		initForce = prev.initForce
		initPromptsFrom = prev.initPromptsFrom
		initNoCreatePrompt = prev.initNoCreatePrompt
	}
}

func maybeUpdateOperationalFixture(t *testing.T, body string) bool {
	t.Helper()
	if os.Getenv("FORGE_UPDATE_GOLDENS") != "1" {
		return false
	}

	for _, path := range operationalFixturePaths(t) {
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatalf("mkdir fixture dir: %v", err)
		}
		if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
			t.Fatalf("write fixture %s: %v", path, err)
		}
	}
	return true
}

func readOperationalFixture(t *testing.T) string {
	t.Helper()
	paths := operationalFixturePaths(t)
	data, err := os.ReadFile(paths[0])
	if err != nil {
		t.Fatalf("read operational fixture: %v", err)
	}
	return strings.TrimSpace(string(data))
}

func operationalFixturePaths(t *testing.T) []string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatalf("resolve caller path")
	}
	base := filepath.Dir(file)
	return []string{
		filepath.Join(base, "..", "parity", "testdata", "oracle", "expected", "forge", "operational", "summary.json"),
		filepath.Join(base, "..", "parity", "testdata", "oracle", "actual", "forge", "operational", "summary.json"),
	}
}
