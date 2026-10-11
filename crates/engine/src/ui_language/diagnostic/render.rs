//! Plain-text formatting; diagnostic data is independent of terminal output.
use super::report::Diagnostic;
use crate::ui_language::source::snapshot::Source;
pub fn render(source: &Source, diagnostic: &Diagnostic) -> String {
    let mut output = format!(
        "{:?}[{}]: {}\n",
        diagnostic.stage, diagnostic.code, diagnostic.message
    );
    for (span, message) in std::iter::once((diagnostic.primary, "here")).chain(
        diagnostic
            .secondary
            .iter()
            .map(|label| (label.span, label.message.as_str())),
    ) {
        if span.source != source.id {
            continue;
        }
        if let Some((line, column)) = source.position(span.start) {
            let excerpt = source.line(line).unwrap_or("").replace('\t', " ");
            let width = source
                .slice(span)
                .unwrap_or("")
                .split('\n')
                .next()
                .unwrap_or("")
                .chars()
                .count()
                .max(1);
            output.push_str(&format!(
                " --> {}:{line}:{column}\n {line} | {excerpt}\n   | {}{} {message}\n",
                source.filename,
                " ".repeat(column - 1),
                "^".repeat(width)
            ));
        }
    }
    for note in &diagnostic.notes {
        output.push_str(&format!(" note: {note}\n"));
    }
    output
}
