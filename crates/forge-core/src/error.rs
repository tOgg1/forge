//! Error types for the Forge system.
//!
//! Provides a common error enum used across Forge crates, plus integration
//! with the validation framework.

use std::fmt;

/// Top-level error type for Forge operations.
#[derive(Debug, Clone)]
pub enum ForgeError {
    /// A validation constraint was violated.
    Validation(String),
    /// A referenced entity was not found.
    NotFound(String),
    /// An internal/unexpected error.
    Internal(String),
}

impl fmt::Display for ForgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(msg) => write!(f, "validation error: {msg}"),
            Self::NotFound(msg) => write!(f, "not found: {msg}"),
            Self::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for ForgeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let e = ForgeError::Validation("bad name".into());
        assert_eq!(e.to_string(), "validation error: bad name");

        let e = ForgeError::NotFound("node xyz".into());
        assert_eq!(e.to_string(), "not found: node xyz");

        let e = ForgeError::Internal("unexpected".into());
        assert_eq!(e.to_string(), "internal error: unexpected");
    }

    #[test]
    fn error_is_std_error() {
        let e: Box<dyn std::error::Error> = Box::new(ForgeError::Internal("test".into()));
        assert!(e.to_string().contains("internal error"));
    }
}
