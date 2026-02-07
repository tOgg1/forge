package cli

import (
	"errors"
	"fmt"
	"io"
)

var errRobotHelpShown = errors.New("robot help shown")

func printRobotHelp(w io.Writer) {
	if w == nil {
		return
	}

	// keep: concise; copy-pasteable commands; stable section names
	fmt.Fprint(w, `Forge Robot Help

Purpose
- control plane + loop runner for coding agents
- loop-safe persistence: work context + generic memory, auto-injected into next prompt

Quick Start (Loops)
1) forge init
2) forge up --name <loop-name> --prompt <prompt-name|path>
3) forge ps
4) forge logs <loop>
5) forge msg <loop> "instruction"

Persistent Loop State (auto prompt injection)
- forge work ...  : task pointer + status (task-tech-agnostic; use sv-..., jira-..., file.md)
- forge mem ...   : per-loop key/value memory
- defaults to current loop via $FORGE_LOOP_ID (injected into loop runs)

Work
- forge work set sv-1v3 --status blocked --detail "waiting for agent-b"
- forge work current
- forge work clear
- forge work ls

sv hook idea (sv may call forge)
- on: sv task start <id>
- run: forge work set <id> --status in_progress --loop <forge-loop> --agent $SV_ACTOR

Memory
- forge mem set blocked_on "agent-b reply"
- forge mem get blocked_on
- forge mem ls
- forge mem rm blocked_on

Env (in loop runs)
- FORGE_LOOP_ID, FORGE_LOOP_NAME
- FMAIL_AGENT defaults to loop name (if unset)

Automation / scripting
- add --json / --jsonl for machine output on most commands
`)
}
