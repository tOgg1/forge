package models

import (
	"testing"
	"time"
)

func TestFileClaim_IsExpired(t *testing.T) {
	tests := []struct {
		name      string
		expiresAt time.Time
		want      bool
	}{
		{
			name:      "expired",
			expiresAt: time.Now().Add(-time.Hour),
			want:      true,
		},
		{
			name:      "not expired",
			expiresAt: time.Now().Add(time.Hour),
			want:      false,
		},
		{
			name:      "just expired",
			expiresAt: time.Now().Add(-time.Second),
			want:      true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			c := &FileClaim{ExpiresAt: tt.expiresAt}
			if got := c.IsExpired(); got != tt.want {
				t.Errorf("FileClaim.IsExpired() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestFileClaim_IsActive(t *testing.T) {
	tests := []struct {
		name      string
		expiresAt time.Time
		want      bool
	}{
		{
			name:      "active",
			expiresAt: time.Now().Add(time.Hour),
			want:      true,
		},
		{
			name:      "inactive",
			expiresAt: time.Now().Add(-time.Hour),
			want:      false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			c := &FileClaim{ExpiresAt: tt.expiresAt}
			if got := c.IsActive(); got != tt.want {
				t.Errorf("FileClaim.IsActive() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestFileClaimSummary_AddClaim(t *testing.T) {
	summary := NewFileClaimSummary()

	// Add exclusive claim
	summary.AddClaim(FileClaim{
		ID:          1,
		PathPattern: "src/*.go",
		Exclusive:   true,
	})

	if summary.TotalClaims != 1 {
		t.Errorf("TotalClaims = %d, want 1", summary.TotalClaims)
	}
	if summary.ExclusiveClaims != 1 {
		t.Errorf("ExclusiveClaims = %d, want 1", summary.ExclusiveClaims)
	}
	if summary.SharedClaims != 0 {
		t.Errorf("SharedClaims = %d, want 0", summary.SharedClaims)
	}

	// Add shared claim
	summary.AddClaim(FileClaim{
		ID:          2,
		PathPattern: "docs/*.md",
		Exclusive:   false,
	})

	if summary.TotalClaims != 2 {
		t.Errorf("TotalClaims = %d, want 2", summary.TotalClaims)
	}
	if summary.ExclusiveClaims != 1 {
		t.Errorf("ExclusiveClaims = %d, want 1", summary.ExclusiveClaims)
	}
	if summary.SharedClaims != 1 {
		t.Errorf("SharedClaims = %d, want 1", summary.SharedClaims)
	}

	if len(summary.Claims) != 2 {
		t.Errorf("len(Claims) = %d, want 2", len(summary.Claims))
	}
}

func TestFileClaimSummary_AddConflict(t *testing.T) {
	summary := NewFileClaimSummary()

	if summary.HasConflicts {
		t.Error("HasConflicts should be false initially")
	}

	summary.AddConflict(FileClaimConflict{
		Path:     "src/main.go",
		Severity: ConflictSeverityError,
	})

	if !summary.HasConflicts {
		t.Error("HasConflicts should be true after adding conflict")
	}
	if summary.Conflicts != 1 {
		t.Errorf("Conflicts = %d, want 1", summary.Conflicts)
	}
	if len(summary.ConflictDetails) != 1 {
		t.Errorf("len(ConflictDetails) = %d, want 1", len(summary.ConflictDetails))
	}
}

func TestNewFileClaimSummary(t *testing.T) {
	summary := NewFileClaimSummary()

	if summary == nil {
		t.Fatal("NewFileClaimSummary() returned nil")
	}
	if summary.TotalClaims != 0 {
		t.Errorf("TotalClaims = %d, want 0", summary.TotalClaims)
	}
	if summary.HasConflicts {
		t.Error("HasConflicts should be false")
	}
}
