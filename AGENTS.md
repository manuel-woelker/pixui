# Repository guidance

## Code style

Follow [the code-style guide](<docs/Code style.md>). Prioritize readability,
good documentation, and simple designs. `lib.rs` and `mod.rs` must contain only
module declarations; put code and other definitions in named modules.

## Verification

Always run `./n check` after completing each unit of work, including fixes and
follow-up changes. Resolve failures caused by your changes before reporting the
work complete. If an existing issue prevents the checks from passing, report
the failing task and the blocker explicitly.

Run development tools through `./t` so they use the pinned toolchain.
