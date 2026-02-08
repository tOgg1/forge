package cli

import (
	"context"
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/spf13/cobra"
	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/loop"
	"github.com/tOgg1/forge/internal/models"
)

var (
	loopUpCount         int
	loopUpName          string
	loopUpNamePrefix    string
	loopUpPool          string
	loopUpProfile       string
	loopUpPrompt        string
	loopUpPromptMsg     string
	loopUpInterval      string
	loopUpMaxRuntime    string
	loopUpMaxIterations int
	loopUpTags          string
	loopUpSpawnOwner    string

	loopUpQuantStopCmd        string
	loopUpQuantStopEvery      int
	loopUpQuantStopWhen       string
	loopUpQuantStopDecision   string
	loopUpQuantStopExitCodes  string
	loopUpQuantStopExitInvert bool
	loopUpQuantStopStdoutMode string
	loopUpQuantStopStderrMode string
	loopUpQuantStopStdoutRe   string
	loopUpQuantStopStderrRe   string
	loopUpQuantStopTimeout    string

	loopUpQualStopEvery     int
	loopUpQualStopPrompt    string
	loopUpQualStopPromptMsg string
	loopUpQualStopOnInvalid string
)

func init() {
	rootCmd.AddCommand(loopUpCmd)

	loopUpCmd.Flags().IntVarP(&loopUpCount, "count", "n", 1, "number of loops to start")
	loopUpCmd.Flags().StringVar(&loopUpName, "name", "", "loop name (single loop)")
	loopUpCmd.Flags().StringVar(&loopUpNamePrefix, "name-prefix", "", "loop name prefix")
	loopUpCmd.Flags().StringVar(&loopUpPool, "pool", "", "pool name or ID")
	loopUpCmd.Flags().StringVar(&loopUpProfile, "profile", "", "profile name or ID")
	loopUpCmd.Flags().StringVar(&loopUpPrompt, "prompt", "", "base prompt path or prompt name")
	loopUpCmd.Flags().StringVar(&loopUpPromptMsg, "prompt-msg", "", "base prompt content for each iteration")
	loopUpCmd.Flags().StringVar(&loopUpInterval, "interval", "", "sleep interval (e.g., 30s, 2m)")
	loopUpCmd.Flags().StringVarP(&loopUpMaxRuntime, "max-runtime", "r", "", "max runtime before stopping (e.g., 30m, 2h; 0s/empty = no limit)")
	loopUpCmd.Flags().IntVarP(&loopUpMaxIterations, "max-iterations", "i", 0, "max iterations before stopping (0 = no limit)")
	loopUpCmd.Flags().StringVar(&loopUpTags, "tags", "", "comma-separated tags")
	loopUpCmd.Flags().StringVar(&loopUpSpawnOwner, "spawn-owner", string(loopSpawnOwnerAuto), "loop runner owner (local|daemon|auto)")

	loopUpCmd.Flags().StringVar(&loopUpQuantStopCmd, "quantitative-stop-cmd", "", "quantitative stop: command to execute (bash -lc)")
	loopUpCmd.Flags().IntVar(&loopUpQuantStopEvery, "quantitative-stop-every", 1, "quantitative stop: evaluate every N iterations (> 0)")
	loopUpCmd.Flags().StringVar(&loopUpQuantStopWhen, "quantitative-stop-when", "before", "quantitative stop: when to evaluate (before|after|both)")
	loopUpCmd.Flags().StringVar(&loopUpQuantStopDecision, "quantitative-stop-decision", "stop", "quantitative stop: decision on match (stop|continue)")
	loopUpCmd.Flags().StringVar(&loopUpQuantStopExitCodes, "quantitative-stop-exit-codes", "", "quantitative stop: match exit codes (comma-separated ints)")
	loopUpCmd.Flags().BoolVar(&loopUpQuantStopExitInvert, "quantitative-stop-exit-invert", false, "quantitative stop: invert exit code match (match when exit not in codes)")
	loopUpCmd.Flags().StringVar(&loopUpQuantStopStdoutMode, "quantitative-stop-stdout", "any", "quantitative stop: stdout mode (any|empty|nonempty)")
	loopUpCmd.Flags().StringVar(&loopUpQuantStopStderrMode, "quantitative-stop-stderr", "any", "quantitative stop: stderr mode (any|empty|nonempty)")
	loopUpCmd.Flags().StringVar(&loopUpQuantStopStdoutRe, "quantitative-stop-stdout-regex", "", "quantitative stop: stdout regex (RE2)")
	loopUpCmd.Flags().StringVar(&loopUpQuantStopStderrRe, "quantitative-stop-stderr-regex", "", "quantitative stop: stderr regex (RE2)")
	loopUpCmd.Flags().StringVar(&loopUpQuantStopTimeout, "quantitative-stop-timeout", "", "quantitative stop: command timeout (duration, e.g. 10s)")

	loopUpCmd.Flags().IntVar(&loopUpQualStopEvery, "qualitative-stop-every", 0, "qualitative stop: run every N main iterations (> 0)")
	loopUpCmd.Flags().StringVar(&loopUpQualStopPrompt, "qualitative-stop-prompt", "", "qualitative stop: prompt path or prompt name under .forge/prompts/")
	loopUpCmd.Flags().StringVar(&loopUpQualStopPromptMsg, "qualitative-stop-prompt-msg", "", "qualitative stop: inline prompt content")
	loopUpCmd.Flags().StringVar(&loopUpQualStopOnInvalid, "qualitative-stop-on-invalid", "continue", "qualitative stop: on invalid judge output (stop|continue)")
}

var loopUpCmd = &cobra.Command{
	Use:   "up",
	Short: "Start loop(s) for a repo",
	Long: `Start loop(s) for the current repo.

Smart stop (optional):
- quantitative: run a command and stop/continue on match (--quantitative-stop-*)
- qualitative: every N main iterations, run a judge iteration; agent prints 0(stop) or 1(continue) (--qualitative-stop-*)`,
	Args: cobra.NoArgs,
	RunE: func(cmd *cobra.Command, args []string) error {
		if loopUpCount < 1 {
			return fmt.Errorf("--count must be at least 1")
		}
		if loopUpName != "" && loopUpCount > 1 {
			return fmt.Errorf("--name requires --count=1")
		}
		if loopUpPool != "" && loopUpProfile != "" {
			return fmt.Errorf("use either --pool or --profile, not both")
		}

		repoPath, err := resolveRepoPath("")
		if err != nil {
			return err
		}

		cfg := GetConfig()
		interval, err := parseDuration(loopUpInterval, cfg.LoopDefaults.Interval)
		if err != nil {
			return err
		}
		if interval < 0 {
			return fmt.Errorf("interval must be >= 0")
		}
		if loopUpMaxIterations < 0 {
			return fmt.Errorf("max iterations must be >= 0")
		}
		maxRuntime, err := parseDuration(loopUpMaxRuntime, 0)
		if err != nil {
			return err
		}
		if maxRuntime < 0 {
			return fmt.Errorf("max runtime must be >= 0")
		}

		basePromptMsg := strings.TrimSpace(loopUpPromptMsg)
		if basePromptMsg == "" {
			basePromptMsg = strings.TrimSpace(cfg.LoopDefaults.PromptMsg)
		}

		basePromptPath := ""
		if loopUpPrompt != "" {
			resolved, _, err := resolvePromptPath(repoPath, loopUpPrompt)
			if err != nil {
				return err
			}
			basePromptPath = resolved
		} else if cfg.LoopDefaults.Prompt != "" {
			resolved, _, err := resolvePromptPath(repoPath, cfg.LoopDefaults.Prompt)
			if err != nil {
				return err
			}
			basePromptPath = resolved
		}

		tags := parseTags(loopUpTags)

		stopCfg := models.LoopStopConfig{}
		if strings.TrimSpace(loopUpQuantStopCmd) != "" {
			if loopUpQuantStopEvery <= 0 {
				return fmt.Errorf("quantitative stop every must be > 0")
			}
			exitCodes, err := parseCSVInts(loopUpQuantStopExitCodes)
			if err != nil {
				return fmt.Errorf("quantitative stop exit codes: %w", err)
			}
			timeout, err := parseDuration(loopUpQuantStopTimeout, 0)
			if err != nil {
				return err
			}
			if timeout < 0 {
				return fmt.Errorf("quantitative stop timeout must be >= 0")
			}
			stdoutMode := strings.ToLower(strings.TrimSpace(loopUpQuantStopStdoutMode))
			stderrMode := strings.ToLower(strings.TrimSpace(loopUpQuantStopStderrMode))
			if stdoutMode == "" {
				stdoutMode = "any"
			}
			if stderrMode == "" {
				stderrMode = "any"
			}
			noCriteria := len(exitCodes) == 0 &&
				stdoutMode == "any" &&
				stderrMode == "any" &&
				strings.TrimSpace(loopUpQuantStopStdoutRe) == "" &&
				strings.TrimSpace(loopUpQuantStopStderrRe) == ""
			if noCriteria {
				// Default: match success.
				exitCodes = []int{0}
			}

			stopCfg.Quant = &models.LoopQuantStopConfig{
				Cmd:            loopUpQuantStopCmd,
				EveryN:         loopUpQuantStopEvery,
				When:           loopUpQuantStopWhen,
				Decision:       loopUpQuantStopDecision,
				ExitCodes:      exitCodes,
				ExitInvert:     loopUpQuantStopExitInvert,
				StdoutMode:     stdoutMode,
				StderrMode:     stderrMode,
				StdoutRegex:    loopUpQuantStopStdoutRe,
				StderrRegex:    loopUpQuantStopStderrRe,
				TimeoutSeconds: int(timeout.Round(time.Second).Seconds()),
			}
		}

		if loopUpQualStopEvery > 0 ||
			strings.TrimSpace(loopUpQualStopPrompt) != "" ||
			strings.TrimSpace(loopUpQualStopPromptMsg) != "" {
			if loopUpQualStopEvery <= 0 {
				return fmt.Errorf("qualitative stop every must be > 0")
			}
			if strings.TrimSpace(loopUpQualStopPrompt) != "" && strings.TrimSpace(loopUpQualStopPromptMsg) != "" {
				return fmt.Errorf("use either --qualitative-stop-prompt or --qualitative-stop-prompt-msg, not both")
			}

			payload := models.NextPromptOverridePayload{}
			if strings.TrimSpace(loopUpQualStopPromptMsg) != "" {
				payload.Prompt = strings.TrimSpace(loopUpQualStopPromptMsg)
				payload.IsPath = false
			} else {
				if strings.TrimSpace(loopUpQualStopPrompt) == "" {
					return fmt.Errorf("qualitative stop requires --qualitative-stop-prompt or --qualitative-stop-prompt-msg")
				}
				resolved, _, err := resolvePromptPath(repoPath, loopUpQualStopPrompt)
				if err != nil {
					return err
				}
				payload.Prompt = resolved
				payload.IsPath = true
			}

			stopCfg.Qual = &models.LoopQualStopConfig{
				EveryN:    loopUpQualStopEvery,
				Prompt:    payload,
				OnInvalid: loopUpQualStopOnInvalid,
			}
		}

		database, err := openDatabase()
		if err != nil {
			return err
		}
		defer database.Close()

		loopRepo := db.NewLoopRepository(database)
		poolRepo := db.NewPoolRepository(database)
		profileRepo := db.NewProfileRepository(database)

		var poolID string
		if loopUpPool != "" {
			pool, err := resolvePoolByRef(context.Background(), poolRepo, loopUpPool)
			if err != nil {
				return err
			}
			poolID = pool.ID
		}

		var profileID string
		if loopUpProfile != "" {
			profile, err := resolveProfileByRef(context.Background(), profileRepo, loopUpProfile)
			if err != nil {
				return err
			}
			profileID = profile.ID
		}

		existing, err := loopRepo.List(context.Background())
		if err != nil {
			return err
		}
		existingNames := make(map[string]struct{}, len(existing))
		for _, item := range existing {
			existingNames[item.Name] = struct{}{}
		}

		created := make([]*models.Loop, 0, loopUpCount)
		spawnOwner, err := resolveSpawnOwner(cmd, loopUpSpawnOwner)
		if err != nil {
			return err
		}
		for i := 0; i < loopUpCount; i++ {
			name := loopUpName
			if name == "" {
				if loopUpNamePrefix != "" {
					name = fmt.Sprintf("%s-%d", loopUpNamePrefix, i+1)
				} else {
					name = generateLoopName(existingNames)
				}
			}
			if _, exists := existingNames[name]; exists {
				return fmt.Errorf("loop name %q already exists", name)
			}
			existingNames[name] = struct{}{}

			loopEntry := &models.Loop{
				Name:              name,
				RepoPath:          repoPath,
				BasePromptPath:    basePromptPath,
				BasePromptMsg:     basePromptMsg,
				IntervalSeconds:   int(interval.Round(time.Second).Seconds()),
				MaxIterations:     loopUpMaxIterations,
				MaxRuntimeSeconds: int(maxRuntime.Round(time.Second).Seconds()),
				PoolID:            poolID,
				ProfileID:         profileID,
				Tags:              tags,
				State:             models.LoopStateStopped,
			}
			if stopCfg.Quant != nil || stopCfg.Qual != nil {
				loopEntry.Metadata = map[string]any{"stop_config": stopCfg}
			}
			if err := loopRepo.Create(context.Background(), loopEntry); err != nil {
				return err
			}

			loopEntry.LogPath = loop.LogPath(cfg.Global.DataDir, loopEntry.Name, loopEntry.ID)
			loopEntry.LedgerPath = loop.LedgerPath(repoPath, loopEntry.Name, loopEntry.ID)
			if err := loopRepo.Update(context.Background(), loopEntry); err != nil {
				return err
			}

			startResult, err := startLoopRunnerFunc(loopEntry.ID, cfgFile, spawnOwner)
			if err != nil {
				return err
			}
			if err := setLoopRunnerMetadata(context.Background(), loopRepo, loopEntry.ID, startResult.Owner, startResult.InstanceID); err != nil {
				return err
			}

			created = append(created, loopEntry)
		}

		if IsJSONOutput() || IsJSONLOutput() {
			return WriteOutput(os.Stdout, created)
		}

		if IsQuiet() {
			return nil
		}

		for _, loopEntry := range created {
			fmt.Fprintf(os.Stdout, "Loop %q started (%s)\n", loopEntry.Name, loopShortID(loopEntry))
		}

		return nil
	},
}
