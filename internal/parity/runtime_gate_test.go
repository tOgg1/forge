package parity

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestRuntimeGateLoopQueueSmartStopLedger(t *testing.T) {
	t.Parallel()

	root := repoRoot(t)

	checks := []struct {
		path   string
		tokens []string
	}{
		{
			path: "internal/loop/queue.go",
			tokens: []string{
				"buildQueuePlan",
				"LoopQueueItemStopGraceful",
				"LoopQueueItemKillNow",
				"markQueueCompleted",
			},
		},
		{
			path: "internal/loop/stop_rules.go",
			tokens: []string{
				"quantRuleMatches",
				"normalizeDecision",
				"stopDecisionStop",
				"stopDecisionContinue",
			},
		},
		{
			path: "internal/loop/ledger.go",
			tokens: []string{
				"appendLedgerEntry",
				"limitOutputLines",
				"buildGitSummary",
			},
		},
		{
			path: "internal/loop/runner_test.go",
			tokens: []string{
				"TestRunnerRunOnceConsumesQueue",
				"LoopQueueItemMessageAppend",
				"LoopQueueItemNextPromptOverride",
			},
		},
		{
			path: "internal/scheduler/tick_test.go",
			tokens: []string{
				"TestTick_DispatchOnlyWhenIdle",
				"ConditionTypeAfterCooldown",
				"ActionTypeRequeue",
			},
		},
	}

	for _, check := range checks {
		body := mustReadFile(t, filepath.Join(root, check.path))
		for _, token := range check.tokens {
			if !strings.Contains(body, token) {
				t.Fatalf("runtime gate drift: %s missing token %q", check.path, token)
			}
		}
	}
}
