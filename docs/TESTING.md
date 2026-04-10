# What is this document for?

This document describes the testing expectations for `pixui`.
Use it when adding features, fixing bugs, or changing behavior that should stay stable over time.

# Where should tests live?

Tests should be colocated with the code they exercise.

In practice, that means:

- unit tests live in the same Rust source file behind `#[cfg(test)]`
- crate-level integration tests are acceptable when the behavior is easier to verify from the public API
- documentation-only changes do not need Rust tests unless they describe behavior that is also changing

# What testing style should be preferred?

Prefer small black-box tests that verify observable behavior.
If behavior is easier to understand as a rendered string or artifact file, assert on that output directly.

Prefer:

- data-driven tests when several inputs should exercise the same rule
- snapshot-style assertions when the output is long and readability matters
- regression tests for every bug fix that changed behavior

Avoid mocking when a real in-memory or file-backed path is practical.

# When should `PalMock` be used?

Use `pixui_pal::pal_mock::PalMock` when the code depends on platform behavior such as:

- reading or writing files
- process execution
- timestamps or system time
- terminal interactivity

`PalMock` is the default test double for code that depends on the PAL boundary.

# What should be verified for framework and tooling changes?

Framework and tooling changes should usually verify:

- diagnostics and failure messages
- filesystem and process behavior when tooling depends on the PAL
- command-line behavior when new tools are added
- observable UI-model or rendering behavior when that logic lands in the workspace

If a bug fix changed a diagnostic, emitted artifact, or command result, add a focused test for that exact behavior.

# What repository-wide checks should be run?

When completing a unit of work, run:

```bash
nao check
```

That script runs formatting, build, clippy, and the test suite.

# What should happen when a change is not tested?

Call it out explicitly in the final summary or commit context.
If a change is intentionally left without automated coverage, explain why the normal testing approach was not practical.
