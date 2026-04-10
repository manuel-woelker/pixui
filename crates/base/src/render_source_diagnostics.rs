use crate::diagnostic_level::DiagnosticLevel;
use crate::shared_string::SharedString;
use crate::source_annotation::SourceAnnotation;
use crate::source_diagnostic::SourceDiagnostic;
use crate::source_excerpt::SourceExcerpt;
use annotate_snippets::Annotation;
use annotate_snippets::AnnotationKind;
use annotate_snippets::Group;
use annotate_snippets::Level;
use annotate_snippets::Renderer;
use annotate_snippets::Snippet;
use annotate_snippets::renderer::DecorStyle;

/// Renders source diagnostics into a displayable string.
pub fn render_source_diagnostics(diagnostics: &[SourceDiagnostic]) -> SharedString {
    render_source_diagnostics_with_renderer(
        diagnostics,
        Renderer::styled().decor_style(DecorStyle::Unicode),
    )
}

fn render_source_diagnostics_with_renderer(
    diagnostics: &[SourceDiagnostic],
    renderer: Renderer,
) -> SharedString {
    let mut rendered_groups = Vec::new();

    for diagnostic in diagnostics {
        let mut rendered = renderer.render(&[render_group(diagnostic)]).to_string();

        if let Some(location_line) = source_diagnostic_location_line(diagnostic) {
            rendered.push('\n');
            rendered.push_str(location_line.as_str());
        }

        rendered_groups.push(rendered);
    }

    rendered_groups.join("\n\n").into()
}

fn render_group<'a>(diagnostic: &'a SourceDiagnostic) -> Group<'a> {
    let mut group =
        Group::with_title(level_for(diagnostic.level).primary_title(diagnostic.message.as_str()));

    for excerpt in &diagnostic.excerpts {
        group = group.element(render_excerpt(excerpt));
    }

    group
}

fn render_excerpt<'a>(excerpt: &'a SourceExcerpt) -> Snippet<'a, Annotation<'a>> {
    Snippet::source(excerpt.source_line.as_str())
        .line_start(excerpt.line_number)
        .path(excerpt.file_path.as_str())
        .fold(false)
        .annotations(excerpt.annotations.iter().map(render_annotation))
}

fn render_annotation<'a>(annotation: &'a SourceAnnotation) -> Annotation<'a> {
    AnnotationKind::Primary
        .span(annotation.span.as_range())
        .label(annotation.message.as_str())
}

fn level_for(level: DiagnosticLevel) -> Level<'static> {
    match level {
        DiagnosticLevel::Error => Level::ERROR,
        DiagnosticLevel::Warning => Level::WARNING,
    }
}

fn source_diagnostic_location_line(diagnostic: &SourceDiagnostic) -> Option<SharedString> {
    let excerpt = diagnostic.excerpts.first()?;
    let column_number = excerpt
        .annotations
        .first()
        .map_or(1, |annotation| annotation.span.start() + 1);
    Some(crate::shared_format!(
        "at {}:{}:{}",
        excerpt.file_path,
        excerpt.line_number,
        column_number
    ))
}

#[cfg(test)]
mod tests {
    use super::render_source_diagnostics;
    use super::render_source_diagnostics_with_renderer;
    use crate::diagnostic_level::DiagnosticLevel;
    use crate::source_annotation::SourceAnnotation;
    use crate::source_diagnostic::SourceDiagnostic;
    use crate::source_excerpt::SourceExcerpt;
    use crate::span::Span;
    use crate::unansi;
    use annotate_snippets::Renderer;
    use annotate_snippets::renderer::DecorStyle;
    use expect_test::expect;

    #[test]
    fn renders_plain_source_diagnostic_with_excerpt() {
        let diagnostic = SourceDiagnostic::new(
            DiagnosticLevel::Error,
            "examples/test.pixui",
            "unresolved identifier `value`",
        )
        .with_excerpt(
            SourceExcerpt::new("examples/test.pixui", 3, "println(value);")
                .with_annotation(SourceAnnotation::new(Span::new(8, 13), "not found")),
        );

        let rendered = render_source_diagnostics_with_renderer(
            &[diagnostic],
            Renderer::plain().decor_style(DecorStyle::Unicode),
        )
        .to_string();

        expect![[r#"
            error: unresolved identifier `value`
              ╭▸ examples/test.pixui:3:9
              │
            3 │ println(value);
              ╰╴        ━━━━━ not found
            at examples/test.pixui:3:9"#]]
        .assert_eq(&rendered);
    }

    #[test]
    fn renders_styled_source_diagnostic() {
        let diagnostic = SourceDiagnostic::new(
            DiagnosticLevel::Warning,
            "examples/test.pixui",
            "unused value",
        )
        .with_excerpt(
            SourceExcerpt::new("examples/test.pixui", 2, "println(value);")
                .with_annotation(SourceAnnotation::new(Span::new(8, 13), "never read")),
        );

        let rendered = unansi(&render_source_diagnostics(&[diagnostic]));

        expect![[r#"
            warning: unused value
              ╭▸ examples/test.pixui:2:9
              │
            2 │ println(value);
              ╰╴        ━━━━━ never read
            at examples/test.pixui:2:9"#]]
        .assert_eq(&rendered);
    }
}
