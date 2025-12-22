# Mock Helpers

This package provides shared mock implementations for testing.

## SSHExecutor

`SSHExecutor` is a mock implementation of `ssh.Executor` for testing SSH operations without
actually connecting to remote hosts.

### Basic Usage

```go
import "github.com/opencode-ai/swarm/internal/testutil/mocks"

func TestMyFunction(t *testing.T) {
    exec := mocks.NewSSHExecutor()
    
    // Set a canned response for commands starting with "echo"
    exec.SetResponse("echo", []byte("hello\n"), nil, nil)
    
    // Your code under test
    stdout, _, err := exec.Exec(ctx, "echo hello")
    
    // Verify the command was called
    if exec.CallCount() != 1 {
        t.Error("expected 1 call")
    }
}
```

### Response Queue

For testing sequences of commands:

```go
exec := mocks.NewSSHExecutor()
exec.QueueResponse([]byte("first"), nil, nil)
exec.QueueResponse([]byte("second"), nil, nil)

// First call returns "first", second returns "second"
```

### Error Simulation

```go
exec := mocks.NewSSHExecutor()
exec.SetResponse("fail", nil, []byte("error output"), errors.New("command failed"))

// Commands starting with "fail" will return an error
```

### Session Support

```go
exec := mocks.NewSSHExecutor()
session, _ := exec.StartSession()

// Session delegates to the parent executor
stdout, _, _ := session.Exec(ctx, "some command")
session.Close()
```
