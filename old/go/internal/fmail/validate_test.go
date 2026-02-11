package fmail

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestNormalizeTopic(t *testing.T) {
	normalized, err := NormalizeTopic("Build-Status")
	require.NoError(t, err)
	require.Equal(t, "build-status", normalized)
}

func TestValidateTopic(t *testing.T) {
	valid := []string{"task", "build-status", "a1", "status123"}
	for _, name := range valid {
		require.NoError(t, ValidateTopic(name))
	}

	invalid := []string{"Task", "task_ok", "task status", "@task", "", "TASK"}
	for _, name := range invalid {
		require.Error(t, ValidateTopic(name))
	}
}

func TestNormalizeAgentName(t *testing.T) {
	normalized, err := NormalizeAgentName("Reviewer-1")
	require.NoError(t, err)
	require.Equal(t, "reviewer-1", normalized)
}

func TestValidateAgentName(t *testing.T) {
	valid := []string{"architect", "coder-1", "reviewer"}
	for _, name := range valid {
		require.NoError(t, ValidateAgentName(name))
	}

	invalid := []string{"Reviewer", "agent_1", "agent 1", "", "@agent"}
	for _, name := range invalid {
		require.Error(t, ValidateAgentName(name))
	}
}

func TestNormalizeTarget(t *testing.T) {
	target, isDM, err := NormalizeTarget("@Reviewer")
	require.NoError(t, err)
	require.True(t, isDM)
	require.Equal(t, "@reviewer", target)

	target, isDM, err = NormalizeTarget("Task")
	require.NoError(t, err)
	require.False(t, isDM)
	require.Equal(t, "task", target)
}

func TestValidateTag(t *testing.T) {
	valid := []string{"urgent", "auth", "bug-fix", "v1", "a"}
	for _, tag := range valid {
		require.NoError(t, ValidateTag(tag), "expected %q to be valid", tag)
	}

	invalid := []string{"Urgent", "auth_bug", "bug fix", "", "@tag"}
	for _, tag := range invalid {
		require.Error(t, ValidateTag(tag), "expected %q to be invalid", tag)
	}
}

func TestValidateTagLength(t *testing.T) {
	longTag := make([]byte, MaxTagLength+1)
	for i := range longTag {
		longTag[i] = 'a'
	}
	require.Error(t, ValidateTag(string(longTag)))

	validTag := make([]byte, MaxTagLength)
	for i := range validTag {
		validTag[i] = 'a'
	}
	require.NoError(t, ValidateTag(string(validTag)))
}

func TestValidateTags(t *testing.T) {
	require.NoError(t, ValidateTags(nil))
	require.NoError(t, ValidateTags([]string{}))
	require.NoError(t, ValidateTags([]string{"urgent", "auth"}))

	// Too many tags
	manyTags := make([]string, MaxTagsPerMessage+1)
	for i := range manyTags {
		manyTags[i] = "tag"
	}
	require.Error(t, ValidateTags(manyTags))
}

func TestNormalizeTags(t *testing.T) {
	normalized, err := NormalizeTags([]string{"URGENT", "Auth", "bug-fix"})
	require.NoError(t, err)
	require.Equal(t, []string{"urgent", "auth", "bug-fix"}, normalized)

	// Dedupe
	normalized, err = NormalizeTags([]string{"urgent", "URGENT", "auth"})
	require.NoError(t, err)
	require.Equal(t, []string{"urgent", "auth"}, normalized)

	// Empty
	normalized, err = NormalizeTags(nil)
	require.NoError(t, err)
	require.Nil(t, normalized)

	// Invalid tag
	_, err = NormalizeTags([]string{"valid", "inv@lid"})
	require.Error(t, err)
}
