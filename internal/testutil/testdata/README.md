# Shared Test Fixtures

This directory contains shared fixtures for tests that import `internal/testutil`.

Package-specific fixtures should live in `testdata/` within that package.

## Directory Structure

```
testdata/
├── transcripts/           # Sample tmux pane captures for state detection tests
│   ├── claude_code_idle.txt
│   ├── opencode_working.txt
│   └── awaiting_approval.txt
└── README.md
```

## Usage

Use the `ReadFixture` function to load fixtures:

```go
import "github.com/opencode-ai/forge/internal/testutil"

func TestStateDetection(t *testing.T) {
    transcript := testutil.ReadFixture(t, "transcripts", "claude_code_idle.txt")
    
    // Use transcript data in your test
    state := detectState(string(transcript))
}
```

Use `FixturePath` to get the absolute path for fixtures:

```go
path := testutil.FixturePath(t, "transcripts", "claude_code_idle.txt")
```

## Adding New Fixtures

1. Create the fixture file in the appropriate subdirectory
2. Use descriptive names that indicate the test scenario
3. Update this README if adding a new category
