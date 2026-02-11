package models

import (
	"errors"
	"testing"
)

func TestValidationErrorsIs(t *testing.T) {
	validation := &ValidationErrors{}
	validation.Add("name", ErrInvalidNodeName)

	err := validation.Err()
	if err == nil {
		t.Fatal("expected error")
	}
	if !errors.Is(err, ErrInvalidNodeName) {
		t.Fatalf("expected errors.Is to match ErrInvalidNodeName, got %v", err)
	}
}

func TestValidationErrorsNestedFields(t *testing.T) {
	nested := &ValidationErrors{}
	nested.AddMessage("text", "message text is required")

	validation := &ValidationErrors{}
	validation.Add("payload", nested)

	err := validation.Err()
	if err == nil {
		t.Fatal("expected error")
	}

	list, ok := err.(*ValidationErrors)
	if !ok {
		t.Fatalf("expected ValidationErrors type, got %T", err)
	}
	if len(list.Errors) != 1 {
		t.Fatalf("expected 1 error, got %d", len(list.Errors))
	}
	if list.Errors[0].Field != "payload.text" {
		t.Fatalf("expected field payload.text, got %q", list.Errors[0].Field)
	}
}
