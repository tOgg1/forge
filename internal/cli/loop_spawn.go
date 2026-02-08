package cli

import (
	"context"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strings"
	"time"

	forgedv1 "github.com/tOgg1/forge/gen/forged/v1"
	"github.com/tOgg1/forge/internal/forged"
	"github.com/tOgg1/forge/internal/procutil"
)

type loopSpawnOwner string

const (
	loopSpawnOwnerLocal  loopSpawnOwner = "local"
	loopSpawnOwnerDaemon loopSpawnOwner = "daemon"
	loopSpawnOwnerAuto   loopSpawnOwner = "auto"
)

type loopRunnerStartResult struct {
	Owner      loopSpawnOwner
	InstanceID string
}

var (
	startLoopRunnerFunc                  = startLoopRunner
	startLoopProcessLocalFunc            = startLoopProcessLocal
	startLoopProcessDaemonFunc           = startLoopProcessDaemon
	dialForgedClientFunc                 = dialForgedClient
	spawnWarningWriter         io.Writer = os.Stderr
)

func parseLoopSpawnOwner(raw string) (loopSpawnOwner, error) {
	normalized := strings.ToLower(strings.TrimSpace(raw))
	if normalized == "" {
		return loopSpawnOwnerAuto, nil
	}
	switch loopSpawnOwner(normalized) {
	case loopSpawnOwnerLocal, loopSpawnOwnerDaemon, loopSpawnOwnerAuto:
		return loopSpawnOwner(normalized), nil
	default:
		return "", fmt.Errorf("invalid --spawn-owner %q (valid: local|daemon|auto)", raw)
	}
}

func startLoopRunner(loopID, configFile string, owner loopSpawnOwner) (loopRunnerStartResult, error) {
	switch owner {
	case loopSpawnOwnerLocal:
		if err := startLoopProcessLocalFunc(loopID, configFile); err != nil {
			return loopRunnerStartResult{}, err
		}
		return loopRunnerStartResult{Owner: loopSpawnOwnerLocal}, nil
	case loopSpawnOwnerDaemon:
		instanceID, err := startLoopProcessDaemonFunc(loopID, configFile)
		if err != nil {
			return loopRunnerStartResult{}, err
		}
		return loopRunnerStartResult{Owner: loopSpawnOwnerDaemon, InstanceID: instanceID}, nil
	case loopSpawnOwnerAuto:
		instanceID, err := startLoopProcessDaemonFunc(loopID, configFile)
		if err == nil {
			return loopRunnerStartResult{Owner: loopSpawnOwnerDaemon, InstanceID: instanceID}, nil
		}
		emitSpawnOwnerWarning(err)
		if localErr := startLoopProcessLocalFunc(loopID, configFile); localErr != nil {
			return loopRunnerStartResult{}, fmt.Errorf("daemon start failed (%v), local fallback failed: %w", err, localErr)
		}
		return loopRunnerStartResult{Owner: loopSpawnOwnerLocal}, nil
	default:
		return loopRunnerStartResult{}, fmt.Errorf("unsupported spawn owner %q", owner)
	}
}

func startLoopProcessLocal(loopID, configFile string) error {
	args := []string{"loop", "run", loopID}
	if strings.TrimSpace(configFile) != "" {
		args = append([]string{"--config", configFile}, args...)
	}

	cmd := exec.Command(os.Args[0], args...)
	cmd.Stdout = nil
	cmd.Stderr = nil
	cmd.Stdin = nil
	procutil.ConfigureDetached(cmd)

	if err := cmd.Start(); err != nil {
		return fmt.Errorf("failed to start local loop process: %w", err)
	}
	if cmd.Process != nil {
		_ = cmd.Process.Release()
	}
	return nil
}

func startLoopProcessDaemon(loopID, configFile string) (string, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	client, err := dialForgedClientFunc(ctx)
	if err != nil {
		return "", fmt.Errorf("forged daemon unavailable: %w", err)
	}
	defer client.Close()

	resp, err := client.StartLoopRunner(ctx, &forgedv1.StartLoopRunnerRequest{
		LoopId:      loopID,
		ConfigPath:  strings.TrimSpace(configFile),
		CommandPath: os.Args[0],
	})
	if err != nil {
		return "", fmt.Errorf("failed to start loop via daemon: %w", err)
	}
	if resp == nil || resp.Runner == nil {
		return "", fmt.Errorf("daemon returned empty loop runner response")
	}
	return strings.TrimSpace(resp.Runner.InstanceId), nil
}

func dialForgedClient(ctx context.Context) (*forged.Client, error) {
	target := fmt.Sprintf("%s:%d", forged.DefaultHost, forged.DefaultPort)
	return forged.Dial(ctx, target, forged.WithLogger(logger))
}

func emitSpawnOwnerWarning(cause error) {
	if spawnWarningWriter == nil || IsQuiet() || IsJSONOutput() || IsJSONLOutput() {
		return
	}
	_, _ = fmt.Fprintf(spawnWarningWriter, "warning: forged unavailable, falling back to local spawn (%v)\n", cause)
}
