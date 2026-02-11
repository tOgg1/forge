package logging

import (
	"testing"
)

func TestRedact(t *testing.T) {
	tests := []struct {
		name     string
		input    string
		expected string
	}{
		{
			name:     "OpenAI API key",
			input:    "Using key sk-abcdefghijklmnopqrstuvwxyz123456",
			expected: "Using key [REDACTED]",
		},
		{
			name:     "GitHub PAT",
			input:    "Token: ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
			expected: "Token: [REDACTED]",
		},
		{
			name:     "Bearer token",
			input:    "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9",
			expected: "Authorization: [REDACTED]",
		},
		{
			name:     "No sensitive data",
			input:    "Hello world, this is a test",
			expected: "Hello world, this is a test",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := Redact(tt.input)
			if result != tt.expected {
				t.Errorf("Redact() = %q, want %q", result, tt.expected)
			}
		})
	}
}

func TestRedactEnv(t *testing.T) {
	env := []string{
		"PATH=/usr/bin",
		"API_KEY=secret123456789012345678901234567890",
		"HOME=/home/user",
		"ANTHROPIC_API_KEY=anthropic-key-value",
	}

	result := RedactEnv(env)

	if result[0] != "PATH=/usr/bin" {
		t.Errorf("PATH should not be redacted: %s", result[0])
	}

	if result[1] != "API_KEY=[REDACTED]" {
		t.Errorf("API_KEY should be redacted: %s", result[1])
	}

	if result[2] != "HOME=/home/user" {
		t.Errorf("HOME should not be redacted: %s", result[2])
	}

	if result[3] != "ANTHROPIC_API_KEY=[REDACTED]" {
		t.Errorf("ANTHROPIC_API_KEY should be redacted: %s", result[3])
	}
}

func TestIsSensitiveField(t *testing.T) {
	tests := []struct {
		name      string
		sensitive bool
	}{
		{"password", true},
		{"Password", true},
		{"user_password", true},
		{"api_key", true},
		{"API_KEY", true},
		{"token", true},
		{"access_token", true},
		{"username", false},
		{"email", false},
		{"name", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := IsSensitiveField(tt.name)
			if result != tt.sensitive {
				t.Errorf("IsSensitiveField(%q) = %v, want %v", tt.name, result, tt.sensitive)
			}
		})
	}
}

func TestRedactMap(t *testing.T) {
	input := map[string]interface{}{
		"username": "john",
		"password": "secret123",
		"nested": map[string]interface{}{
			"api_key": "key123",
			"name":    "test",
		},
	}

	result := RedactMap(input)

	if result["username"] != "john" {
		t.Errorf("username should not be redacted")
	}

	if result["password"] != RedactedValue {
		t.Errorf("password should be redacted")
	}

	nested := result["nested"].(map[string]interface{})
	if nested["api_key"] != RedactedValue {
		t.Errorf("nested api_key should be redacted")
	}

	if nested["name"] != "test" {
		t.Errorf("nested name should not be redacted")
	}
}
