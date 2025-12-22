package ssh

import (
	"testing"
	"time"
)

func TestNewNativeExecutor(t *testing.T) {
	tests := []struct {
		name    string
		options ConnectionOptions
		wantErr bool
	}{
		{
			name:    "empty host",
			options: ConnectionOptions{},
			wantErr: true,
		},
		{
			name: "valid options",
			options: ConnectionOptions{
				Host: "example.com",
				User: "testuser",
			},
			wantErr: false, // Will still fail without actual auth, but config building should succeed
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := NewNativeExecutor(tt.options)
			if (err != nil) != tt.wantErr {
				// Note: without SSH agent or key, this will error on auth methods
				// So we skip the detailed check if it's a no-auth-methods error
				if tt.wantErr == false && err != nil && err.Error() != "failed to build SSH config: no authentication methods available" {
					t.Errorf("NewNativeExecutor() error = %v, wantErr %v", err, tt.wantErr)
				}
			}
		})
	}
}

func TestParseSSHTarget(t *testing.T) {
	tests := []struct {
		target   string
		wantUser string
		wantHost string
		wantPort string
	}{
		{
			target:   "example.com",
			wantUser: "",
			wantHost: "example.com",
			wantPort: "",
		},
		{
			target:   "user@example.com",
			wantUser: "user",
			wantHost: "example.com",
			wantPort: "",
		},
		{
			target:   "user@example.com:2222",
			wantUser: "user",
			wantHost: "example.com",
			wantPort: "2222",
		},
		{
			target:   "example.com:2222",
			wantUser: "",
			wantHost: "example.com",
			wantPort: "2222",
		},
	}

	for _, tt := range tests {
		t.Run(tt.target, func(t *testing.T) {
			user, host, port := parseSSHTarget(tt.target)
			if user != tt.wantUser {
				t.Errorf("parseSSHTarget() user = %v, want %v", user, tt.wantUser)
			}
			if host != tt.wantHost {
				t.Errorf("parseSSHTarget() host = %v, want %v", host, tt.wantHost)
			}
			if port != tt.wantPort {
				t.Errorf("parseSSHTarget() port = %v, want %v", port, tt.wantPort)
			}
		})
	}
}

func TestConnectionPool(t *testing.T) {
	pool := &connectionPool{
		maxSize: 2,
		conns:   make(map[string]*pooledConn),
	}

	// Test that get returns nil for non-existent connection
	if conn := pool.get("addr1"); conn != nil {
		t.Error("expected nil for non-existent connection")
	}

	// Test closeAll on empty pool
	if err := pool.closeAll(); err != nil {
		t.Errorf("closeAll on empty pool failed: %v", err)
	}
}

func TestNativeExecutorOptions(t *testing.T) {
	// Test options are applied correctly
	e := &NativeExecutor{
		pool: &connectionPool{
			maxSize: 5,
			conns:   make(map[string]*pooledConn),
		},
	}

	// Test WithKeepAlive
	opt := WithKeepAlive(60*time.Second, 30*time.Second)
	opt(e)
	if e.KeepAliveInterval != 60*time.Second {
		t.Errorf("KeepAliveInterval = %v, want %v", e.KeepAliveInterval, 60*time.Second)
	}
	if e.KeepAliveTimeout != 30*time.Second {
		t.Errorf("KeepAliveTimeout = %v, want %v", e.KeepAliveTimeout, 30*time.Second)
	}

	// Test WithPoolSize
	opt2 := WithPoolSize(10)
	opt2(e)
	if e.pool.maxSize != 10 {
		t.Errorf("pool.maxSize = %v, want %v", e.pool.maxSize, 10)
	}

	// Test WithPassphrasePrompt
	prompt := func(path string) (string, error) {
		return "test", nil
	}
	opt3 := WithPassphrasePrompt(prompt)
	opt3(e)
	if e.PassphrasePrompt == nil {
		t.Error("PassphrasePrompt should be set")
	}
}

func TestTargetAddr(t *testing.T) {
	tests := []struct {
		options ConnectionOptions
		want    string
	}{
		{
			options: ConnectionOptions{Host: "example.com"},
			want:    "example.com:22",
		},
		{
			options: ConnectionOptions{Host: "example.com", Port: 2222},
			want:    "example.com:2222",
		},
	}

	for _, tt := range tests {
		t.Run(tt.want, func(t *testing.T) {
			e := &NativeExecutor{options: tt.options}
			if got := e.targetAddr(); got != tt.want {
				t.Errorf("targetAddr() = %v, want %v", got, tt.want)
			}
		})
	}
}
