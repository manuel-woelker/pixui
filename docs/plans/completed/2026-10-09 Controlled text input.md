# Controlled text input

Status: completed.

## Goal and initial scope

Add a core `TextInputComponent` and standard painter. The component displays its
`content: String` property and proposes a complete new content value through a
change callback. Support typing, caret movement, selection, copy, cut and paste.
Demonstrate it in the showcase and use it for adding todos.

Start with a single-line field. Multiline editing, wrapping, undo/redo, password
masking, rich text, bidi navigation and shaping are separate work. Unicode-safe
editing is required even where the current font cannot display a character.

Prerequisite:
[stable component identity and focus navigation](<2026-10-09 Stable component identity and focus navigation.md>).
The implementation and native verification are complete.

## Controlled contract

- `TextInputProps::content` is the sole source of displayed committed text.
  Application data supplies it through the normal props resolver/expressions.
- Each content-changing edit builds a candidate from the latest resolved content
  and current selection, then calls the change binding with an owned `String`.
  The application action can accept, normalize, reject or ignore it.
- Do not maintain an optimistic editable string that can diverge from props.
  Publish the next frame from freshly resolved application content.
- Invoke change once for an actual content proposal; movement, selection, copy,
  no-op deletion and IME preedit do not invoke it. Equal content is a no-op.
- Apply the proposed caret/selection position when props equal the accepted
  candidate. On rejection/error, retain the previous selection. If application
  normalization returns another value, clamp the proposed position to its valid
  grapheme boundaries. External replacements clamp existing endpoints; they do
  not attempt a speculative text-diff mapping.
- Errors retain authoritative content and remain inspectable through the
  existing worker error path. Existing action errors do not roll back
  application changes; resolve actual props even after a handler reports
  failure.
- Without a change binding the field is read-only: focus, movement, selection
  and copy still work. There is no implicit default action that mutates props.

Use an expression-backed `content` property so application values are easy to
bind. Add a change factory alongside the existing activation factory mechanism,
with a binding conceptually accepting `(&Application, String)` and returning a
`PixuiResult<ActionCall>`. Capture cached action handles and opaque refs; owned
content goes into the action request. Do not introduce a new application lock or
call blocking `ApplicationHandle` methods from the worker.

Illustrative declaration; finalize exact helper names during implementation:

```rust
ComponentPart::typed_with_expressions(input, vec![Expression::entity(text)], resolve_props)
    .with_focus(FocusBehavior::Sequential)
    .with_change(change_action)
```

Register the component and painter through the standard registries. Keep props
minimal initially: `content`. Placeholder, validation messages and configurable
limits can be added when exercised by a concrete example.

## State ownership

| State | Owner |
| --- | --- |
| Authoritative content | Application data, exposed as props |
| Selection anchor/head and accepted-content revision | Definition-side editing state keyed by `ComponentInstanceId` |
| Pending edit reconciliation and composition session | Worker, associated with that editing identity/session |
| Text advances, caret stops, clips and horizontal scroll | UiInstance, because font settings and width can differ |
| Native clipboard and IME facilities | GUI host |

Selection is definition-wide, matching shared focus. All instances display the
same selected range; each computes its own pixels. Remove editing state when its
physical target disappears from the controlling source, and invalidate pending
operations when focus/session identity changes. A hidden source follows the
existing deferred focus-reconciliation policy. Keyed loops retain editing
identity; match removal discards it, and returning starts fresh.

Do not put independently mutable selection into every instance's component
state. Prepare a read-only editing snapshot for the input painter; per-instance
`TextInputState` may cache measurement/presentation data. Retaining previous
resolved content for equality/reconciliation is allowed; it is never an editable
source of truth. Prune definition-side entries after successful preparation,
without letting a peer's different active tree erase the source's state.

## Input routing and ordering

Add narrow component input bindings for editable targets. Route recognized
editing input to the focused input before generic Enter/Space activation, and
pointer press/movement/release to an input selection handler. Retain global Tab,
Shift+Tab and F11 behavior. Do not build bubbling, capture phases or a general
DOM-style event framework for this component.

Ordinary button input retains current revision/dirty-geometry checks. Text edits
need an explicit ordering rule: the worker currently batches commands, so
several keystrokes can arrive against one displayed revision before application
props are rerendered. Using a stale props snapshot loses edits; rejecting every
key after the first content invalidation also loses edits.

Introduce a focus editing session tied to the source instance, stable target ID
and focus epoch. Content updates preserve that session while the target remains
eligible. Focus transfer/removal ends it. Ordered keyboard editing within that
validated session uses current authoritative content rather than old geometry;
it must never silently retarget another input. Pointer selection still requires
compatible presented geometry.

After an edit action, refresh the target's authoritative props before processing
the next edit. Start with a preparation/render barrier after each editing action
if needed; optimize targeted preparation only after correctness is tested. There
is no need to wait for native presentation between characters.

The host receives latest `WindowCommand::SetTextInput` metadata and stamps
`UiCommand::TextInput` with its opaque editing token. Ordered click/Tab/native
focus handshakes allow tokenless input only against that session's originating
presented revision. Programmatic focus has no tokenless origin. No buffering or
blocking reply is needed; subsequent metadata supplies the token. IME retains a
separate native token during keyboard handshakes. Captured selection motion uses
its gesture session while horizontal scroll changes; unrelated content or
presentation invalidation cancels capture.

## Editing behavior

Store anchor/head as UTF-8 byte offsets that are always grapheme boundaries.
Normalize the selected range with min/max; retain direction for Shift movement.
Use a maintained Unicode boundary implementation rather than hand-written scalar
or byte splitting. Select/pin the dependency through the repository toolchain.

| Input | Behavior |
| --- | --- |
| Produced keyboard text | Replace selection, or insert at caret |
| Left/Right | Move one grapheme; collapse an existing selection toward that side |
| Shift + Left/Right | Extend selection one grapheme |
| Home/End | Move to beginning/end; Shift extends |
| Backspace/Delete | Delete selection, otherwise previous/next grapheme |
| Select-all shortcut | Anchor at start, head at end |
| Copy shortcut | Copy selected text; empty selection leaves clipboard unchanged |
| Cut shortcut | Request copy, then propose deletion after successful clipboard write |
| Paste shortcut | Request clipboard text, then propose selection replacement |
| Word movement/deletion shortcut | Use documented Unicode word boundaries |
| Enter | Consume without inserting a newline; submission callback is deferred |
| Tab/Shift+Tab | Existing focus navigation |

Platform shortcut policy uses the primary shortcut modifier: Control on
Linux/Windows and Command on macOS. Document platform-specific word movement and
Home/End equivalents. Never insert shortcut text or synthesize characters from
logical key names; use produced text/IME commits. Ignore key releases and
synthetic text. Allow repeat for movement, deletion and ordinary typing; do not
repeat clipboard commands or insert a composition commit twice.

Pointer press places the caret at the nearest measured grapheme stop.
Shift-click extends from the anchor. Drag selects; retain a worker-side capture
identity until release/cancellation, even outside the input bounds. Use native
pointer capture where needed/supported and cancel on native blur, hide, close or
target removal. Double/triple click and primary-selection clipboard are
deferred. Dragging beyond an edge adjusts the input's horizontal scroll to
extend selection.

Single-line policy: flatten pasted CR/LF/tab runs to one space and discard other
control characters; ordinary Enter/Tab retain their command meanings. Apply the
same normalization to IME commits. Programmatic props containing line breaks or
unsupported controls fail input preparation with an actionable error rather than
painting a different string, preserving the controlled contract. Choose a
bounded insertion/clipboard byte limit and document it; never truncate inside a
grapheme.

## Measurement and painting

Share a text-position helper between input measurement, painting and hit
testing. It must use the same normalization, font configuration, glyph advances
and fallback behavior as existing atlas-based `DrawText`. Map grapheme stops to
logical x positions; combining characters must not introduce editable
intermediate stops. Do not add an independent host text renderer or send one
draw command per glyph.

The standard painter draws field chrome, selection background, one text command
and a caret, using existing rectangle/text/clip commands. Natural width should
have a useful bounded default rather than grow with every keystroke; explicit
layout styles continue to control allocation. Keep text horizontally scrollable
inside its content box and reveal the caret after editing/movement without
changing the definition's vertical scroll unnecessarily.

Use the master paint timestamp and scheduled redraws for caret blink. Only
visible, active editing needs blink redraws; inactive/hidden inputs remain idle.
Reset blink on editing/movement. Shared logical focus does not imply that both
native windows own an active IME or need to blink while the application is
inactive.

Current unshaped text rendering limits visual behavior for complex scripts.
Grapheme-safe storage/editing does not promise shaping, ligatures or bidi
support. Document that limit and keep the text-position helper ready to consume
a future shaped layout without redesigning editing state.

## Clipboard and IME host integration

Clipboard access belongs to the native host, behind a small interface that can
be replaced with an in-memory implementation in tests. Select a maintained
cross-platform backend during implementation. Support plain UTF-8 text only.

Use ordered host-effect requests and worker replies with request IDs. Do not put
copy/read requests into the existing latest-value title/icon mailbox: coalescing
would lose operations and cut acknowledgements. Bound outstanding requests and
propagate clipboard errors without blocking the application worker.

A paste/cut request captures target ID, source/session, content revision and
selection. Validate them on completion; discard a result if focus moved, content
or selection changed, the target vanished, or the instance closed. Paste inserts
at the captured valid selection. Cut deletes only after successful copying and
only while that snapshot remains valid. Copy never changes application content.
Headless hosts report clipboard unavailable unless a test adapter is installed.

The host already forwards IME events. Enable native IME for an editable focused
input in the active native window, update the candidate-window caret rectangle
from that instance's geometry, and disable it when the editing session ends.
Keep temporary preedit separate from `content`; only commits propose changes.
Cancel composition on focus loss/transfer, target removal or external content
replacement. Guard keyboard-text versus IME-commit duplication.

Chosen interpretation of “always show content”: preedit never replaces rendered
props. Native composition UI may show tentative text. Inline preedit remains a
follow-up requiring an explicit exception to the controlled display contract.

## Implementation checklist

- [x] Resolve the open scope/session questions below and record choices here.
- [x] Add documented core input props/state, standard registration and a painter
  with bounded intrinsic size, clipping and authoritative-content validation.
- [x] Add change bindings and narrow input routing; publish safe editing-session
  metadata and define batch/render barriers before handling content edits.
- [x] Implement a pure editing reducer, Unicode boundaries, selection
      reconciliation and shared editing-state lifecycle keyed by stable
      occurrence IDs.
- [x] Add shared text-position measurement and per-instance caret/selection
      geometry, horizontal scrolling and pointer selection capture.
- [x] Add ordered native effect requests/replies and injectable clipboard
      access, including cancellation, bounds and asynchronous cut/paste
      validation.
- [x] Integrate IME commits, composition lifecycle and native caret placement;
  retain generic shortcut/activation behavior for other components.
- [x] Implement selection/caret painting and idle-aware blinking from master
      time.
- [x] Add a controlled showcase field and todo-entry field with change actions;
  demonstrate accepted, normalized and rejected values in the showcase.
- [x] Update architecture, UI/input/text documentation and significant decision
      records; document the controlled contract, ownership and rendering
      limitations.
- [x] Run `./n check` after each completed unit and resolve introduced failures.

## Verification checklist

- [x] Reducer: empty text, selection direction, bounds, grapheme/word movement,
  deletion, insertion, combining marks, emoji sequences and multibyte text.
- [x] Controlled callbacks: full proposed values, no-op suppression, acceptance,
  normalization, rejection/errors and external replacement reconciliation.
- [x] Rapid typing against one presented frame, repeats, click-then-type
      batches, interleaved actions and session changes never lose or retarget
      text.
- [x] Clipboard: copy without a change callback, cut write failure, paste
      failure, non-text/empty/oversized data, normalization, delayed replies and
      cancellation.
- [x] Pointer placement/drag, Shift selection, clipping, horizontal scroll,
      resizing, scale changes and different font fallback widths agree with
      painted text.
- [x] Stable focus through edits, keyed reorder, arm removal and slot reuse;
  shared selection in two windows with independent geometry and native activity.
- [x] IME: preedit causes no change, commits insert once, cancellation and
      external updates are safe, candidate geometry follows the active source
      window.
- [x] Software/GPU display lists render selection and caret consistently; frozen
  timestamps make blink tests deterministic, and idle/hidden inputs do not spin.
- [x] Native showcase/todo checks: type, select, copy/cut/paste, platform
      shortcuts, focus changes, two windows, resize, hide/show and an IME where
      available.

## Recorded choices and verification

1. **Single-line:** implemented. Enter is consumed, with submission and
   multiline behavior deferred.
2. **Composition:** native composition UI only. Preedit is separate from props;
   only commits propose changes.
3. **Shared selection:** stored by structural path within the definition;
   measured caret stops and horizontal scroll stay per instance.
4. **Session handshake:** opaque session metadata plus a revision-scoped initial
   native focus handshake. Programmatic focus requires the published token.
   Worker barriers refresh authoritative props before and after edits. Batched
   queued edits, stale tokens, Tab and programmatic-focus races have tests.
5. **Limits/libraries:** 1 MiB of UTF-8 per committed value/insertion;
   `unicode-segmentation` 1.13.3 and arboard 3.6.1 with Wayland data-control.
   Clipboard I/O runs on an injectable native executor thread. Worker pending
   requests, per-instance effects, executor requests and replies are bounded to
   16. OS clipboard reads can allocate before the byte check.

Automated coverage includes the pure reducer/geometry tests, worker integration
suite, injected clipboard executor, example action/layout regressions and a
software/GPU input fixture. `./n check` passed after implementation units.
The input renderer test also passed with `PIXUI_REQUIRE_GPU=1`, requiring an
actual adapter rather than silently skipping GPU verification.

Implementation and automated verification are complete. The user reported
successful manual validation on 2026-10-09 and requested moving the finished
plans to completed.

API and ownership documentation:
[Text input](<../../../crates/engine/src/ui/Text input.md>). Architecture and
[DR-018](<../../decisions/DR-018 Keep text input controlled with shared selection and editing sessions.md>)
record the design and its costs. The todo draft remains populated after adding;
clearing it and Enter-to-submit can be added with a dedicated submission action.

Submission callbacks, validation styling, placeholders and accessibility support
are follow-ups. Do not imply that this first native input has an accessibility
bridge merely because keyboard editing works.
