# DR-008 Retain only the active match subtree

Date: 2026-10-05

Status: Accepted

## Decision

Conditional rendering uses `MatchPart` with ordered typed equality patterns and
an optional final wildcard. Only the selected candidate retains physical state.
Switching or losing selection drops the previous subtree. Returning creates
fresh state through normal on-demand initialization and component `Default`.

## Context

Live parts describe an abstract tree; loop bodies have independent physical
state per item. Conditional rendering needs an equally explicit lifecycle.
The todo UI switches between complete and filtered lists using shared slice
settings, while each window owns independent component state.

## Rationale

One active subtree keeps memory proportional to visible structure and makes
unmounting predictable. Existing traversal can visit the chosen branch with
unchanged expression context. Equality and wildcard patterns cover the current
boolean use case without a full pattern interpreter.

## Consequences

Hidden branches have no drawing, layout rows, hit targets, or state payloads.
Branch-local state resets on return. Selection is positional; candidate edits
and item reordering do not provide stable identity. All candidate templates are
validated at registration. Wrong selector types are errors, even with a
wildcard; no matching value intentionally renders nothing.

## Alternatives considered

- Retain state for every candidate: preserves hidden state, but increases memory
  and requires a separate lifecycle for inactive trees. No current feature needs
  it.
- A boolean-only conditional part: smaller initially, but cannot express ordered
  choices without nesting. Typed equality remains simple and supports other
  values.
- Full Rust patterns: bindings, guards, and destructuring need a richer
  reflection and expression model. This complexity is unnecessary for current
  rendering.
- Filter the todo collection: changes data or requires a filtered sequence
  adapter. Conditional rows preserve stored todos and directly exercise the live
  tree.
