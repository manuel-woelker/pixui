# pixui

Rust workspace with a core engine library in [`crates/engine`](crates/engine)
and shared infrastructure in [`crates/base`](crates/base).

[`crates/reflection`](crates/reflection) provides explicit indexed field reads
and method invocation; see its [API and assumptions](crates/reflection/README.md).

`pixui-base` provides `PixuiString` (an owned `HipStr` with serde support),
`PixuiError` and `PixuiResult` (based on error-stack), and the
`pixui_error!`, `pixui_result!`, and `pixui_bail!` macros. These are adapted
from Joi's `joi-base` and `joi-error` libraries.

## Development

Run commands from the repository root:

```bash
./n --list                            # List available tasks
./n check                             # Formatting, clippy, tests, and doc tests
./n ci                                # Run the same checks for CI
./t cargo build --workspace            # Build all crates
./t cargo-nextest nextest run -p pixui-engine # Test the engine
./t cargo fmt --all                    # Format Rust code
```

Use `./t` for tool invocations so builds use the pinned toolchain.
`./n` runs tasks defined in `.nao/nao.kdl` through the same wrapper.

Tool versions are pinned in `.tool-tool/tool-tool.v2.kdl`, with download
checksums in `.tool-tool/v2/checksums.kdl`. The `./t` bootstrap also verifies
the tool-tool binary checksum. Tools download into `.cache/` on first use.
The tooling currently supports Linux x86_64 and requires Bash, curl,
sha256sum, and archive extraction tools. No Node or JavaScript tooling is used.

Tests run through nextest and fail if no tests are discovered. Documentation
tests run separately through Cargo because nextest does not run them.
