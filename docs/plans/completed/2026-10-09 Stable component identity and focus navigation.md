# Stable component identity and focus navigation

Status: completed.

## Goal

Preserve focus through application updates and make keyboard navigation work
for focusable components independently of activation. This is groundwork for a
controlled text input: typing must not lose focus on every change callback.
Text editing and clipboard integration belong in a subsequent plan.

## Starting behavior

- `UiDefinitionState` stores focus and hover as flattened component indices.
  All instances of a definition share them; geometry remains per instance.
- `UiRegistry::invalidate_all` clears both on every application action.
- Only components with an activation binding enter `FocusTarget`. Tab cycles
  those targets in physical preorder; Enter/Space activate the selected target.
- `walk.rs` retains loop state by position. Match changes discard the previous
  arm's state. Inserting/removing earlier content shifts flattened indices.
- Render revisions protect against input targeting stale geometry. Stable IDs
  must preserve this protection, rather than bypass it.

## Identity model

Use an owned, opaque `ComponentPath` backed by a `Vec<PathSegment>`. A scoped
`ComponentInstanceId` combines `UiDefinitionId` and this path. This is distinct
from the existing typed `ComponentId<C>`, which identifies a registered
component type, not an occurrence in the live tree.

Start with explicit segments:

| Segment | Meaning |
| --- | --- |
| `Child(usize)` | Template child index in a Composite or Container. |
| `LoopItem(usize)` | Sequence position in an unkeyed loop. |
| `LoopKey(ItemKey)` | Item identity in a keyed loop. |
| `MatchArm(usize)` | Candidate index in a Match. |

The root has an empty path. Descent adds one segment; components inherit the
current path. Distinct segment variants make paths unambiguous. Nested loops
and matches naturally compose. A leaf after a loop does not change identity
when that loop gains rows. Inactive match arms do not renumber other children.

Paths are scoped to an immutable registered definition. Replacing a definition
must allocate a new definition ID and discard its interaction state. Template
child and match-arm indices are stable because application updates change data,
not the registered template. Legacy mutable walker visitors do not gain a
guarantee for arbitrary template edits.

Retain flattened indices for ordered arrays, layout and drawing. Publish a
parallel component-ID array and an ID-to-index lookup per successful layout.
Never use a flattened index as persistent interaction identity. Start with
ordinary vectors and maps; path interning is unnecessary without measurements.

### Loop identity and reconciliation

Index paths preserve **positional** identity only. Deleting or reordering items
can assign the same path to different data. Do not describe unkeyed loops as
preserving entity identity. They remain suitable for fixed-position sequences.

Add an optional loop key resolver, evaluated in each item's expression context.
Use a small owned `ItemKey` supporting integer and string keys initially.
Keys are unique within one loop occurrence; duplicate keys fail preparation
before publishing a new frame. Nested occurrences have separate namespaces.
Applications must provide immutable keys and must not reuse a removed entity's
key for an unrelated entity. Collection adapters should eventually expose full
arena identity, including generation, rather than just a slot index; do not
silently derive identity from reflected values or current iteration order.

For keyed loops, reconcile `ForLoopState` entries by key as well as assigning
keyed paths. Reorder retained state into sequence order, initialize new keys
with Unknown, and drop removed keys. Focus and component-owned state must follow
the same item. Keep existing unkeyed positional reconciliation unchanged.

Match-arm identity does not imply retention of inactive state. Switching away
drops the arm state and clears focus if its target disappears. Returning later
does not automatically restore focus to that arm.

## Focusability and navigation

Introduce component-part focus behavior independent of activation:

- Default: activatable components participate in sequential focus navigation,
  preserving button/checkbox behavior.
- Explicit focusable: participates without needing an activation callback.
- Pointer/programmatic only: can receive focus but is skipped by Tab.
- Not focusable: excluded from all focus targets, even if activatable.

Resolve eligibility during preparation so definitions can disable a control
from application data. Keep activation optional on focus targets. Distinguish
pointer activation regions from focus regions: clicking a nonfocusable action
may still activate it; clicking an input must focus it without activating.

Navigation rules:

- Tab/Shift+Tab use current source-instance physical preorder, not paint order,
  Grid placement or coordinates. Wrap at either end as today.
- With no eligible current focus, Tab chooses the first target and Shift+Tab
  the last. Empty lists leave focus unset.
- Offscreen but present components remain in the sequence. Scroll the chosen
  target into view using source-instance geometry and the shared scroll request;
  other instances clamp that request independently. Fully clipped descendants
  of a non-scrolling container must not become unreachable tab stops. Specify
  this eligibility separately from ordinary viewport clipping.
- Pointer press sets focus on the topmost eligible focus region; release retains
  existing button activation policy. Clicking empty space clears focus.
- Native window blur does not discard logical focus. The painter's focused flag
  remains shared across instances; native active-window state is separate.
- Enter/Space activate only when a target has a binding. Future text-input event
  handling can consume Space/Enter before generic activation. Arrow keys remain
  available to components; spatial navigation is out of scope.
- Provide a worker-side checked focus request using a scoped ID, including an
  explicit clear operation. Reject foreign-definition or absent/ineligible IDs.

## Shared state and render lifecycle

Replace stored focus/hover indices with IDs. Preserve them when actions mark
instances dirty. Resolve IDs against the freshly prepared tree before painting;
painters still receive simple focused/hovered booleans.

Clear focus when the target is removed, loses eligibility, changes component
type at the same path, or belongs to a discarded match arm. Do not automatically
focus a neighbor after removal; the next Tab starts at the appropriate end.
Hover must be recomputed from the source pointer and new geometry, rather than
retained solely because an ID survives.

Successful frame publication installs identity, eligibility, geometry and
bindings together. Preparation/layout failures do not publish partial targets
or clear focus from an incomplete tree. Continue rejecting discrete input while
geometry is stale and reject incompatible presented revisions. IDs cannot make
old pointer coordinates or captured activation bindings safe.

Focus stays definition-wide. If instances resolve different conditional trees,
the latest input source determines focus eligibility. Another instance may
render no focus highlight when the shared ID is absent there; it must not erase
focus merely because its own tree lacks the target. Track the focus source and
reconcile removal against its successful preparation. If that source closes,
use a remaining instance deterministically, or clear focus when none remain.
Hidden sources retain logical focus until they can prepare again; input from a
visible instance supersedes them. Do not render hidden windows just to validate
focus. Document this policy and test render-order independence.

## Implementation checklist

- [x] Add documented path segments, item keys and scoped occurrence IDs in named
  modules; distinguish them clearly from component registry IDs.
- [x] Extend walker traversal context with occurrence paths, including stack and
  recursive loop descent. Preserve existing visitor order and error behavior.
- [x] Add optional loop key resolution and keyed physical-state reconciliation.
  Document positional semantics for unkeyed loops and reject duplicate keys.
- [x] Carry occurrence IDs through preparation/layout. Add lookup tables and
  include IDs/eligibility in published-layout compatibility checks.
- [x] Introduce explicit focus behavior; separate focus targets/regions from
  activation bindings and regions. Maintain default button/checkbox behavior.
- [x] Migrate shared focus, hover, painted-hover tracking and painter resolution
  to IDs; replace unconditional focus clearing with successful reconciliation.
- [x] Implement source-aware reconciliation, pointer focus, sequential
      navigation, checked programmatic focus and scroll-to-focus behavior.
- [x] Update examples/tests that currently rely on actions clearing focus.
- [x] Update UI/layout documentation and Architecture.md for identity ownership,
      preparation/publication and shared focus; record significant decisions if
      needed.
- [x] Run `./n check` after each completed unit and resolve introduced failures.

## Verification

- [x] Path tests: nested containers, composites, loops and matches have distinct
  IDs; growing an earlier branch leaves later IDs unchanged; definition scoping
  prevents cross-definition focus requests.
- [x] Keyed loops: insertion, deletion, reorder, nested keys, duplicate
      rejection, removed/recreated entities and retained component state.
      Unkeyed tests explicitly demonstrate positional semantics.
- [x] Focus survives ordinary actions, prop updates, resize, translation changes
  and visual redraws. Removing/disabling a target or switching arms clears it
  without focusing a different component at the previous flat index.
- [x] Nonactivatable focus targets, pointer-only targets, focus opt-out, forward
  and backward wrap, empty lists, pointer press/release and click outside.
- [x] Offscreen focus scrolls into view; permanently clipped targets are
      skipped.
- [x] Two windows share focus despite different geometry. Different active
      trees, failed preparation, hidden sources and source closure obey the
      documented policy independently of render order.
- [x] Stale revisions and dirty geometry still reject discrete input; compatible
  redraws preserve safe bindings without confusing moved identities.
- [x] Native showcase verification: Tab, Shift+Tab, click, action-triggered
      updates, conditional pages, scrolling and two-window focus. Add a
      nonactivating focusable showcase target to exercise the input groundwork
      without implementing editing.

## Implementation choices

- Retained shared logical focus and wrapping Tab navigation, using the proposed
  source-aware reconciliation policy.
- Unkeyed sequences explicitly retain positional identity. Keyed loops reconcile
  state and paths together; duplicate keys fail before moving loop payloads.
- Collection expressions automatically use complete packed arena keys, including
  arena ID and generation. Explicit resolvers override this default.
- Published layouts store unscoped paths for direct index lookup; shared state
  and focus requests use definition-scoped occurrence IDs.
- Programmatic requests use `UiCommand::Focus` through the existing dispatch
  channel. They require a successfully prepared eligible target and current
  geometry; no new mutable application-state access API is needed.
- Default button release also selects its focus target, preserving existing
  programmatic release-only click behavior. Pointer press now focuses
  immediately.
- A bounded second rendering sweep updates peers dirtied by source
  reconciliation. Hidden instances remain skipped; failed preparation is not
  retried by that sweep.
- No new text-input plan or editing implementation was started in this unit.

Out of scope: text editing, clipboard/IME, undo, accessibility integration,
spatial navigation, nested focus scopes, custom tab order and arbitrary live
template mutation. These should build on this identity/navigation contract.

## Manual verification

The user reported successful manual validation on 2026-10-09 and requested
that the finished plans move to completed.
