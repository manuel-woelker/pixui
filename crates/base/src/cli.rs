use crate::assertion_error::render_assertion_error;
use crate::error::ErrorKind;
use crate::error::PixuiError;
use crate::render_source_diagnostics::render_source_diagnostics;
use crate::result::PixuiResult;
use std::fmt::Write as _;
use std::process::ExitCode;

/// Runs a fallible CLI entrypoint and converts the result into a process exit code.
///
/// It runs a fallible entrypoint, prints a readable error report on failure,
/// and converts the outcome into a process exit code.
pub fn try_main(run: impl FnOnce() -> PixuiResult<()>) -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprint!("{}", format_cli_error("operation failed", &error));
            ExitCode::FAILURE
        }
    }
}

/// Runs a fallible CLI entrypoint with a custom top-level error headline.
///
/// It behaves like [`try_main`], but lets a binary choose a more specific
/// headline for its top-level error report.
pub fn try_main_with_headline(headline: &str, run: impl FnOnce() -> PixuiResult<()>) -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprint!("{}", format_cli_error(headline, &error));
            ExitCode::FAILURE
        }
    }
}

/// Returns a stable, human-readable rendering of an [`PixuiError`] for CLI output.
///
/// It returns a stable, human-readable rendering of a [`PixuiError`] that is
/// suitable for printing from a command-line binary.
pub fn format_cli_error(headline: &str, error: &PixuiError) -> String {
    if let Some(rendered_diagnostics) = special_error_output(error) {
        return ensure_trailing_newline(rendered_diagnostics);
    }

    let mut rendered = String::new();
    let _ = writeln!(&mut rendered, "\u{1b}[1;31m━━ {}\u{1b}[0m", headline);
    if error.write_to(&mut rendered).is_err() {
        let _ = writeln!(
            &mut rendered,
            "\u{1b}[1;31m× error\u{1b}[0m failed to render detailed error output"
        );
    }

    let mut causes = Vec::new();
    let mut current = error.source();
    while let Some(cause) = current {
        causes.push((cause.kind().to_string(), cause.location()));
        current = cause.source();
    }

    if !causes.is_empty() {
        let simple_causes: Vec<_> = causes
            .iter()
            .filter(|(cause, _)| !cause.contains('\n'))
            .collect();
        if simple_causes.is_empty() {
            return rendered;
        }

        rendered.push('\n');
        rendered.push_str("\u{1b}[1;33m━━ cause chain\u{1b}[0m\n");
        for (cause, location) in simple_causes {
            let _ = writeln!(&mut rendered, "  • {}", cause);
            let _ = writeln!(
                &mut rendered,
                "    at {}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            );
        }
    }

    rendered
}

fn special_error_output(error: &PixuiError) -> Option<String> {
    match error.kind() {
        ErrorKind::AssertionError(assertion_error) => {
            Some(render_assertion_error(assertion_error).to_string())
        }
        ErrorKind::RuntimeError(diagnostic) => {
            Some(render_source_diagnostics(std::slice::from_ref(diagnostic.as_ref())).to_string())
        }
        ErrorKind::CompilationError(_) => error
            .source()
            .and_then(|source| source.kind().as_message())
            .map(ToString::to_string),
        ErrorKind::Message(_) | ErrorKind::Std(_) => None,
    }
}

fn ensure_trailing_newline(mut rendered: String) -> String {
    if !rendered.ends_with('\n') {
        rendered.push('\n');
    }

    rendered
}
