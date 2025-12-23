package models

import "time"

// FileClaim represents a file reservation/claim from Agent Mail.
// This tracks which agent has claimed exclusive or shared access to files.
type FileClaim struct {
	// ID is the unique identifier for the claim.
	ID int64 `json:"id"`

	// AgentID is the Swarm agent ID that holds this claim.
	AgentID string `json:"agent_id"`

	// AgentMailName is the Agent Mail agent name (e.g., "GreenCastle").
	AgentMailName string `json:"agent_mail_name,omitempty"`

	// PathPattern is the file path or glob pattern being claimed.
	PathPattern string `json:"path_pattern"`

	// Exclusive indicates if this is an exclusive claim (true) or shared (false).
	Exclusive bool `json:"exclusive"`

	// Reason explains why the claim was made.
	Reason string `json:"reason,omitempty"`

	// ExpiresAt is when the claim expires.
	ExpiresAt time.Time `json:"expires_at"`

	// CreatedAt is when the claim was created.
	CreatedAt time.Time `json:"created_at"`
}

// IsExpired returns true if the claim has expired.
func (c *FileClaim) IsExpired() bool {
	return time.Now().After(c.ExpiresAt)
}

// IsActive returns true if the claim is still valid.
func (c *FileClaim) IsActive() bool {
	return !c.IsExpired()
}

// FileClaimConflict represents a conflict between file claims.
type FileClaimConflict struct {
	// Path is the conflicting file path or pattern.
	Path string `json:"path"`

	// Claims are the conflicting claims on this path.
	Claims []FileClaim `json:"claims"`

	// Severity indicates how serious the conflict is.
	Severity ConflictSeverity `json:"severity"`
}

// ConflictSeverity indicates the severity of a file claim conflict.
type ConflictSeverity string

const (
	// ConflictSeverityWarning indicates a potential conflict (overlapping patterns).
	ConflictSeverityWarning ConflictSeverity = "warning"

	// ConflictSeverityError indicates a definite conflict (same exact path, both exclusive).
	ConflictSeverityError ConflictSeverity = "error"
)

// FileClaimSummary provides a summary of claims for an agent.
type FileClaimSummary struct {
	// TotalClaims is the number of active file claims.
	TotalClaims int `json:"total_claims"`

	// ExclusiveClaims is the number of exclusive claims.
	ExclusiveClaims int `json:"exclusive_claims"`

	// SharedClaims is the number of shared claims.
	SharedClaims int `json:"shared_claims"`

	// Conflicts is the number of detected conflicts.
	Conflicts int `json:"conflicts"`

	// HasConflicts indicates if there are any conflicts.
	HasConflicts bool `json:"has_conflicts"`

	// Claims is the list of active claims (optional, for detailed views).
	Claims []FileClaim `json:"claims,omitempty"`

	// ConflictDetails contains conflict information (optional).
	ConflictDetails []FileClaimConflict `json:"conflict_details,omitempty"`
}

// NewFileClaimSummary creates an empty FileClaimSummary.
func NewFileClaimSummary() *FileClaimSummary {
	return &FileClaimSummary{}
}

// AddClaim adds a claim to the summary.
func (s *FileClaimSummary) AddClaim(claim FileClaim) {
	s.TotalClaims++
	if claim.Exclusive {
		s.ExclusiveClaims++
	} else {
		s.SharedClaims++
	}
	s.Claims = append(s.Claims, claim)
}

// AddConflict adds a conflict to the summary.
func (s *FileClaimSummary) AddConflict(conflict FileClaimConflict) {
	s.Conflicts++
	s.HasConflicts = true
	s.ConflictDetails = append(s.ConflictDetails, conflict)
}
