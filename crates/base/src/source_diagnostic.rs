use crate::diagnostic_level::DiagnosticLevel;
use crate::file_path::FilePath;
use crate::shared_string::SharedString;
use crate::source_excerpt::SourceExcerpt;

/// Structured source diagnostic with excerpts and inline annotations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDiagnostic {
    /// Severity level of the diagnostic.
    pub level: DiagnosticLevel,
    /// Primary file associated with the diagnostic.
    pub file_path: FilePath,
    /// User-facing summary message.
    pub message: SharedString,
    /// Relevant excerpts that help explain the diagnostic.
    pub excerpts: Vec<SourceExcerpt>,
}

impl SourceDiagnostic {
    /// Creates a source diagnostic from its level, file path, and message.
    pub fn new(
        level: DiagnosticLevel,
        file_path: impl Into<FilePath>,
        message: impl Into<SharedString>,
    ) -> Self {
        Self {
            level,
            file_path: file_path.into(),
            message: message.into(),
            excerpts: Vec::new(),
        }
    }

    /// Returns a copy of this diagnostic with one appended excerpt.
    pub fn with_excerpt(mut self, excerpt: SourceExcerpt) -> Self {
        self.excerpts.push(excerpt);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::SourceDiagnostic;
    use crate::diagnostic_level::DiagnosticLevel;
    use crate::source_annotation::SourceAnnotation;
    use crate::source_excerpt::SourceExcerpt;
    use crate::span::Span;

    #[test]
    fn source_diagnostic_tracks_level_file_and_excerpts() {
        let diagnostic = SourceDiagnostic::new(
            DiagnosticLevel::Error,
            "examples/test.pixui",
            "unresolved identifier `value`",
        )
        .with_excerpt(
            SourceExcerpt::new("examples/test.pixui", 2, "println(value);")
                .with_annotation(SourceAnnotation::new(Span::new(8, 13), "not found")),
        );

        assert_eq!(diagnostic.level, DiagnosticLevel::Error);
        assert_eq!(diagnostic.file_path.as_str(), "examples/test.pixui");
        assert_eq!(diagnostic.message, "unresolved identifier `value`");
        assert_eq!(diagnostic.excerpts.len(), 1);
        assert_eq!(diagnostic.excerpts[0].line_number, 2);
        assert_eq!(diagnostic.excerpts[0].annotations.len(), 1);
    }
}
