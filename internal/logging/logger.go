// Package logging provides structured logging for Forge using zerolog.
package logging

import (
	"context"
	"io"
	"os"
	"time"

	"github.com/rs/zerolog"
)

// Logger is the global logger instance.
var Logger zerolog.Logger

// ctxKey is the type for context keys.
type ctxKey string

const (
	// loggerKey is the context key for the logger.
	loggerKey ctxKey = "logger"
)

// Config holds logging configuration.
type Config struct {
	// Level is the minimum log level (debug, info, warn, error).
	Level string

	// Format is the output format (json, console).
	Format string

	// Output is where logs are written (defaults to stderr).
	Output io.Writer

	// EnableCaller adds caller information to logs.
	EnableCaller bool
}

// DefaultConfig returns the default logging configuration.
func DefaultConfig() Config {
	return Config{
		Level:        "info",
		Format:       "console",
		Output:       os.Stderr,
		EnableCaller: false,
	}
}

// Init initializes the global logger with the given configuration.
func Init(cfg Config) {
	level := parseLevel(cfg.Level)
	zerolog.SetGlobalLevel(level)

	// Configure time format
	zerolog.TimeFieldFormat = time.RFC3339

	output := cfg.Output
	if output == nil {
		output = os.Stderr
	}

	// Use console writer for human-readable output
	if cfg.Format == "console" {
		output = zerolog.ConsoleWriter{
			Out:        output,
			TimeFormat: "15:04:05",
			NoColor:    false,
		}
	}

	// Create logger
	ctx := zerolog.New(output).With().Timestamp()

	if cfg.EnableCaller {
		ctx = ctx.Caller()
	}

	Logger = ctx.Logger()
}

// parseLevel converts a string level to zerolog.Level.
func parseLevel(level string) zerolog.Level {
	switch level {
	case "debug":
		return zerolog.DebugLevel
	case "info":
		return zerolog.InfoLevel
	case "warn", "warning":
		return zerolog.WarnLevel
	case "error":
		return zerolog.ErrorLevel
	case "fatal":
		return zerolog.FatalLevel
	case "trace":
		return zerolog.TraceLevel
	default:
		return zerolog.InfoLevel
	}
}

// WithContext returns a new context with the logger attached.
func WithContext(ctx context.Context, logger zerolog.Logger) context.Context {
	return context.WithValue(ctx, loggerKey, logger)
}

// FromContext returns the logger from the context, or the global logger.
func FromContext(ctx context.Context) zerolog.Logger {
	if logger, ok := ctx.Value(loggerKey).(zerolog.Logger); ok {
		return logger
	}
	return Logger
}

// With creates a child logger with additional fields.
func With() zerolog.Context {
	return Logger.With()
}

// Debug logs a debug message.
func Debug() *zerolog.Event {
	return Logger.Debug()
}

// Info logs an info message.
func Info() *zerolog.Event {
	return Logger.Info()
}

// Warn logs a warning message.
func Warn() *zerolog.Event {
	return Logger.Warn()
}

// Error logs an error message.
func Error() *zerolog.Event {
	return Logger.Error()
}

// Fatal logs a fatal message and exits.
func Fatal() *zerolog.Event {
	return Logger.Fatal()
}

// Component creates a logger with a component field.
func Component(name string) zerolog.Logger {
	return Logger.With().Str("component", name).Logger()
}

// WithNode creates a logger with node context.
func WithNode(nodeID string) zerolog.Logger {
	return Logger.With().Str("node_id", nodeID).Logger()
}

// WithWorkspace creates a logger with workspace context.
func WithWorkspace(workspaceID string) zerolog.Logger {
	return Logger.With().Str("workspace_id", workspaceID).Logger()
}

// WithAgent creates a logger with agent context.
func WithAgent(agentID string) zerolog.Logger {
	return Logger.With().Str("agent_id", agentID).Logger()
}

func init() {
	// Initialize with default config
	Init(DefaultConfig())
}
