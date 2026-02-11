// Package version provides version information for Forge.
package version

// These variables are set at build time using ldflags.
var (
	// Version is the semantic version.
	Version = "dev"

	// Commit is the git commit hash.
	Commit = "none"

	// Date is the build date.
	Date = "unknown"
)

// Info returns formatted version information.
func Info() string {
	return Version + " (commit: " + Commit + ", built: " + Date + ")"
}

// Short returns just the version string.
func Short() string {
	return Version
}
