# DR-014: Register declared translations by definition domain

- Status: Accepted
- Date: 2026-10-07

## Decision

Declare source messages and named argument expressions in UI definitions.
Assign application-wide indices at registration, with zero reserved for
unresolved messages. Evaluate compiled templates through dense per-language
vectors selected by each presentation's language index.

Give each definition one domain, defaulting to its name. Identify catalog
messages by domain, source text, and optional semantic context. Export those
stable keys rather than runtime indices. Separate catalog file adapters from
runtime evaluation, initially using singular UTF-8 PO/POT.

## Rationale

Explicit declarations make complete extraction possible, including hidden
branches and empty collection loops. Indices avoid repeated source-string and
domain lookup during painting. Definition domains isolate application areas
while allowing deliberate reuse across definitions.

Source keys avoid mandatory identifier naming and work with translator tools.
Context separates different meanings of the same text. A neutral catalog
boundary permits alternative interchange formats without changing expressions,
draw lists, or rendering clients.

## Context

The examples previously translated strings inside callbacks by branching on
locale. Those callbacks are opaque to the live tree walker, cannot be extracted,
and duplicate translation logic. The worker owns registration and evaluation;
each UI instance already has independent presentation settings.

The first implementation needs interpolation and fallback. Full plural rules,
grammatical selection, locale formatting, and complex text shaping remain
separate requirements.

## Consequences

- Expression-backed props and native-property callbacks receive evaluated
  values. Existing callbacks remain supported but cannot hide extractable
  declarations.
- Runtime indices depend on registration order and application identity.
  Catalogs use stable keys and are mapped into vectors at installation.
- Source edits require catalog updates. Translator comments do not distinguish
  keys; use context for that purpose.
- Domains are definition-wide. Shared components inherit the enclosing domain;
  per-expression and subtree overrides are deferred.
- PO/POT is the first adapter, with validation of our brace placeholders. Empty,
  fuzzy, and obsolete translations fall back; plural entries are rejected.
- Catalog replacement is atomic and triggers worker rendering/metadata updates.
  The initial implementation invalidates all UIs, consistent with other content
  changes. Affected-instance-only invalidation can follow if measurements
  justify it.
- Rich Fluent or gettext plural semantics cannot be represented losslessly by
  the current literal/argument template model.

## Considered alternatives

### Continue locale branches in callbacks

Rejected because extraction cannot discover their messages, and translators
would have to edit Rust source.

### Persist numeric translation indices in files

Rejected because definition registration order can change. A catalog could then
silently translate the wrong message.

### Require a developer identifier for every message

Deferred because source/context keys support ordinary reuse with less
declaration overhead. Reconsider stable explicit IDs if source edits become
costly.

### Use a single undifferentiated application catalog

Rejected because identical source strings in separate application areas may need
independent translations and translator workflows.

### Allow domains on every expression or subtree

Deferred to keep declaration and extraction rules simple. Definition-wide
domains meet the current application-area separation requirement.

### Adopt Fluent's full runtime immediately

Deferred because its language-sensitive selectors and terms require semantics
beyond the requested interpolation model. The interchange boundary stays
pluggable without promising those semantics.

### Build a runtime plugin registry

Rejected because an explicit import/export adapter already provides the required
format substitution without adding global registration or render-path dispatch.
