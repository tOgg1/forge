//! Validation framework for composable error aggregation.
//!
//! Mirrors Go `internal/models/validation.go` — collects multiple field-level
//! errors into one report.

use std::fmt;

/// A single validation error tied to a field.
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

/// Aggregated collection of validation errors.
#[derive(Debug, Clone, Default)]
pub struct ValidationErrors {
    errors: Vec<ValidationError>,
}

impl ValidationErrors {
    /// Create an empty error collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a field-level validation error.
    pub fn add(&mut self, field: impl Into<String>, message: impl Into<String>) {
        self.errors.push(ValidationError {
            field: field.into(),
            message: message.into(),
        });
    }

    /// Returns `true` if there are no errors.
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    /// Returns the number of errors.
    pub fn len(&self) -> usize {
        self.errors.len()
    }

    /// Returns a slice of all collected errors.
    pub fn errors(&self) -> &[ValidationError] {
        &self.errors
    }
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, e) in self.errors.iter().enumerate() {
            if i > 0 {
                write!(f, "; ")?;
            }
            write!(f, "{e}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_validation() {
        let v = ValidationErrors::new();
        assert!(v.is_empty());
        assert_eq!(v.len(), 0);
        assert_eq!(v.to_string(), "");
    }

    #[test]
    fn add_and_display() {
        let mut v = ValidationErrors::new();
        v.add("name", "must not be empty");
        v.add("port", "must be > 0");
        assert!(!v.is_empty());
        assert_eq!(v.len(), 2);
        assert_eq!(v.to_string(), "name: must not be empty; port: must be > 0");
    }

    #[test]
    fn errors_slice() {
        let mut v = ValidationErrors::new();
        v.add("field1", "err1");
        let errs = v.errors();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].field, "field1");
        assert_eq!(errs[0].message, "err1");
    }
}
