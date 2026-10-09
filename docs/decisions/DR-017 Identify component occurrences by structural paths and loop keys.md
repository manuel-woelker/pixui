# DR-017: Identify component occurrences by structural paths and loop keys

- Status: Accepted
- Date: 2026-10-09

## Decision

Identify physical component occurrences with definition-scoped structural paths
of typed child, loop-item/key and match-arm segments. Collection loops use
complete generational arena keys automatically; other loops can supply immutable
integer/string keys or retain positional identity.

Keep focus and hover shared by definition. Reconcile focus against successful
preparation of the latest focus source. Resolve focus eligibility independently
of activation and retain stale-input revision checks.

## Rationale

Application edits should preserve focus, especially for controlled input change
callbacks. Paths prevent unrelated conditional branches or loop lengths from
renumbering later components. Keys let focus and component state follow the same
entity through reordering; arena generations prevent slot reuse from
transferring focus to a replacement entity.

Separating focus from activation supports editable controls without artificial
action callbacks. Source-aware reconciliation allows windows with different
active trees to share logical focus without render order clearing it.

## Context

The implementation stored focus and hover as flattened component indices and
cleared both after actions. Only activatable components could receive focus.
Physical loop state followed sequence positions, and instances could resolve
different conditional structures from presentation settings.

Definitions are immutable after registration. Existing layout indices remain
useful for compact arrays, geometry and action dispatch within one render.

## Consequences

Focus survives ordinary data changes; removal or loss of eligibility clears it
after successful source preparation. Per-instance layouts keep paths and index
lookup tables alongside geometry. Vector/string keys add allocation and lookup
cost; start with ordinary storage and optimize only when measured.

Unkeyed loops intentionally preserve position, not entity identity. Explicit
keys must be unique and immutable. Returning to a match arm creates fresh state
without restoring old focus. Hidden sources defer validation until shown, and
failed renders preserve previous published targets.

Stable identity does not make stale coordinates or captured action bindings
safe. Revision and dirty-geometry validation remain required.

## Considered alternatives

- **Flattened indices with unconditional clearing:** simpler, but loses focus
  after every edit and cannot distinguish a shifted target from its replacement.
- **Paths using loop indices exclusively:** handles unrelated branch-size
  changes but transfers identity when rows move or disappear. Retained only as
  the documented fallback for unkeyed sequences.
- **Mandatory application-assigned IDs for every component:** adds declaration
  overhead to static trees whose structural positions already supply identity.
- **Opaque IDs allocated each render:** cannot preserve identity without another
  reconciliation mechanism, recreating the problem behind a new number.
- **Per-instance focus:** simplifies differing-tree reconciliation but changes
  the established shared-interaction behavior. Retain the ownership chosen in
  [DR-010](<DR-010 Share interaction state across windows of a UI definition.md>).
- **Clear focus when any peer lacks its target:** depends on render order and
  lets an unrelated presentation erase the active source's focus.
