package models

import (
	"errors"
	"fmt"
	"strings"
)

// ValidationError represents a single validation failure.
type ValidationError struct {
	Field   string `json:"field"`
	Message string `json:"message"`
	Cause   error  `json:"-"`
}

func (v ValidationError) Error() string {
	if v.Field == "" {
		return v.Message
	}
	return fmt.Sprintf("%s: %s", v.Field, v.Message)
}

// ValidationErrors aggregates multiple validation failures.
type ValidationErrors struct {
	Errors []ValidationError `json:"errors"`
}

// Add records a validation error for a field.
func (v *ValidationErrors) Add(field string, err error) {
	if err == nil {
		return
	}

	var nested *ValidationErrors
	if errors.As(err, &nested) {
		for _, sub := range nested.Errors {
			v.Errors = append(v.Errors, ValidationError{
				Field:   joinField(field, sub.Field),
				Message: sub.Message,
				Cause:   sub.Cause,
			})
		}
		return
	}

	v.Errors = append(v.Errors, ValidationError{
		Field:   field,
		Message: err.Error(),
		Cause:   err,
	})
}

// AddMessage records a validation error with a custom message.
func (v *ValidationErrors) AddMessage(field, message string) {
	if message == "" {
		return
	}
	v.Errors = append(v.Errors, ValidationError{Field: field, Message: message})
}

// Err returns nil if there are no errors, otherwise returns the validation error.
func (v *ValidationErrors) Err() error {
	if v == nil || len(v.Errors) == 0 {
		return nil
	}
	return v
}

// Error implements error.
func (v *ValidationErrors) Error() string {
	if v == nil || len(v.Errors) == 0 {
		return "validation failed"
	}
	if len(v.Errors) == 1 {
		return v.Errors[0].Error()
	}

	var builder strings.Builder
	for i, err := range v.Errors {
		if i > 0 {
			builder.WriteString("; ")
		}
		builder.WriteString(err.Error())
	}

	return builder.String()
}

// Is allows errors.Is to match nested validation errors.
func (v *ValidationErrors) Is(target error) bool {
	if v == nil {
		return false
	}
	for _, err := range v.Errors {
		if err.Cause != nil && errors.Is(err.Cause, target) {
			return true
		}
	}
	return false
}

func joinField(prefix, field string) string {
	switch {
	case prefix == "":
		return field
	case field == "":
		return prefix
	default:
		return prefix + "." + field
	}
}
