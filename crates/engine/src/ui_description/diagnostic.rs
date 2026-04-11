use pixui_base::diagnostic_level::DiagnosticLevel;
use pixui_base::error::PixuiError;
use pixui_base::line_bounds::LineBounds;
use pixui_base::source_annotation::SourceAnnotation;
use pixui_base::source_diagnostic::SourceDiagnostic;
use pixui_base::source_excerpt::SourceExcerpt;
use pixui_base::source_file::SourceFile;
use pixui_base::span::Span;

pub fn ui_description_error(
    source_file: &SourceFile,
    span: Span,
    summary: impl Into<String>,
    annotation: impl Into<String>,
) -> PixuiError {
    let span = clamped_span(source_file, span);
    let line_bounds = LineBounds::new(source_file.source(), span.start());
    let source_line = &source_file.source()[line_bounds.line_start..line_bounds.line_end];
    let relative_start = span.start().saturating_sub(line_bounds.line_start);
    let relative_end = span.end().saturating_sub(line_bounds.line_start);

    PixuiError::runtime_error(
        SourceDiagnostic::new(DiagnosticLevel::Error, &source_file.path, summary.into())
            .with_excerpt(
                SourceExcerpt::new(&source_file.path, line_bounds.line_number, source_line)
                    .with_annotation(SourceAnnotation::new(
                        Span::new(relative_start, relative_end),
                        annotation.into(),
                    )),
            ),
    )
}

fn clamped_span(source_file: &SourceFile, span: Span) -> Span {
    let source_len = source_file.source().len();
    let start = span.start().min(source_len);
    let mut end = span.end().min(source_len);

    if start == end && start < source_len {
        end = start + 1;
    }

    Span::new(start, end)
}
