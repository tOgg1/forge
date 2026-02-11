package tmux

import "testing"

func TestParseVersion(t *testing.T) {
	tests := []struct {
		input   string
		major   int
		minor   int
		wantErr bool
	}{
		{input: "tmux 3.3a", major: 3, minor: 3},
		{input: "3.1", major: 3, minor: 1},
		{input: "tmux 2", major: 2, minor: 0},
		{input: "tmux", wantErr: true},
		{input: "invalid", wantErr: true},
	}

	for _, tt := range tests {
		t.Run(tt.input, func(t *testing.T) {
			version, err := ParseVersion(tt.input)
			if tt.wantErr {
				if err == nil {
					t.Fatalf("expected error for %q", tt.input)
				}
				return
			}
			if err != nil {
				t.Fatalf("ParseVersion failed: %v", err)
			}
			if version.Major != tt.major || version.Minor != tt.minor {
				t.Fatalf("expected %d.%d, got %d.%d", tt.major, tt.minor, version.Major, version.Minor)
			}
		})
	}
}
