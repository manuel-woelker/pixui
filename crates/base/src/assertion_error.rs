use crate::diagnostic_level::DiagnosticLevel;
use crate::file_path::FilePath;
use crate::line_bounds::LineBounds;
use crate::render_source_diagnostics::render_source_diagnostics;
use crate::shared_string::SharedString;
use crate::source_annotation::SourceAnnotation;
use crate::source_diagnostic::SourceDiagnostic;
use crate::source_excerpt::SourceExcerpt;
use crate::source_file::SourceFile;
use crate::span::Span;

/// Structured assertion failure data for test-oriented runtime errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionError {
    pub diagnostic: SourceDiagnostic,
    pub expected: Option<SharedString>,
    pub actual: Option<SharedString>,
}

impl AssertionError {
    /// Creates an assertion error for a source-file span and rendered values.
    pub fn new(
        source_file: &SourceFile,
        span: Span,
        summary: impl Into<SharedString>,
        expected: impl Into<SharedString>,
        actual: impl Into<SharedString>,
    ) -> Self {
        let line_bounds = LineBounds::new(source_file.source(), span.start());
        let source_line = &source_file.source()[line_bounds.line_start..line_bounds.line_end];
        let relative_start = span.start().saturating_sub(line_bounds.line_start);
        let relative_end = span.end().saturating_sub(line_bounds.line_start);
        let summary = summary.into();
        let diagnostic = SourceDiagnostic::new(DiagnosticLevel::Error, &source_file.path, summary)
            .with_excerpt(
                SourceExcerpt::new(&source_file.path, line_bounds.line_number, source_line)
                    .with_annotation(SourceAnnotation::new(
                        Span::new(relative_start, relative_end),
                        "assertion failed here",
                    )),
            );

        Self {
            diagnostic,
            expected: Some(expected.into()),
            actual: Some(actual.into()),
        }
    }

    /// Creates an assertion error for a source-file span without an expected/actual diff.
    pub fn new_without_diff(
        source_file: &SourceFile,
        span: Span,
        summary: impl Into<SharedString>,
    ) -> Self {
        let line_bounds = LineBounds::new(source_file.source(), span.start());
        let source_line = &source_file.source()[line_bounds.line_start..line_bounds.line_end];
        let relative_start = span.start().saturating_sub(line_bounds.line_start);
        let relative_end = span.end().saturating_sub(line_bounds.line_start);
        let summary = summary.into();
        let diagnostic = SourceDiagnostic::new(DiagnosticLevel::Error, &source_file.path, summary)
            .with_excerpt(
                SourceExcerpt::new(&source_file.path, line_bounds.line_number, source_line)
                    .with_annotation(SourceAnnotation::new(
                        Span::new(relative_start, relative_end),
                        "assertion failed here",
                    )),
            );

        Self {
            diagnostic,
            expected: None,
            actual: None,
        }
    }

    /// Returns the assertion summary message.
    pub fn summary(&self) -> &SharedString {
        &self.diagnostic.message
    }

    /// Returns the logical file path for this assertion failure.
    pub fn file_path(&self) -> &FilePath {
        &self.diagnostic.file_path
    }
}

/// Renders an assertion error as a source diagnostic and an optional short diff.
pub fn render_assertion_error(assertion_error: &AssertionError) -> SharedString {
    let rendered_diagnostic = strip_column_from_assertion_diagnostic(&render_source_diagnostics(
        std::slice::from_ref(&assertion_error.diagnostic),
    ));

    match (&assertion_error.expected, &assertion_error.actual) {
        (Some(expected), Some(actual)) => crate::shared_format!(
            "{rendered_diagnostic}\n\nexpected: {expected}\nactual:   {actual}"
        ),
        _ => rendered_diagnostic,
    }
}

fn strip_column_from_assertion_diagnostic(rendered: &str) -> SharedString {
    let mut updated_lines = Vec::new();

    for line in rendered.lines() {
        if line.contains("╭▸ ") {
            updated_lines.push(strip_trailing_column_number(line));
        } else {
            updated_lines.push(line.to_owned());
        }
    }

    SharedString::from(updated_lines.join("\n"))
}

fn strip_trailing_column_number(line: &str) -> String {
    let Some(last_colon_index) = line.rfind(':') else {
        return line.to_owned();
    };

    if line[last_colon_index + 1..]
        .chars()
        .all(|character| character.is_ascii_digit())
    {
        line[..last_colon_index].to_owned()
    } else {
        line.to_owned()
    }
}

impl std::fmt::Display for AssertionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.summary())
    }
}

#[cfg(test)]
mod tests {
    use super::AssertionError;
    use super::render_assertion_error;
    use crate::source_file::SourceFile;
    use crate::span::Span;
    use crate::unansi;
    use expect_test::expect;

    #[test]
    fn renders_assertion_errors_with_a_source_diagnostic_and_diff() {
        let source_file = SourceFile::new("examples/tests.pixui", "assert_eq(\"a\", \"b\");");
        let assertion_error = AssertionError::new(
            &source_file,
            Span::new(0, source_file.source().len() - 1),
            "assert_eq values differ",
            "\"a\"",
            "\"b\"",
        );

        let rendered = unansi(&render_assertion_error(&assertion_error));

        expect![[r#"
            error: assert_eq values differ
              ╭▸ examples/tests.pixui:1
              │
            1 │ assert_eq("a", "b");
              ╰╴━━━━━━━━━━━━━━━━━━━ assertion failed here
            at examples/tests.pixui:1:1

            expected: "a"
            actual:   "b""#]]
        .assert_eq(&rendered);
    }

    #[test]
    fn renders_assertion_errors_without_a_diff_block() {
        let source_file = SourceFile::new("examples/tests.pixui", "assert(false);");
        let assertion_error = AssertionError::new_without_diff(
            &source_file,
            Span::new(0, source_file.source().len() - 1),
            "assert condition was false",
        );

        let rendered = unansi(&render_assertion_error(&assertion_error));

        expect![[r#"
            error: assert condition was false
              ╭▸ examples/tests.pixui:1
              │
            1 │ assert(false);
              ╰╴━━━━━━━━━━━━━ assertion failed here
            at examples/tests.pixui:1:1"#]]
        .assert_eq(&rendered);
    }
}
