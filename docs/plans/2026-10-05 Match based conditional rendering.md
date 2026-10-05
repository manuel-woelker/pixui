# Match based conditional rendering plan

Status: proposed. This plan adds conditional live parts and uses them to hide
completed todos through a boolean owned by the todo slice.

## Goal

Add `MatchPart`: evaluate an expression, choose the first matching candidate,
and walk/render that candidate's part. Preserve normal expression context,
component state, actions, and worker-side rendering contracts.

Demonstrate two list presentations in the todo example: show all todos, or hide
completed todos. A slice-owned `hide_completed` flag controls both windows.
Hiding affects presentation; it never removes items from the collection.

## Proposed API

Illustrative signatures; finalize module locations during implementation:

```rust,ignore
struct MatchPart {
    expression: Expression,
    candidates: Vec<MatchCandidate>,
}

struct MatchCandidate {
    pattern: MatchPattern,
    part: LivePart,
}

enum MatchPattern {
    Value(/* private erased comparable value */),
    Wildcard,
}

impl MatchPattern {
    fn value<T: Reflect + PartialEq + Send + Sync>(expected: T) -> Self;
}

impl MatchPart {
    fn new(expression: Expression, candidates: Vec<MatchCandidate>)
        -> PixuiResult<Self>;
}
```

Add `LivePart::Match(MatchPart)`. Use a named matching module for pattern
implementation; keep `lib.rs`/`mod.rs` limited to module declarations.

Typed value patterns use `DynamicObject::downcast_ref::<T>` and T's `PartialEq`.
Store the erased immutable comparison behind an Arc so template cloning does
not require `T: Clone`. Keep the erasure private; do not add dynamic equality to
the reflection descriptors or a public matcher plugin interface.

This supports booleans and other reflected comparable values with a small API.
It mirrors ordered value arms and `_`, without implementing Rust's complete
pattern language. Enum destructuring, bindings, guards, ranges, OR patterns,
numeric coercions, and exhaustiveness analysis are outside the initial scope.
Values use exact concrete types; `i32` and `i64` are different. PartialEq
semantics apply, including NaN never matching a value pattern.

## Matching contract

- Evaluate the selector once per reached match during an ordinary walk, after
  the node visitor and its existing post-visit template reconciliation.
- Check that the selected expression value has the literal patterns' concrete
  type. Wrong types produce a descriptive error, including when a wildcard is
  present; they do not silently select an unrelated branch.
- Test candidates in declaration order; walk only the first match.
- An optional wildcard is last and catches values unmatched by preceding arms.
- No matching arm means no child and no drawing, hit region, or layout row.
  Non-exhaustive rendering is intentional. An empty composite can also express
  an explicit empty branch; do not introduce a separate empty part variant.
- Selection does not replace the current context with the selector value.
  Matching `TodoItem.completed` must leave TodoItem available to its row's props
  resolver and activation binding. Application access also remains available.
- Constructors reject mixed literal types, multiple wildcards, and candidates
  after a wildcard. Revalidate these structural invariants during walking to
  handle visitor edits to public templates. Duplicate literals may remain;
  first wins, without an unreachable-pattern warning system.
- Expression/comparison/visitor failures stop the walk. Published output keeps
  its last good revision and geometry; prior state updates are not rolled back.

## Physical state and lifecycle

Add `PartState::Match(MatchState)` with a selected candidate index and one boxed
child `PartState`. Start with no selection and `PartState::Unknown`.

Retain child state while the same arm remains selected. Switching arms drops
the old subtree and starts the new child as Unknown. Losing a match drops its
child and clears selection; returning later initializes fresh state. This is
an unmount/remount policy, avoiding retained hidden component trees.

Candidate identity follows position, consistent with existing composites and
loops. Candidate insertion/reordering or same-kind template replacement does
not guarantee state identity. Component registration identity still triggers
its existing reset behavior. Document this limit; keyed reconciliation remains
separate work.

Visitors see the initialized match node before selection for the current walk,
as loop visitors currently see structural state before sequence reconciliation.
Its selected index is refreshed before child visitation; inspecting that index
inside the match-node callback observes the preceding walk's selection. Document
this timing so visitors do not infer current child counts prematurely.

Descend through the existing stack when a match preserves context. Do not add
recursion for match nesting. Existing context-changing loop recursion remains.
Prepare props/state/actions only for the selected branch. The renderer's later
physical-state traversal must likewise descend only into its selected child,
maintaining the same order as preparation.

Component registration validates every candidate subtree, including inactive
branches. Missing painters or foreign component identities must fail
registration rather than appearing only after a toggle.

## Todo slice and UI

Proposed storage: add reflected `TodoSettings { hide_completed: bool }` in a
named `settings` collection, initialized with exactly one entry and false. This
keeps the flag in the existing slice/collection model without adding a second
application-state storage mechanism or storing it in presentation settings.

Register `toggle_hide_completed(settings: &mut Arena<TodoSettings>)` through
the existing action macro. The dispatcher injects `settings`; the generated
facade takes no request fields. Validate the exactly-one-entry invariant before
mutation, and return a useful error if it is violated. Keep this invariant in
the example's setup/action/resolvers, not in generic Collection behavior.

Build the todo UI with:

1. Existing heading, add action, and animated comets.
2. A loop over the singleton settings collection providing TodoSettings context.
3. A checkbox labeled "Hide completed" / "Erledigte ausblenden", checked from
   `hide_completed`, with activation calling the toggle action.
4. A MatchPart selecting `hide_completed` through a reflected field index:
   - `false`: the existing todos collection loop with all checkbox rows.
   - `true`: a todos collection loop whose body matches `TodoItem.completed`:
     `false` selects the normal row; no matching arm for true renders nothing.

Use a small helper to build the shared row template so both list branches use
identical props and mark-done action bindings. Resolve collection keys and
field indices when constructing templates, following current indexed APIs.
No general boolean operators or sequence-filter expression are needed.

Remove the obsolete headless TodoUi, its visitor/tests, and the tree-printing
binary. Make the native GUI the default todo binary and update its README and
run commands. Engine walker tests continue exercising non-GUI traversal.

The flag is shared across instances; changing it invalidates both windows.
Completing a visible todo while hiding is enabled removes its rendered row.
Turning hiding off shows the same stored item as completed. Hidden rows occupy
no space and have no action target. Existing invalidation clears focus/hover;
stale revisions cannot activate rows after layout changes.

## Implementation checklist

- [ ] Add MatchPart, candidate/pattern APIs, construction validation, and docs.
- [ ] Add MatchState and reconcile/select/walk behavior with active-child
      cleanup.
- [ ] Update all exhaustive LivePart/PartState matches, including test visitors,
      component registration, renderer state collection, and example visitors.
- [ ] Add TodoSettings, initialize its singleton collection, and register the
      toggle action with a generated zero-request-field facade method.
- [ ] Add the localized visibility checkbox and both list branches to the native
      todo UI; remove the headless UI/binary and make the GUI the default.
- [ ] Update live-model/API documentation, example README, and architecture
      documentation. Explain active-only state and shared slice settings; update
      the architecture diagram only if its represented contracts need changing.
- [ ] Run `./n check` after each unit, including follow-up fixes.

## Verification checklist

- [ ] Cover ordered matching, duplicate values, wildcard, no match, empty
      candidates/parts, wrong value types, invalid pattern configurations,
      failed expressions, and comparable non-boolean values.
- [ ] Verify selector evaluation occurs once during ordinary walking; nested
      matches preserve context/application access and depth-first visitation.
- [ ] Test matches inside loops and loops inside matches with independent state.
- [ ] Test repeated selection retains state, switches/no-match drop payloads,
      returning arms initialize via Default, and visitor edits reconcile safely.
- [ ] Validate inactive branches during registration, including empty loops,
      missing painters, and foreign component registrations.
- [ ] Verify preparation and physical-state painting orders agree, inactive
      resolvers/updates/actions never run, and empty branches add no
      rows/hitboxes.
- [ ] Verify rendering errors retain the last published
      output/revision/geometry.
- [ ] Test the singleton settings invariant and zero-field action facade,
      default showing all items, repeated toggles, and completing a hidden-mode
      row without deleting data. Include duplicate todo titles.
- [ ] Test two windows share the visibility flag, stale clicks are rejected, and
      animation remains compatible with current visible action targets.
- [ ] Inspect English/German visibility controls, all-completed/empty lists,
      focus/scroll changes, and repeated toggles in the native todo UI.
- [ ] Run `./n check`; record manual verification accurately before completing
      and moving this plan to the completed folder.

## Assumptions and choices to confirm

- Use a singleton settings collection for the slice flag. A dedicated singleton
  slice-state API would be a broader design change and can be planned
  separately.
- Keep only the active arm's state, dropping it when switching. Preserving each
  arm's state would require additional storage and a different hidden-state
  lifecycle; this plan intentionally proposes fresh state on return.
- Start with typed value equality and a wildcard. Add richer Rust-like patterns
  only when a concrete use case requires them.
- No-match is an empty presentation, not an exhaustiveness error. Type errors
  remain errors so configuration mistakes cannot silently hide the UI.
