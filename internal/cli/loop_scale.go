package cli

import (
	"context"
	"fmt"
	"os"
	"sort"
	"strings"
	"time"

	"github.com/spf13/cobra"
	"github.com/tOgg1/forge/internal/db"
	"github.com/tOgg1/forge/internal/loop"
	"github.com/tOgg1/forge/internal/models"
)

var (
	loopScaleCount         int
	loopScalePool          string
	loopScaleProfile       string
	loopScalePrompt        string
	loopScalePromptMsg     string
	loopScaleInterval      string
	loopScaleMaxRuntime    string
	loopScaleMaxIterations int
	loopScaleTags          string
	loopScaleNamePrefix    string
	loopScaleKill          bool

	loopScaleQuantStopCmd        string
	loopScaleQuantStopEvery      int
	loopScaleQuantStopWhen       string
	loopScaleQuantStopDecision   string
	loopScaleQuantStopExitCodes  string
	loopScaleQuantStopExitInvert bool
	loopScaleQuantStopStdoutMode string
	loopScaleQuantStopStderrMode string
	loopScaleQuantStopStdoutRe   string
	loopScaleQuantStopStderrRe   string
	loopScaleQuantStopTimeout    string

	loopScaleQualStopEvery     int
	loopScaleQualStopPrompt    string
	loopScaleQualStopPromptMsg string
	loopScaleQualStopOnInvalid string
)

func init() {
	rootCmd.AddCommand(loopScaleCmd)

	loopScaleCmd.Flags().IntVarP(&loopScaleCount, "count", "n", 1, "target loop count")
	loopScaleCmd.Flags().StringVar(&loopScalePool, "pool", "", "pool name or ID")
	loopScaleCmd.Flags().StringVar(&loopScaleProfile, "profile", "", "profile name or ID")
	loopScaleCmd.Flags().StringVar(&loopScalePrompt, "prompt", "", "base prompt path or name")
	loopScaleCmd.Flags().StringVar(&loopScalePromptMsg, "prompt-msg", "", "base prompt content for each iteration")
	loopScaleCmd.Flags().StringVar(&loopScaleInterval, "interval", "", "sleep interval")
	loopScaleCmd.Flags().StringVarP(&loopScaleMaxRuntime, "max-runtime", "r", "", "max runtime before stopping (e.g., 30m, 2h)")
	loopScaleCmd.Flags().IntVarP(&loopScaleMaxIterations, "max-iterations", "i", 0, "max iterations before stopping (> 0 required for new loops)")
	loopScaleCmd.Flags().StringVar(&loopScaleTags, "tags", "", "comma-separated tags")
	loopScaleCmd.Flags().StringVar(&loopScaleNamePrefix, "name-prefix", "", "name prefix for new loops")
	loopScaleCmd.Flags().BoolVar(&loopScaleKill, "kill", false, "kill extra loops instead of stopping")

	loopScaleCmd.Flags().StringVar(&loopScaleQuantStopCmd, "quantitative-stop-cmd", "", "quantitative stop: command to execute (bash -lc)")
	loopScaleCmd.Flags().IntVar(&loopScaleQuantStopEvery, "quantitative-stop-every", 1, "quantitative stop: evaluate every N iterations (> 0)")
	loopScaleCmd.Flags().StringVar(&loopScaleQuantStopWhen, "quantitative-stop-when", "before", "quantitative stop: when to evaluate (before|after|both)")
	loopScaleCmd.Flags().StringVar(&loopScaleQuantStopDecision, "quantitative-stop-decision", "stop", "quantitative stop: decision on match (stop|continue)")
	loopScaleCmd.Flags().StringVar(&loopScaleQuantStopExitCodes, "quantitative-stop-exit-codes", "", "quantitative stop: match exit codes (comma-separated ints)")
	loopScaleCmd.Flags().BoolVar(&loopScaleQuantStopExitInvert, "quantitative-stop-exit-invert", false, "quantitative stop: invert exit code match (match when exit not in codes)")
	loopScaleCmd.Flags().StringVar(&loopScaleQuantStopStdoutMode, "quantitative-stop-stdout", "any", "quantitative stop: stdout mode (any|empty|nonempty)")
	loopScaleCmd.Flags().StringVar(&loopScaleQuantStopStderrMode, "quantitative-stop-stderr", "any", "quantitative stop: stderr mode (any|empty|nonempty)")
	loopScaleCmd.Flags().StringVar(&loopScaleQuantStopStdoutRe, "quantitative-stop-stdout-regex", "", "quantitative stop: stdout regex (RE2)")
	loopScaleCmd.Flags().StringVar(&loopScaleQuantStopStderrRe, "quantitative-stop-stderr-regex", "", "quantitative stop: stderr regex (RE2)")
	loopScaleCmd.Flags().StringVar(&loopScaleQuantStopTimeout, "quantitative-stop-timeout", "", "quantitative stop: command timeout (duration, e.g. 10s)")

	loopScaleCmd.Flags().IntVar(&loopScaleQualStopEvery, "qualitative-stop-every", 0, "qualitative stop: run every N main iterations (> 0)")
	loopScaleCmd.Flags().StringVar(&loopScaleQualStopPrompt, "qualitative-stop-prompt", "", "qualitative stop: prompt path or prompt name under .forge/prompts/")
	loopScaleCmd.Flags().StringVar(&loopScaleQualStopPromptMsg, "qualitative-stop-prompt-msg", "", "qualitative stop: inline prompt content")
	loopScaleCmd.Flags().StringVar(&loopScaleQualStopOnInvalid, "qualitative-stop-on-invalid", "continue", "qualitative stop: on invalid judge output (stop|continue)")
}

var loopScaleCmd = &cobra.Command{
	Use:   "scale",
	Short: "Scale loops to a target count",
	Long: `Scale loops to a target count.

For new loops, you can configure smart stop:
- quantitative: run a command and stop/continue on match (--quantitative-stop-*)
- qualitative: every N main iterations, run a judge iteration; agent prints 0(stop) or 1(continue) (--qualitative-stop-*)`,
	Args: cobra.NoArgs,
	RunE: func(cmd *cobra.Command, args []string) error {
		if loopScaleCount < 0 {
			return fmt.Errorf("--count must be >= 0")
		}
		if loopScalePool != "" && loopScaleProfile != "" {
			return fmt.Errorf("use either --pool or --profile, not both")
		}

		repoPath, err := resolveRepoPath("")
		if err != nil {
			return err
		}

		cfg := GetConfig()
		interval, err := parseDuration(loopScaleInterval, cfg.LoopDefaults.Interval)
		if err != nil {
			return err
		}
		if loopScaleMaxIterations < 0 {
			return fmt.Errorf("max iterations must be >= 0")
		}
		maxRuntime, err := parseDuration(loopScaleMaxRuntime, 0)
		if err != nil {
			return err
		}
		if maxRuntime < 0 {
			return fmt.Errorf("max runtime must be >= 0")
		}

		basePromptMsg := loopScalePromptMsg
		if basePromptMsg == "" {
			basePromptMsg = cfg.LoopDefaults.PromptMsg
		}

		basePromptPath := ""
		if loopScalePrompt != "" {
			resolved, _, err := resolvePromptPath(repoPath, loopScalePrompt)
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

		tags := parseTags(loopScaleTags)

		stopCfg := models.LoopStopConfig{}
		if strings.TrimSpace(loopScaleQuantStopCmd) != "" {
			if loopScaleQuantStopEvery <= 0 {
				return fmt.Errorf("quantitative stop every must be > 0")
			}
			exitCodes, err := parseCSVInts(loopScaleQuantStopExitCodes)
			if err != nil {
				return fmt.Errorf("quantitative stop exit codes: %w", err)
			}
			timeout, err := parseDuration(loopScaleQuantStopTimeout, 0)
			if err != nil {
				return err
			}
			if timeout < 0 {
				return fmt.Errorf("quantitative stop timeout must be >= 0")
			}
			stdoutMode := strings.ToLower(strings.TrimSpace(loopScaleQuantStopStdoutMode))
			stderrMode := strings.ToLower(strings.TrimSpace(loopScaleQuantStopStderrMode))
			if stdoutMode == "" {
				stdoutMode = "any"
			}
			if stderrMode == "" {
				stderrMode = "any"
			}
			noCriteria := len(exitCodes) == 0 &&
				stdoutMode == "any" &&
				stderrMode == "any" &&
				strings.TrimSpace(loopScaleQuantStopStdoutRe) == "" &&
				strings.TrimSpace(loopScaleQuantStopStderrRe) == ""
			if noCriteria {
				exitCodes = []int{0}
			}

			stopCfg.Quant = &models.LoopQuantStopConfig{
				Cmd:            loopScaleQuantStopCmd,
				EveryN:         loopScaleQuantStopEvery,
				When:           loopScaleQuantStopWhen,
				Decision:       loopScaleQuantStopDecision,
				ExitCodes:      exitCodes,
				ExitInvert:     loopScaleQuantStopExitInvert,
				StdoutMode:     stdoutMode,
				StderrMode:     stderrMode,
				StdoutRegex:    loopScaleQuantStopStdoutRe,
				StderrRegex:    loopScaleQuantStopStderrRe,
				TimeoutSeconds: int(timeout.Round(time.Second).Seconds()),
			}
		}

		if loopScaleQualStopEvery > 0 ||
			strings.TrimSpace(loopScaleQualStopPrompt) != "" ||
			strings.TrimSpace(loopScaleQualStopPromptMsg) != "" {
			if loopScaleQualStopEvery <= 0 {
				return fmt.Errorf("qualitative stop every must be > 0")
			}
			if strings.TrimSpace(loopScaleQualStopPrompt) != "" && strings.TrimSpace(loopScaleQualStopPromptMsg) != "" {
				return fmt.Errorf("use either --qualitative-stop-prompt or --qualitative-stop-prompt-msg, not both")
			}

			payload := models.NextPromptOverridePayload{}
			if strings.TrimSpace(loopScaleQualStopPromptMsg) != "" {
				payload.Prompt = strings.TrimSpace(loopScaleQualStopPromptMsg)
				payload.IsPath = false
			} else {
				if strings.TrimSpace(loopScaleQualStopPrompt) == "" {
					return fmt.Errorf("qualitative stop requires --qualitative-stop-prompt or --qualitative-stop-prompt-msg")
				}
				resolved, _, err := resolvePromptPath(repoPath, loopScaleQualStopPrompt)
				if err != nil {
					return err
				}
				payload.Prompt = resolved
				payload.IsPath = true
			}

			stopCfg.Qual = &models.LoopQualStopConfig{
				EveryN:    loopScaleQualStopEvery,
				Prompt:    payload,
				OnInvalid: loopScaleQualStopOnInvalid,
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
		queueRepo := db.NewLoopQueueRepository(database)

		var poolID string
		if loopScalePool != "" {
			pool, err := resolvePoolByRef(context.Background(), poolRepo, loopScalePool)
			if err != nil {
				return err
			}
			poolID = pool.ID
		}

		var profileID string
		if loopScaleProfile != "" {
			profile, err := resolveProfileByRef(context.Background(), profileRepo, loopScaleProfile)
			if err != nil {
				return err
			}
			profileID = profile.ID
		}

		selector := loopSelector{Repo: repoPath, Pool: loopScalePool, Profile: loopScaleProfile}
		loops, err := selectLoops(context.Background(), loopRepo, poolRepo, profileRepo, selector)
		if err != nil {
			return err
		}

		sort.Slice(loops, func(i, j int) bool { return loops[i].CreatedAt.Before(loops[j].CreatedAt) })

		if len(loops) > loopScaleCount {
			extra := loops[loopScaleCount:]
			itemType := models.LoopQueueItemStopGraceful
			if loopScaleKill {
				itemType = models.LoopQueueItemKillNow
			}
			for _, loopEntry := range extra {
				payload, _ := controlPayload(itemType)
				item := &models.LoopQueueItem{Type: itemType, Payload: payload}
				if err := queueRepo.Enqueue(context.Background(), loopEntry.ID, item); err != nil {
					return err
				}
			}
		}

		if len(loops) < loopScaleCount {
			if loopScaleMaxIterations == 0 || maxRuntime == 0 {
				return fmt.Errorf("max iterations and max runtime must be > 0 to create loops")
			}
			toCreate := loopScaleCount - len(loops)
			existingNames := make(map[string]struct{}, len(loops))
			for _, entry := range loops {
				existingNames[entry.Name] = struct{}{}
			}
			for i := 0; i < toCreate; i++ {
				name := generateLoopName(existingNames)
				if loopScaleNamePrefix != "" {
					name = fmt.Sprintf("%s-%d", loopScaleNamePrefix, i+1)
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
					MaxIterations:     loopScaleMaxIterations,
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

				if err := startLoopProcess(loopEntry.ID); err != nil {
					return err
				}
			}
		}

		if IsJSONOutput() || IsJSONLOutput() {
			return WriteOutput(os.Stdout, map[string]any{"target": loopScaleCount, "current": len(loops)})
		}

		if IsQuiet() {
			return nil
		}

		fmt.Fprintf(os.Stdout, "Scaled loops to %d\n", loopScaleCount)
		return nil
	},
}
