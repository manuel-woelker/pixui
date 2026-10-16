# Image props in the UI language

Status: implemented; native showcase verification pending.

## Goal

Allow the core `ImageComponent` to be declared in `.pixui`, using a validated
resource path supplied by a literal, an untranslated template or application
data. Convert the showcase Images pane without adding a special image-only
component builder or changing its Rust props type.

```text
<Image path='images/pixui-logo.png' layout=layouts.logo />
<Image path=entities.selected_image layout=layouts.logo />
<Image path='images/{item.filename}.png' layout=layouts.logo />
```

Rust continues to register the image component, export application data/layouts
and configure its image loader. Loading, caching and hot reload retain their
existing resource-system behavior.

## Starting point

`ImageProps` contains `path: ResourcePath`. `ResourcePath::new` validates
portable relative filenames and keeps the underlying string private. Neither
type previously implemented the reflection needed for language props
construction.

The language constructs props in descriptor field order and requires exact
types, except contextual numeric literals. String expressions produced
`String`; there was no conversion to `ResourcePath`. Double-quoted templates
translate by default, making their use for resource filenames inappropriate.

## Design

### Reflect the validated value and image props

- Implement `Reflect` for `ResourcePath` with a cached `TypeDescriptor`. Expose
  its type identity as an opaque value: no writable/string field, reflected
  constructor or method is needed. Valid values originate from
  `ResourcePath::new`.
- Add reflected fields and a sendable constructor for `ImageProps`, following
  the standard component props infrastructure. The `path` field remains a
  `ResourcePath`; unvalidated strings cannot enter through that constructor.
- Register the standard `ResourcePath` reflection and ownership adapter in
  `Environment::new`, alongside its existing built-in value adapters. Other
  custom components and action requests can then use the same value type.
- Keep `ImageState` and image pixel resources unreflected. Registering a
  component or compiling its props must not decode images or access a
  filesystem.

### Contextual conversion of strings to resource paths

When an expression is expected to produce `ResourcePath`:

1. Accept an expression that already has that exact type without conversion.
2. Otherwise check it as a `String` expression and insert an explicit checked
   conversion node. Reject other types with a source-located diagnostic.
3. Validate static single-quoted literals during checking, producing a typed
   `ResourcePath` constant. Do not require the referenced file to exist.
4. Evaluate dynamic strings/templates on the worker and call
   `ResourcePath::new`. Preserve the expression's origin through conversion so
   validation failures identify the filename and expression span.

Use the existing `ResourcePath` validator for both stages. Do not duplicate its
rules, normalize invalid input, strip prefixes or silently fall back to another
filename. Existing errors for missing resources and malformed images continue to
come from normal component preparation.

This is a narrow conversion for the semantic `ResourcePath` type wherever that
expected type is known, including custom props or action request fields. It is
not a general implicit-coercion framework. There is no reverse conversion to
`String`, truthiness conversion, or arbitrary reflected constructor invocation.

An exported `String` entity is accepted through this conversion; an exported
`ResourcePath` entity is accepted directly. Collection-field paths work through
the same expression and reflection rules. Callback expressions retain their
existing capture and deferred-evaluation semantics.

### Translation rules

Resource filenames are structural strings. Require untranslated source templates
for string-to-path conversion. Reject double-quoted translated templates,
including translated subexpressions nested inside an untranslated template, with
an explanation to use single quotes. Do not register resource path templates as
translatable messages.

Application `String` values remain permitted; the compiler cannot determine
whether application code localized their contents. Document that those values
must contain resource filenames. Locale-dependent image selection can be done
explicitly in application data in the future; it is not inferred from i18n text.

### Runtime and failure behavior

Add a small typed conversion representation to checking/lowering and the shared
expression program evaluator. Its child is the original checked string
expression. Existing callback capture traversal and source-origin handling must
continue to visit that child normally.

A dynamic path validation error or resource load/decode error prevents
publication of the new frame. Preserve the last successful render and the
existing `ImageState` behavior. Fixing the underlying value allows the next
preparation to recover. Successful state updates in earlier components are not
rolled back.

A changed valid path uses the existing loader/cache; unchanged paths reuse its
immutable image snapshots. Hot reload swaps resource data through the existing
resource pipeline and does not require recompiling the `.pixui` definition.

## Showcase migration

- Add `examples/showcase/ui/images.pixui` using the core `Image` component and
  `'images/pixui-logo.png'`.
- Export the image component and a fixed 160 × 160 logo layout in the host
  environment. Keep layout sizing separate from the resource path.
- Replace the Rust-built Images pane with the compiled template, inheriting the
  showcase translation domain. Keep its translated heading and existing image
  placement/appearance.
- Reuse the existing application image loader and hot-reload configuration.
  The engine change supports dynamic paths even if the showcase pane only needs
  a literal; exercise dynamic updates in tests rather than adding unrelated UI.

## Implementation checklist

- [x] Add and document opaque reflection for `ResourcePath` and reflected
  sendable construction for `ImageProps`.
- [x] Register the resource-path value adapter in the language environment.
- [x] Add contextual string conversion, compile-time literal validation and
  diagnostics for translated/non-string expressions.
- [x] Lower and evaluate dynamic conversions with source origins and normal
  callback/i18n traversal.
- [x] Add reflection, compiler and worker integration tests described below.
- [x] Convert the showcase Images pane into its own `.pixui` template.
- [x] Update the language reference, resource/component documentation and
  architecture overview to explain the conversion and its limits.
- [x] Run `./n check` after each unit of work and resolve introduced failures.
- [ ] Manually verify the Images pane in both showcase windows.
- [ ] Mark this plan complete and move it to `completed` after verification.

## Verification

- Reflection exposes the correct image props field/type and constructs props
  from an owned `ResourcePath`. Wrong field types/counts fail. The opaque path
  descriptor cannot construct an unvalidated path from a raw string.
- Compilation accepts valid literals, escaped literal text, untranslated
  interpolation, exported string values and already-typed paths. Invalid static
  filenames report checking diagnostics with correct spans.
- Reject empty, absolute, parent/dot/empty-component, backslash, colon and NUL
  paths according to the existing validator. Include Unicode filenames and
  case-preservation tests. Do not introduce stricter platform-specific rules.
- Reject booleans/numbers and translated templates, including nested translated
  interpolation. Resource templates must not appear in exported PO/POT catalogs.
- Headless worker tests use an in-memory/layered image source: verify image
  drawing, dynamic path changes, reuse of unchanged image identities, missing
  and invalid paths, last-good-frame retention and recovery.
- Test a custom reflected prop and an action request with `ResourcePath` to show
  that conversion is type-directed rather than hard-coded to `Image.path`.
  Exercise a callback payload string so conversion does not evaluate before the
  payload exists.
- Retain the existing image loading/hot-reload tests and add a compiled-template
  case demonstrating that a resource replacement updates the drawn image.
- Verify the showcase still renders the logo at 160 × 160 in both presentations
  and exports its translated heading without exporting the filename.

## Assumptions and open questions

- **Proposed syntax:** use existing prop expressions with contextual conversion,
  rather than adding `resource(...)` or another constructor syntax. This keeps
  ordinary image declarations short and avoids exposing arbitrary calls.
- **Proposed translation policy:** reject translated path templates, rather than
  silently treating double quotes differently for image props. String syntax
  should retain one meaning throughout the language.
- **Validation boundary:** static syntax can be checked during compilation;
  existence, decoding and dynamic path validity remain runtime concerns because
  filesystems can be layered, replaced or unavailable at compilation time.
- **Scope:** implement this narrow validated value conversion first. Reconsider
  a pluggable conversion registry when another concrete semantic value type
  needs one. Container layout, dynamic styles and native metadata syntax remain
  separate showcase migration work.

## Implementation notes

- Plan committed before implementation as `0f7fb51`.
- Added an opaque cached path descriptor and standard image props construction.
  Conversion is type-directed, including custom props and callback request
  fields.
- Worker tests cover layered lookup, snapshot reuse, invalid/missing/undecodable
  resources, retained output and recovery. Existing watcher tests now compile
  the image declaration. Compiler tests cover loop fields and catalog exclusion.
- The showcase Images pane now comes from `ui/images.pixui`; host code retains
  loader configuration and the fixed 160 × 160 layout.
- Automated checks have passed for the reflection/compiler and worker/callback
  units. Final checks also passed with documentation and catalog/loop coverage.
- Native verification remains required before moving this plan to completed.
