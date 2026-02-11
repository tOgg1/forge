package cli

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"runtime"
	"sort"
	"strings"
	"testing"

	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/models"
)

type configFamiliesOracleReport struct {
	Steps []configFamiliesOracleStep `json:"steps"`
}

type configFamiliesOracleStep struct {
	Name   string                    `json:"name"`
	Stdout string                    `json:"stdout,omitempty"`
	Stderr string                    `json:"stderr,omitempty"`
	State  configFamiliesOracleState `json:"state"`
}

type configFamiliesOracleState struct {
	Profiles  []configFamiliesProfileState `json:"profiles,omitempty"`
	Pools     []configFamiliesPoolState    `json:"pools,omitempty"`
	Prompts   []string                     `json:"prompts,omitempty"`
	Templates []string                     `json:"templates,omitempty"`
	Sequences []string                     `json:"sequences,omitempty"`
	Queue     []configFamiliesQueueState   `json:"queue,omitempty"`
}

type configFamiliesProfileState struct {
	Name           string            `json:"name"`
	Harness        models.Harness    `json:"harness"`
	AuthKind       string            `json:"auth_kind,omitempty"`
	AuthHome       string            `json:"auth_home,omitempty"`
	PromptMode     models.PromptMode `json:"prompt_mode,omitempty"`
	Command        string            `json:"command,omitempty"`
	Model          string            `json:"model,omitempty"`
	ExtraArgs      []string          `json:"extra_args,omitempty"`
	Env            map[string]string `json:"env,omitempty"`
	MaxConcurrency int               `json:"max_concurrency"`
	Cooldown       bool              `json:"cooldown"`
}

type configFamiliesPoolState struct {
	Name     string              `json:"name"`
	Strategy models.PoolStrategy `json:"strategy"`
	Default  bool                `json:"default"`
	Members  []string            `json:"members,omitempty"`
}

type configFamiliesQueueState struct {
	Type     models.QueueItemType   `json:"type"`
	Status   models.QueueItemStatus `json:"status"`
	Position int                    `json:"position"`
	Message  string                 `json:"message,omitempty"`
}

func TestOracleConfigFamiliesFixtures(t *testing.T) {
	if testing.Short() {
		t.Skip("oracle fixtures are integration-style; skip in -short")
	}

	repo := t.TempDir()
	homeDir := filepath.Join(repo, "home")
	if err := os.MkdirAll(homeDir, 0o755); err != nil {
		t.Fatalf("mkdir home: %v", err)
	}

	cleanupConfig := withTempConfig(t, repo)
	defer cleanupConfig()

	withWorkingDir(t, repo, func() {
		restoreCLI := snapshotCLIFlags()
		defer restoreCLI()
		defer resetConfigFamiliesCommandState()

		t.Setenv("HOME", homeDir)
		t.Setenv("XDG_CONFIG_HOME", "")
		t.Setenv("EDITOR", "true")
		t.Setenv("FMAIL_AGENT", "")
		t.Setenv("FORGE_LOOP_ID", "")

		jsonOutput = true
		jsonlOutput = false
		quiet = false
		noColor = true
		yesFlag = true
		nonInteractive = true

		seedConfigFamiliesOracleDB(t, repo)
		writeConfigFamiliesPromptSource(t, repo)

		var report configFamiliesOracleReport

		recordConfigFamiliesStep(t, &report, "profile add alpha", repo, homeDir, func() error {
			resetConfigFamiliesProfileFlags()
			profileAddName = "alpha"
			profileAddAuthKind = "codex"
			profileAddAuthHome = "/tmp/oracle-auth-alpha"
			profileAddPromptMode = string(models.PromptModeEnv)
			profileAddCommand = "codex exec"
			profileAddModel = "gpt-5"
			profileAddExtraArgs = []string{"--sandbox", "workspace-write"}
			profileAddEnv = []string{"A=1"}
			profileAddMaxConcurrency = 2
			return profileAddCmd.RunE(profileAddCmd, []string{"codex"})
		})

		recordConfigFamiliesStep(t, &report, "profile add beta", repo, homeDir, func() error {
			resetConfigFamiliesProfileFlags()
			profileAddName = "beta"
			profileAddAuthKind = "claude"
			profileAddAuthHome = "/tmp/oracle-auth-beta"
			profileAddPromptMode = string(models.PromptModeStdin)
			profileAddCommand = "claude --print"
			profileAddModel = "sonnet"
			profileAddMaxConcurrency = 1
			return profileAddCmd.RunE(profileAddCmd, []string{"claude"})
		})

		recordConfigFamiliesStep(t, &report, "profile add gamma", repo, homeDir, func() error {
			resetConfigFamiliesProfileFlags()
			profileAddName = "gamma"
			profileAddAuthKind = "opencode"
			profileAddPromptMode = string(models.PromptModePath)
			profileAddCommand = "opencode run"
			profileAddModel = "default"
			return profileAddCmd.RunE(profileAddCmd, []string{"opencode"})
		})

		recordConfigFamiliesStep(t, &report, "profile edit alpha --model", repo, homeDir, func() error {
			resetConfigFamiliesProfileFlags()
			clearConfigFamiliesProfileEditChanged()
			if err := profileEditCmd.Flags().Set("model", "gpt-5.1"); err != nil {
				return err
			}
			defer clearConfigFamiliesProfileEditChanged()
			return profileEditCmd.RunE(profileEditCmd, []string{"alpha"})
		})

		recordConfigFamiliesStep(t, &report, "profile cooldown set alpha", repo, homeDir, func() error {
			resetConfigFamiliesProfileFlags()
			profileCooldownUntil = "2026-01-01T00:00:00Z"
			return profileCooldownSetCmd.RunE(profileCooldownSetCmd, []string{"alpha"})
		})

		recordConfigFamiliesStep(t, &report, "profile cooldown clear alpha", repo, homeDir, func() error {
			resetConfigFamiliesProfileFlags()
			return profileCooldownClearCmd.RunE(profileCooldownClearCmd, []string{"alpha"})
		})

		recordConfigFamiliesStep(t, &report, "profile ls", repo, homeDir, func() error {
			resetConfigFamiliesProfileFlags()
			return profileListCmd.RunE(profileListCmd, nil)
		})

		recordConfigFamiliesStep(t, &report, "profile rm gamma", repo, homeDir, func() error {
			resetConfigFamiliesProfileFlags()
			return profileRemoveCmd.RunE(profileRemoveCmd, []string{"gamma"})
		})

		recordConfigFamiliesStep(t, &report, "pool create primary", repo, homeDir, func() error {
			resetConfigFamiliesPoolFlags()
			return poolCreateCmd.RunE(poolCreateCmd, []string{"primary"})
		})

		recordConfigFamiliesStep(t, &report, "pool add primary alpha beta", repo, homeDir, func() error {
			resetConfigFamiliesPoolFlags()
			return poolAddCmd.RunE(poolAddCmd, []string{"primary", "alpha", "beta"})
		})

		recordConfigFamiliesStep(t, &report, "pool show primary", repo, homeDir, func() error {
			resetConfigFamiliesPoolFlags()
			return poolShowCmd.RunE(poolShowCmd, []string{"primary"})
		})

		recordConfigFamiliesStep(t, &report, "pool set-default primary", repo, homeDir, func() error {
			resetConfigFamiliesPoolFlags()
			return poolSetDefaultCmd.RunE(poolSetDefaultCmd, []string{"primary"})
		})

		recordConfigFamiliesStep(t, &report, "pool ls", repo, homeDir, func() error {
			resetConfigFamiliesPoolFlags()
			return poolListCmd.RunE(poolListCmd, nil)
		})

		recordConfigFamiliesStep(t, &report, "prompt add oracle-prompt", repo, homeDir, func() error {
			return promptAddCmd.RunE(promptAddCmd, []string{"oracle-prompt", filepath.Join(repo, "prompt-source.md")})
		})

		recordConfigFamiliesStep(t, &report, "prompt ls", repo, homeDir, func() error {
			return promptListCmd.RunE(promptListCmd, nil)
		})

		recordConfigFamiliesStep(t, &report, "prompt set-default oracle-prompt", repo, homeDir, func() error {
			return promptSetDefaultCmd.RunE(promptSetDefaultCmd, []string{"oracle-prompt"})
		})

		recordConfigFamiliesStep(t, &report, "prompt edit oracle-prompt", repo, homeDir, func() error {
			return promptEditCmd.RunE(promptEditCmd, []string{"oracle-prompt"})
		})

		recordConfigFamiliesStep(t, &report, "template add oracle-template", repo, homeDir, func() error {
			resetConfigFamiliesTemplateFlags()
			return templateAddCmd.RunE(templateAddCmd, []string{"oracle-template"})
		})

		recordConfigFamiliesStep(t, &report, "template ls", repo, homeDir, func() error {
			resetConfigFamiliesTemplateFlags()
			return templateListCmd.RunE(templateListCmd, nil)
		})

		recordConfigFamiliesStep(t, &report, "template show oracle-template", repo, homeDir, func() error {
			resetConfigFamiliesTemplateFlags()
			return templateShowCmd.RunE(templateShowCmd, []string{"oracle-template"})
		})

		recordConfigFamiliesStep(t, &report, "template edit oracle-template", repo, homeDir, func() error {
			resetConfigFamiliesTemplateFlags()
			return templateEditCmd.RunE(templateEditCmd, []string{"oracle-template"})
		})

		recordConfigFamiliesStep(t, &report, "template run oracle-template", repo, homeDir, func() error {
			resetConfigFamiliesTemplateFlags()
			templateAgent = "config-oracle-agent"
			return templateRunCmd.RunE(templateRunCmd, []string{"oracle-template"})
		})

		recordConfigFamiliesStep(t, &report, "template delete oracle-template", repo, homeDir, func() error {
			resetConfigFamiliesTemplateFlags()
			return templateDeleteCmd.RunE(templateDeleteCmd, []string{"oracle-template"})
		})

		recordConfigFamiliesStep(t, &report, "seq add oracle-seq", repo, homeDir, func() error {
			resetConfigFamiliesSequenceFlags()
			return sequenceAddCmd.RunE(sequenceAddCmd, []string{"oracle-seq"})
		})

		recordConfigFamiliesStep(t, &report, "seq ls", repo, homeDir, func() error {
			resetConfigFamiliesSequenceFlags()
			return sequenceListCmd.RunE(sequenceListCmd, nil)
		})

		recordConfigFamiliesStep(t, &report, "seq show oracle-seq", repo, homeDir, func() error {
			resetConfigFamiliesSequenceFlags()
			return sequenceShowCmd.RunE(sequenceShowCmd, []string{"oracle-seq"})
		})

		recordConfigFamiliesStep(t, &report, "seq edit oracle-seq", repo, homeDir, func() error {
			resetConfigFamiliesSequenceFlags()
			return sequenceEditCmd.RunE(sequenceEditCmd, []string{"oracle-seq"})
		})

		recordConfigFamiliesStep(t, &report, "seq run oracle-seq", repo, homeDir, func() error {
			resetConfigFamiliesSequenceFlags()
			sequenceAgent = "config-oracle-agent"
			return sequenceRunCmd.RunE(sequenceRunCmd, []string{"oracle-seq"})
		})

		recordConfigFamiliesStep(t, &report, "seq delete oracle-seq", repo, homeDir, func() error {
			resetConfigFamiliesSequenceFlags()
			return sequenceDeleteCmd.RunE(sequenceDeleteCmd, []string{"oracle-seq"})
		})

		got := mustMarshalJSON(t, report)
		goldenPath := configFamiliesGoldenPath(t)

		if os.Getenv("FORGE_UPDATE_GOLDENS") == "1" {
			if err := os.MkdirAll(filepath.Dir(goldenPath), 0o755); err != nil {
				t.Fatalf("mkdir golden dir: %v", err)
			}
			if err := os.WriteFile(goldenPath, []byte(got), 0o644); err != nil {
				t.Fatalf("write golden: %v", err)
			}
			return
		}

		wantBytes, err := os.ReadFile(goldenPath)
		if err != nil {
			t.Fatalf("read golden: %v (set FORGE_UPDATE_GOLDENS=1 to generate)", err)
		}
		want := string(wantBytes)
		if normalizeGolden(want) != normalizeGolden(got) {
			t.Fatalf("oracle fixture drift: %s (set FORGE_UPDATE_GOLDENS=1 to regenerate)\n--- want\n%s\n--- got\n%s", goldenPath, want, got)
		}
	})
}

func recordConfigFamiliesStep(t *testing.T, report *configFamiliesOracleReport, name, repo, home string, run func() error) {
	t.Helper()

	stdout, stderr, runErr := captureStdoutStderr(run)
	if runErr != nil {
		t.Fatalf("%s: %v\nstderr:\n%s\nstdout:\n%s", name, runErr, stderr, stdout)
	}

	step := configFamiliesOracleStep{
		Name:  name,
		State: snapshotConfigFamiliesState(t, repo, "config-oracle-agent"),
	}
	if strings.TrimSpace(stdout) != "" {
		step.Stdout = normalizeConfigFamiliesOutput(t, stdout, repo, home)
	}
	if strings.TrimSpace(stderr) != "" {
		step.Stderr = normalizeConfigFamiliesText(stderr, repo, home)
	}
	report.Steps = append(report.Steps, step)
}

func seedConfigFamiliesOracleDB(t *testing.T, repo string) {
	t.Helper()

	withDB(t, func(database *db.DB) {
		ctx := context.Background()
		nodeRepo := db.NewNodeRepository(database)
		workspaceRepo := db.NewWorkspaceRepository(database)
		agentRepo := db.NewAgentRepository(database)

		node := &models.Node{
			Name:       "config-oracle-node",
			SSHBackend: models.SSHBackendAuto,
			Status:     models.NodeStatusUnknown,
			IsLocal:    true,
		}
		if err := nodeRepo.Create(ctx, node); err != nil {
			t.Fatalf("create node: %v", err)
		}

		workspace := &models.Workspace{
			ID:          "config-oracle-workspace",
			NodeID:      node.ID,
			Name:        "config-oracle-workspace",
			RepoPath:    repo,
			TmuxSession: "config-oracle",
		}
		if err := workspaceRepo.Create(ctx, workspace); err != nil {
			t.Fatalf("create workspace: %v", err)
		}

		agent := &models.Agent{
			ID:          "config-oracle-agent",
			WorkspaceID: workspace.ID,
			Type:        models.AgentTypeOpenCode,
			TmuxPane:    "config-oracle:0.0",
			State:       models.AgentStateIdle,
		}
		if err := agentRepo.Create(ctx, agent); err != nil {
			t.Fatalf("create agent: %v", err)
		}
	})
}

func writeConfigFamiliesPromptSource(t *testing.T, repo string) {
	t.Helper()
	body := "# Oracle Prompt\n\nFollow the fixture flow.\n"
	path := filepath.Join(repo, "prompt-source.md")
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatalf("write prompt source: %v", err)
	}
}

func snapshotConfigFamiliesState(t *testing.T, repo, agentID string) configFamiliesOracleState {
	t.Helper()

	state := configFamiliesOracleState{
		Prompts:   listConfigFamiliesNames(filepath.Join(repo, ".forge", "prompts"), ".md"),
		Templates: listConfigFamiliesNames(filepath.Join(getConfigDir(), "templates"), ".yaml"),
		Sequences: listConfigFamiliesNames(filepath.Join(getConfigDir(), "sequences"), ".yaml"),
	}

	withDB(t, func(database *db.DB) {
		ctx := context.Background()
		profileRepo := db.NewProfileRepository(database)
		poolRepo := db.NewPoolRepository(database)
		queueRepo := db.NewQueueRepository(database)

		profiles, err := profileRepo.List(ctx)
		if err != nil {
			t.Fatalf("list profiles: %v", err)
		}
		sort.Slice(profiles, func(i, j int) bool { return profiles[i].Name < profiles[j].Name })
		for _, profile := range profiles {
			state.Profiles = append(state.Profiles, configFamiliesProfileState{
				Name:           profile.Name,
				Harness:        profile.Harness,
				AuthKind:       profile.AuthKind,
				AuthHome:       profile.AuthHome,
				PromptMode:     profile.PromptMode,
				Command:        profile.CommandTemplate,
				Model:          profile.Model,
				ExtraArgs:      append([]string(nil), profile.ExtraArgs...),
				Env:            cloneStringMap(profile.Env),
				MaxConcurrency: profile.MaxConcurrency,
				Cooldown:       profile.CooldownUntil != nil,
			})
		}

		pools, err := poolRepo.List(ctx)
		if err != nil {
			t.Fatalf("list pools: %v", err)
		}
		sort.Slice(pools, func(i, j int) bool { return pools[i].Name < pools[j].Name })
		for _, pool := range pools {
			row := configFamiliesPoolState{
				Name:     pool.Name,
				Strategy: pool.Strategy,
				Default:  pool.IsDefault,
			}
			members, err := poolRepo.ListMembers(ctx, pool.ID)
			if err != nil {
				t.Fatalf("list pool members: %v", err)
			}
			sort.Slice(members, func(i, j int) bool { return members[i].Position < members[j].Position })
			for _, member := range members {
				profile, err := profileRepo.Get(ctx, member.ProfileID)
				if err != nil {
					continue
				}
				row.Members = append(row.Members, profile.Name)
			}
			state.Pools = append(state.Pools, row)
		}

		items, err := queueRepo.List(ctx, agentID)
		if err != nil {
			t.Fatalf("list queue: %v", err)
		}
		for _, item := range items {
			queueRow := configFamiliesQueueState{
				Type:     item.Type,
				Status:   item.Status,
				Position: item.Position,
			}
			var payload models.MessagePayload
			if err := json.Unmarshal(item.Payload, &payload); err == nil {
				queueRow.Message = payload.Text
			}
			state.Queue = append(state.Queue, queueRow)
		}
	})

	if len(state.Profiles) == 0 {
		state.Profiles = nil
	}
	if len(state.Pools) == 0 {
		state.Pools = nil
	}
	if len(state.Prompts) == 0 {
		state.Prompts = nil
	}
	if len(state.Templates) == 0 {
		state.Templates = nil
	}
	if len(state.Sequences) == 0 {
		state.Sequences = nil
	}
	if len(state.Queue) == 0 {
		state.Queue = nil
	}

	return state
}

func listConfigFamiliesNames(dir, ext string) []string {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil
	}
	out := make([]string, 0, len(entries))
	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}
		if strings.ToLower(filepath.Ext(entry.Name())) != strings.ToLower(ext) {
			continue
		}
		out = append(out, strings.TrimSuffix(entry.Name(), filepath.Ext(entry.Name())))
	}
	sort.Strings(out)
	return out
}

func normalizeConfigFamiliesOutput(t *testing.T, raw, repo, home string) string {
	t.Helper()
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return ""
	}

	var value any
	if err := json.Unmarshal([]byte(raw), &value); err != nil {
		return normalizeConfigFamiliesText(raw, repo, home) + "\n"
	}
	value = normalizeConfigFamiliesJSONValue(value)
	data, err := json.MarshalIndent(value, "", "  ")
	if err != nil {
		t.Fatalf("marshal normalized json: %v", err)
	}
	return normalizeConfigFamiliesText(string(data), repo, home) + "\n"
}

func normalizeConfigFamiliesJSONValue(v any) any {
	switch vv := v.(type) {
	case map[string]any:
		out := make(map[string]any, len(vv))
		for key, value := range vv {
			switch key {
			case "id":
				out[key] = "<ID>"
			case "node_id":
				out[key] = "<NODE_ID>"
			case "workspace_id":
				out[key] = "<WORKSPACE_ID>"
			case "profile_id":
				out[key] = "<PROFILE_ID>"
			case "pool_id":
				out[key] = "<POOL_ID>"
			case "item_id":
				out[key] = "<ITEM_ID>"
			case "item_ids":
				if arr, ok := value.([]any); ok {
					ids := make([]any, 0, len(arr))
					for range arr {
						ids = append(ids, "<ITEM_ID>")
					}
					out[key] = ids
				} else {
					out[key] = normalizeConfigFamiliesJSONValue(value)
				}
			case "created_at", "updated_at", "cooldown_until", "dispatched_at", "completed_at":
				out[key] = "<TIME>"
			default:
				out[key] = normalizeConfigFamiliesJSONValue(value)
			}
		}
		return out
	case []any:
		out := make([]any, 0, len(vv))
		for _, item := range vv {
			out = append(out, normalizeConfigFamiliesJSONValue(item))
		}
		return out
	default:
		return v
	}
}

func normalizeConfigFamiliesText(text, repo, home string) string {
	replacer := strings.NewReplacer(
		repo, "<REPO>",
		home, "<HOME>",
		"\r\n", "\n",
	)
	return strings.TrimSpace(replacer.Replace(text))
}

func cloneStringMap(input map[string]string) map[string]string {
	if len(input) == 0 {
		return nil
	}
	out := make(map[string]string, len(input))
	for key, value := range input {
		out[key] = value
	}
	return out
}

func configFamiliesGoldenPath(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatalf("resolve test file path")
	}
	base := filepath.Dir(file)
	return filepath.Join(base, "testdata", "oracle", "config_families.json")
}

func resetConfigFamiliesCommandState() {
	resetConfigFamiliesProfileFlags()
	clearConfigFamiliesProfileEditChanged()
	resetConfigFamiliesPoolFlags()
	resetConfigFamiliesTemplateFlags()
	resetConfigFamiliesSequenceFlags()
}

func resetConfigFamiliesProfileFlags() {
	profileAddName = ""
	profileAddAuthKind = ""
	profileAddAuthHome = ""
	profileAddPromptMode = ""
	profileAddCommand = ""
	profileAddModel = ""
	profileAddExtraArgs = nil
	profileAddEnv = nil
	profileAddMaxConcurrency = 0

	profileEditName = ""
	profileEditAuthKind = ""
	profileEditAuthHome = ""
	profileEditPromptMode = ""
	profileEditCommand = ""
	profileEditModel = ""
	profileEditExtraArgs = nil
	profileEditEnv = nil
	profileEditMaxConcurrency = 0

	profileCooldownUntil = ""
}

func clearConfigFamiliesProfileEditChanged() {
	for _, flagName := range []string{
		"name",
		"auth-kind",
		"home",
		"prompt-mode",
		"command",
		"model",
		"extra-arg",
		"env",
		"max-concurrency",
	} {
		if flag := profileEditCmd.Flags().Lookup(flagName); flag != nil {
			flag.Changed = false
		}
	}
}

func resetConfigFamiliesPoolFlags() {
	poolCreateStrategy = string(models.PoolStrategyRoundRobin)
}

func resetConfigFamiliesTemplateFlags() {
	templateTags = nil
	templateAgent = ""
	templateVars = nil
}

func resetConfigFamiliesSequenceFlags() {
	sequenceTags = nil
	sequenceAgent = ""
	sequenceVars = nil
}
