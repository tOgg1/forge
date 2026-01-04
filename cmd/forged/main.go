// Package main is the entry point for the forged daemon.
// forged runs on each node to provide real-time agent orchestration,
// screen capture, and event streaming to the control plane.
package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"os/signal"
	"path/filepath"
	"syscall"

	"github.com/tOgg1/forge/internal/agent"
	"github.com/tOgg1/forge/internal/config"
	"github.com/tOgg1/forge/internal/forged"
	"github.com/tOgg1/forge/internal/logging"
	"github.com/tOgg1/forge/internal/node"
	"github.com/tOgg1/forge/internal/scheduler"
	"github.com/tOgg1/forge/internal/workspace"
)

// Version information (set by goreleaser)
var (
	version = "dev"
	commit  = "none"
	date    = "unknown"
)

func main() {
	hostname := flag.String("hostname", forged.DefaultHost, "hostname to listen on")
	port := flag.Int("port", forged.DefaultPort, "port to listen on")
	configFile := flag.String("config", "", "config file (default is $HOME/.config/forge/config.yaml)")
	logLevel := flag.String("log-level", "", "override logging level (debug, info, warn, error)")
	logFormat := flag.String("log-format", "", "override logging format (json, console)")
	defaultDisk := forged.DefaultDiskMonitorConfig()
	diskPath := flag.String("disk-path", "", "filesystem path to monitor for disk usage")
	diskWarn := flag.Float64("disk-warn", defaultDisk.WarnPercent, "disk usage percent to warn at")
	diskCritical := flag.Float64("disk-critical", defaultDisk.CriticalPercent, "disk usage percent to treat as critical")
	diskResume := flag.Float64("disk-resume", defaultDisk.ResumePercent, "disk usage percent to resume paused agents")
	diskPause := flag.Bool("disk-pause", defaultDisk.PauseAgents, "pause agent processes when disk is critically full")
	flag.Parse()

	cfg, loader, err := loadConfig(*configFile)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error loading config: %v\n", err)
		os.Exit(1)
	}

	if *logLevel != "" {
		cfg.Logging.Level = *logLevel
	}
	if *logFormat != "" {
		cfg.Logging.Format = *logFormat
	}

	logging.Init(logging.Config{
		Level:        cfg.Logging.Level,
		Format:       cfg.Logging.Format,
		EnableCaller: cfg.Logging.EnableCaller,
	})
	logger := logging.Component("forged")

	if err := cfg.EnsureDirectories(); err != nil {
		logger.Warn().Err(err).Msg("failed to create directories")
	}

	if cfgUsed := loader.ConfigFileUsed(); cfgUsed != "" {
		logger.Debug().Str("config_file", cfgUsed).Msg("loaded config file")
	}

	logger.Info().
		Str("version", version).
		Str("commit", commit).
		Str("built", date).
		Msg("forged starting")

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	diskConfig := forged.DefaultDiskMonitorConfig()
	if cfg.Global.DataDir != "" {
		diskConfig.Path = cfg.Global.DataDir
	}
	if *diskPath != "" {
		diskConfig.Path = *diskPath
	}
	diskConfig.WarnPercent = *diskWarn
	diskConfig.CriticalPercent = *diskCritical
	diskConfig.ResumePercent = *diskResume
	diskConfig.PauseAgents = *diskPause

	daemon, err := forged.New(cfg, logger, forged.Options{
		Hostname:          *hostname,
		Port:              *port,
		DiskMonitorConfig: &diskConfig,
	})
	if err != nil {
		logger.Error().Err(err).Msg("failed to initialize forged")
		os.Exit(1)
	}

	// Create and register scheduler if database is available
	if daemon.Database() != nil {
		// Create node service
		nodeService := node.NewService(daemon.NodeRepository())

		// Create workspace service
		wsService := workspace.NewService(
			daemon.WorkspaceRepository(),
			nodeService,
			daemon.AgentRepository(),
			workspace.WithEventRepository(daemon.EventRepository()),
		)

		// Create agent service
		agentServiceOpts := []agent.ServiceOption{
			agent.WithEventRepository(daemon.EventRepository()),
			agent.WithPortRepository(daemon.PortRepository()),
		}
		if cfg.Global.DataDir != "" {
			archiveDir := filepath.Join(cfg.Global.DataDir, "archives", "agents")
			agentServiceOpts = append(agentServiceOpts, agent.WithArchiveDir(archiveDir))
		}
		agentService := agent.NewService(
			daemon.AgentRepository(),
			daemon.QueueRepository(),
			wsService,
			nil, // accountService - can be nil for now
			daemon.TmuxClient(),
			agentServiceOpts...,
		)

		// Create scheduler with config
		schedConfig := scheduler.DefaultConfig()
		if cfg.Scheduler.DispatchInterval > 0 {
			schedConfig.TickInterval = cfg.Scheduler.DispatchInterval
		}
		if cfg.Scheduler.MaxRetries > 0 {
			schedConfig.MaxRetries = cfg.Scheduler.MaxRetries
		}
		if cfg.Scheduler.RetryBackoff > 0 {
			schedConfig.RetryBackoff = cfg.Scheduler.RetryBackoff
		}
		if cfg.Scheduler.DefaultCooldownDuration > 0 {
			schedConfig.DefaultCooldownDuration = cfg.Scheduler.DefaultCooldownDuration
		}

		sched := scheduler.New(
			schedConfig,
			agentService,
			daemon.QueueService(),
			daemon.StateEngine(),
			nil, // accountService
		)

		daemon.SetScheduler(sched)
		logger.Info().Msg("scheduler configured")
	}

	if err := daemon.Run(ctx); err != nil {
		logger.Error().Err(err).Msg("forged exited with error")
		os.Exit(1)
	}
}

func loadConfig(path string) (*config.Config, *config.Loader, error) {
	loader := config.NewLoader()
	if path != "" {
		loader.SetConfigFile(path)
	}
	cfg, err := loader.Load()
	if err != nil {
		return nil, nil, err
	}
	return cfg, loader, nil
}
