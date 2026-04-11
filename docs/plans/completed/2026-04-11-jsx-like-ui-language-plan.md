# What problem does this plan solve?

The engine needs a small parser pipeline for a UI description language that feels like JSX, but the repository does not yet have lexer or parser infrastructure for UI markup.
This plan covers the first slice only: tags and string properties.

# What syntax is in scope for the first implementation?

The initial language should support:

- opening tags such as `<Button>`
- self-closing tags such as `<Button />`
- closing tags such as `</Button>`
- string properties such as `<Button label="Click me" />`
- nested tags such as `<Stack><Button label="Save" /></Stack>`

The initial language should not support:

- expressions inside braces
- spread props
- non-string property values
- text nodes between tags
- comments
- fragments
- namespaces or member-expression tag names

# Where should the implementation live?

The parser should live in `crates/engine`, since this language is part of the engine-facing UI model rather than a general-purpose base utility.

Prefer a small module tree under a dedicated namespace, for example:

- `crates/engine/src/ui_description.rs`
- `crates/engine/src/ui_description/lexer.rs`
- `crates/engine/src/ui_description/parser.rs`
- `crates/engine/src/ui_description/ast.rs`

Each struct, enum, and trait should still follow the repository rule of one item per file where practical.

# What should the lexer produce?

The lexer should turn source text into a compact token stream that preserves enough information for parsing and useful diagnostics.

The first token set should cover:

- `<`
- `>`
- `</`
- `/>`
- `=`
- identifiers for tag names and property names
- string literals with their decoded value and source span

The lexer should reject malformed strings and unexpected characters with structured `PixuiError` values instead of panicking.

# What should the parser produce?

The parser should build a small AST for element nodes with:

- tag name
- ordered string properties
- ordered child elements

The parser should validate:

- opening and closing tags match
- property syntax is `name="value"`
- duplicate closing or premature EOF cases surface useful diagnostics

The parser does not need to perform semantic validation beyond syntactic correctness in this phase.

# What implementation order should be used?

1. Define the AST types for elements and string properties.
2. Define token types and implement the lexer with source spans.
3. Add lexer tests for valid tokens and malformed input.
4. Implement the parser over the token stream.
5. Add parser tests for self-closing tags, nesting, properties, and syntax errors.
6. Expose a small parse entry point from the engine crate.
7. Run repository-wide verification.

# How will progress be tracked?

- [x] Add a `ui_description` module to `crates/engine` and expose it from `lib.rs`.
- [x] Add AST types for UI elements and string properties.
- [x] Add lexer token types and span-aware lexing helpers.
- [x] Implement lexing for tag punctuation, identifiers, and quoted string values.
- [x] Return structured errors for malformed strings and unexpected characters.
- [x] Add colocated lexer tests covering simple tags, self-closing tags, nested tags, properties, and invalid input.
- [x] Add parser types or parsing helpers needed to build the AST.
- [x] Implement parsing for elements with nested children and string properties.
- [x] Validate matching close tags and malformed property syntax.
- [x] Add colocated parser tests covering successful parsing and representative syntax failures.
- [x] Add a small top-level parse API that callers can use without knowing lexer internals.
- [x] Run `nao check`.

# How should the work be verified?

Verification should rely on colocated unit tests in the lexer and parser modules plus a final repository check.

Completed verification:

- `cargo test -p pixui-engine ui_description`
- `cargo clippy -p pixui-engine --all-targets --all-features -- -D warnings`
- `nao check`

Important parser test cases:

- a single self-closing component with one string property
- a parent element with multiple child elements
- an element with multiple string properties
- mismatched closing tags
- missing closing tag at EOF
- malformed string literal
- invalid property syntax such as missing `=`

# What assumptions and risks should stay explicit?

- This plan assumes the first consumer only needs an in-memory AST, not direct lowering into component or entity structures.
- This implementation accepts raw `&str` input, assumes UTF-8 source text, and does not attempt JSX-level compatibility beyond the syntax listed above.
- The biggest scope risk is overengineering the grammar too early. Keep the AST and parser narrow so future expression support can be added without dragging in half a compiler up front.
- Error reporting currently uses message-based `PixuiError` values with byte offsets and spans. A later source-diagnostic pass may still be worthwhile if editor integration becomes important.

# What open questions remain after this plan?

- Should text nodes be added immediately after tags and string props, or deferred until there is a concrete renderer need?
- Should property ordering be preserved exactly in the AST, or normalized into a map later during semantic analysis?
- Should the parse API accept raw `&str`, a `SourceFile` abstraction from `pixui-base`, or both?
