# Component identity and focus

## Structural identity

`ComponentId<C>` names a registered component type. `ComponentInstanceId` names
one occurrence in a registered definition. Its `ComponentPath` contains typed
child, loop-item/key and match-arm segments; flattened component indices remain
temporary positions in a successful layout.

An immutable template's sibling keeps its path when an earlier loop changes
length or a match arm appears. Nested loops have separate key namespaces.
Replacing a definition requires a new definition identity; these handles are
process-local and should not be persisted.

`ForLoopPart::new(expression, body)` automatically keys collection expressions
by the full generational arena key. Removing and reusing a slot creates a
different identity. Other sequences retain positional identity unless the loop
uses `with_key`. The resolver receives the current item's expression context:

```rust
use pixui_engine::live_model::identity::ItemKey;

let keyed = loop_part.with_key(|context| {
    let item = context.value()?.downcast_ref::<MyItem>().unwrap();
    Ok(ItemKey::Integer(item.id))
});
```

This is an illustrative application snippet: `loop_part` and `MyItem` are
supplied by the application. Integer and string keys must be immutable and
unique within the current loop. Reusing an explicit key transfers identity to
its replacement; use generational entity identifiers when reuse must invalidate
old handles. Duplicate keys reject preparation. Keyed physical state follows the
same keys as focus; unkeyed state follows positions. Match changes drop the
previous subtree and never automatically restore focus when an arm returns.

## Eligibility

Use `ComponentPart::with_focus` or `with_focus_resolver`:

| Behavior | Pointer/explicit focus | Tab | Default |
| --- | --- | --- | --- |
| Automatic | With activation | With activation | Yes |
| Sequential | Yes | Yes | No |
| Direct | Yes | No | No |
| None | No | No | No |

The resolver runs after props preparation, independently of painting. It can
disable focus from application data. Focus eligibility does not disable pointer
activation: an explicitly nonfocusable action still has an activation region.
There is no text-event handler yet.

Tab/Shift+Tab follow physical preorder, wrap and start at the appropriate end
when no sequential target is selected. Direct targets are skipped. Offscreen
targets scroll into view; fully clipped descendants of non-scrolling containers
are excluded. Partly visible targets remain eligible. Arrow keys are reserved
for future component behavior.

Pointer press selects focus; button release activates and also supports existing
release-only programmatic clicks. Empty-space clicks clear focus. Enter/Space
activate only eligible focused components with activation bindings. A focusable
component without an action simply ignores them.

## Checked focus requests

Obtain a scoped ID from a successfully published instance layout, then send a
command through the ordinary application handle:

```rust
let target = application.inspect(move |app| {
    app.uis().instance(instance)?.component_id(component_index)
})?;
application.ui_command(UiCommand::Focus {
    instance,
    target: Some(target),
})?;
```

The application supplies `application`, `instance`, `component_index`, and the
`UiCommand` import. Sending `target: None` clears focus. Foreign-definition,
removed, ineligible or dirty-geometry targets return an error. Resolving an ID
from a noninteractive component does not grant it focus eligibility.
Stable IDs never bypass presented-revision checks for keyboard or pointer input.

## Shared ownership and failure behavior

Focus and hover remain shared by definition; bounds, clips and bindings are per
instance. Painters receive ordinary `focused` and `hovered` booleans.

Actions retain logical focus and invalidate geometry. Successful preparation of
the latest focus source clears targets that disappeared or became ineligible.
Failed preparation or publication keeps the previous logical focus and published
layout. A peer with a different active tree paints no focus highlight but cannot
clear another source's logical focus.

Native blur retains logical focus. Hidden sources defer reconciliation until
shown; focus input in another window supersedes them. Closing the source selects
the oldest remaining instance deterministically, or clears focus if none remain.
Hover separately follows the latest pointer source and is retested against new
geometry; pointer leave, hide and close clear its source.

The implementation uses vector paths and ordinary maps. It provides no arbitrary
live-template mutation guarantee, custom tab order, focus scopes, accessibility
bridge, spatial navigation, or text editing.
