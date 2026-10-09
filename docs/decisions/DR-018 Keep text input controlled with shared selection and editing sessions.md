# DR-018: Keep text input controlled with shared selection and editing sessions

- Status: Accepted
- Date: 2026-10-09

## Decision

Committed input text comes exclusively from component props. Editing proposes a
complete owned string through an ordinary action binding. Resolve authoritative
props before and after editing commands. Keep grapheme-based selection shared by
definition occurrence; retain measurement and horizontal scroll per instance.

Use an opaque focus session for ordered editing and a separate bounded ordered
queue for native clipboard effects. Native IME commits propose changes; preedit
never replaces the displayed controlled value.

## Rationale

Applications must be able to accept, normalize or reject changes without an
independent editable buffer diverging from their data. Shared selection matches
shared focus; instance geometry accounts for differing widths and font scales.

Several characters can arrive before a native window presents a new frame.
Session validation and worker render barriers preserve their order without
losing edits or weakening ordinary activation geometry checks. Clipboard replies
need snapshot validation because native operations finish asynchronously.

## Context

Application data and live-tree preparation belong to one worker. Windows consume
latest complete display lists and send bounded queued input. The existing
revision checks correctly reject stale coordinates but alone cannot support
rapid controlled editing. Stable occurrence paths and keyed loops already
preserve focus through application edits.

## Consequences

Actions see full proposed values. Rejection retains authoritative text and
selection; normalization clamps proposed endpoints. Erroring actions still have
no rollback, so preparation resolves their actual mutations.

Each edit can perform a full preparation/paint pass, increasing worker cost.
Latest-output coalescing avoids requiring native presentation between edits.
Optimize targeted preparation when measurements justify it.

Clipboard operations run on a native I/O thread; bounded queues and session,
content and selection revisions prevent obsolete cut/paste from changing data.
IME/capture capability and clipboard availability remain platform dependent.
Single-line, grapheme-safe editing does not provide shaping, bidi navigation,
accessibility, multiline editing or undo/redo.

## Considered alternatives

- **Optimistic internal text:** gives immediate speculative feedback but can
  violate the controlled contract after rejection or normalization.
- **Selection per window:** simplifies local input handling but disagrees with
  shared logical focus and creates different editing selections for one field.
- **Require presentation after every character:** reuses strict frame checks,
  but drops rapid native input or makes typing depend on presentation latency.
- **Ignore revision checks for all keyboard/pointer events:** permits stale
  events to target a different component. Only an explicitly validated editing
  session grants ordering continuity to its stable target.
- **Coalesce clipboard effects with native metadata:** loses copy/cut/read
  operations and acknowledgements. Persistent metadata can safely coalesce;
  operations require ordered delivery.
- **Run clipboard I/O on the application or GUI thread:** simpler, but native
  clipboard stalls would block dispatch or event processing.
- **Render inline preedit immediately:** customary in richer editors, but needs
  a documented exception to displaying only `content`. Native composition UI
  remains the initial presentation mechanism.
