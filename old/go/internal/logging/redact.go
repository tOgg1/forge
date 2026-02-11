package logging

import (
	"regexp"
	"strings"
)

// Sensitive field names that should be redacted.
var sensitiveFields = []string{
	"password",
	"secret",
	"token",
	"api_key",
	"apikey",
	"api-key",
	"authorization",
	"auth",
	"credential",
	"private_key",
	"privatekey",
	"access_key",
	"accesskey",
}

// Patterns for secrets that should be redacted.
var secretPatterns = []*regexp.Regexp{
	// API keys (common formats)
	regexp.MustCompile(`(?i)(sk-[a-zA-Z0-9]{20,})`),                     // OpenAI style
	regexp.MustCompile(`(?i)(anthropic-[a-zA-Z0-9]{20,})`),              // Anthropic style
	regexp.MustCompile(`(?i)(AIza[a-zA-Z0-9_-]{35})`),                   // Google API key
	regexp.MustCompile(`(?i)(ghp_[a-zA-Z0-9]{36})`),                     // GitHub PAT
	regexp.MustCompile(`(?i)(gho_[a-zA-Z0-9]{36})`),                     // GitHub OAuth
	regexp.MustCompile(`(?i)(github_pat_[a-zA-Z0-9]{22}_[a-zA-Z0-9]+)`), // GitHub fine-grained PAT

	// Bearer tokens
	regexp.MustCompile(`(?i)bearer\s+([a-zA-Z0-9._-]{20,})`),

	// Generic long hex/base64 strings that look like secrets
	regexp.MustCompile(`(?i)(key|token|secret|password|auth)[=:]["']?([a-zA-Z0-9+/=_-]{32,})["']?`),
}

// RedactedValue is the replacement for sensitive values.
const RedactedValue = "[REDACTED]"

// Redact replaces sensitive information in a string.
func Redact(s string) string {
	result := s

	// Apply pattern-based redaction
	for _, pattern := range secretPatterns {
		result = pattern.ReplaceAllString(result, RedactedValue)
	}

	return result
}

// RedactMap redacts sensitive fields in a map.
func RedactMap(m map[string]interface{}) map[string]interface{} {
	result := make(map[string]interface{}, len(m))

	for k, v := range m {
		lowerKey := strings.ToLower(k)

		// Check if this is a sensitive field
		isSensitive := false
		for _, field := range sensitiveFields {
			if strings.Contains(lowerKey, field) {
				isSensitive = true
				break
			}
		}

		if isSensitive {
			result[k] = RedactedValue
		} else if nested, ok := v.(map[string]interface{}); ok {
			result[k] = RedactMap(nested)
		} else if str, ok := v.(string); ok {
			result[k] = Redact(str)
		} else {
			result[k] = v
		}
	}

	return result
}

// RedactEnv redacts environment variables, returning a safe copy.
func RedactEnv(env []string) []string {
	result := make([]string, len(env))

	for i, e := range env {
		parts := strings.SplitN(e, "=", 2)
		if len(parts) != 2 {
			result[i] = e
			continue
		}

		key := parts[0]
		lowerKey := strings.ToLower(key)

		// Check if this is a sensitive field
		isSensitive := false
		for _, field := range sensitiveFields {
			if strings.Contains(lowerKey, field) {
				isSensitive = true
				break
			}
		}

		if isSensitive {
			result[i] = key + "=" + RedactedValue
		} else {
			result[i] = key + "=" + Redact(parts[1])
		}
	}

	return result
}

// IsSensitiveField checks if a field name is considered sensitive.
func IsSensitiveField(name string) bool {
	lowerName := strings.ToLower(name)
	for _, field := range sensitiveFields {
		if strings.Contains(lowerName, field) {
			return true
		}
	}
	return false
}
