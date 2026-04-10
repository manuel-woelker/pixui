/// Severity level for a source diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticLevel {
    Error,
    Warning,
}

#[cfg(test)]
mod tests {
    use super::DiagnosticLevel;

    #[test]
    fn diagnostic_levels_are_comparable() {
        assert_eq!(DiagnosticLevel::Error, DiagnosticLevel::Error);
        assert_ne!(DiagnosticLevel::Error, DiagnosticLevel::Warning);
    }
}
