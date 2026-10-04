# pixui

Rust workspace with a core engine library in [`crates/engine`](crates/engine)
and shared infrastructure in [`crates/base`](crates/base).

See the [architecture overview](docs/Architecture.md) for the application runtime
diagram, state ownership, and workspace responsibilities.

[`crates/reflect`](crates/reflect) provides explicit indexed field reads
and method invocation; see its [API and assumptions](crates/reflect/README.md).

`pixui-base` provides `PixuiString` (an owned `HipStr` with serde support),
`PixuiError` and `PixuiResult` (based on error-stack), and the
`pixui_error!`, `pixui_result!`, and `pixui_bail!` macros. These are adapted
from Joi's `joi-base` and `joi-error` libraries.

The base crate also provides a typed `Arena<T>` and an eight-byte `Key<T>`
containing a slot index, arena identity, and generation. See its
[arena documentation](crates/base/README.md) for usage and limits.

## Development

Follow the [code-style guide](<docs/Code style.md>) for readable code, useful
documentation, simple designs, and module layout conventions.

Run commands from the repository root:

Always run `./n check` after completing each unit of work. It runs Rust
formatting, compilation checks, clippy, nextest, and documentation tests.

```bash
./n --list                            # List available tasks
./n check                             # Formatting, compilation, clippy, and tests
./n ci                                # Run the same checks for CI
./t cargo build --workspace            # Build all crates
./t cargo-nextest nextest run -p pixui-engine # Test the engine
./t cargo fmt --all                    # Format Rust code
```

Use `./t` for tool invocations so builds use the pinned toolchain.
`./n` runs tasks defined in `.nao/nao.kdl` through the same wrapper.

Enable the repository pre-commit hook once per clone:

```bash
git config core.hooksPath .githooks
```

The hook regenerates every staged added or modified `.drawio.svg` from its
embedded diagram using `./t drawio`, then stages the regenerated SVG. It requires
Bash and a working desktop display for draw.io. Unstaged edits are preserved;
for partially staged diagrams, only the staged version is regenerated. Deleted
diagrams are skipped. Export errors block the commit without changing staged
files. To run regeneration manually, use `./scripts/regenerate-drawio-svg.sh`.

Tool versions are pinned in `.tool-tool/tool-tool.v2.kdl`, with download
checksums in `.tool-tool/v2/checksums.kdl`. The `./t` bootstrap also verifies
the tool-tool binary checksum. Tools download into `.cache/` on first use.
The tooling currently supports Linux x86_64 and requires Bash, curl,
sha256sum, and archive extraction tools. No Node or JavaScript tooling is used.

Tests run through nextest and fail if no tests are discovered. Documentation
tests run separately through Cargo because nextest does not run them.

## Decision records

Significant technical and product decisions are recorded in
[`docs/decisions`](docs/decisions). See [DR-000](<docs/decisions/DR-000 Record decisions in the repository.md>)
for the required format and conventions.
