# Todo example

`Application::new()` starts the worker internally and returns a cheaply clonable
`ApplicationHandle` containing only a bounded MPSC sender. The example adds a
configured `todo` slice containing a `todos` collection of `TodoItem`s.

Ordinary action functions describe their inputs:

```rust
fn add_todo(todos: &mut Arena<TodoItem>, title: String) -> PixuiResult<Key<TodoItem>>;
fn mark_done(todo: &mut TodoItem);
```

`#[action]` generates reflected request types and dispatch adapters. Doc
comments become descriptions. Registration checks that `todos` exists and
contains `TodoItem`; multiple collections of that type are allowed.

- `add_todo` requests contain only `title`. Dispatch injects the named arena;
  blank titles are rejected before insertion.
- `mark_done` requests contain only an opaque `ObjectRef<TodoItem>`. Dispatch
  resolves it to `&mut TodoItem` and rejects stale or foreign handles.

The generated `TodoActions` facade binds to the named slice and caches its
action handles. Request construction is local; facade methods dispatch and wait
for typed results. The main thread and a second caller thread use cloned
facades. Application state stays on its worker without `Rc` or `RefCell`.
Dropping all handles closes the queue and drains accepted commands.

## Component tree

`ui.rs` defines a `LivePart` template: a composite containing a heading and a
todo loop. A collection expression selects the slice's reflected todos arena.
The loop body is a composite with checkbox and label components. `TodoUi`
retains the same template and `LiveState` across renders.

Construction resolves a `CollectionKey` for todos by name once. Each render
moves the owned template and UI state to an `inspect` callback on the
application worker. The walker evaluates the collection expression and borrows
live `TodoItem` values as loop contexts. The visitor refreshes checkbox and
label state and prints the physical tree with one row subtree per todo. New
entries initialize on demand; existing presentation state is retained. UI state
and output text return to the caller; application borrows do not escape and no
snapshot collection is created. Component payloads are owned and sendable. A
worker communication failure discards the transferred UI state.

The binary prints the tree before and after inserting a second todo. This is a
textual component UI demonstration; it does not create a graphical window. State
is matched by position, so the example appends items rather than reordering
them.

Run from the repository root:

```sh
./t cargo run -p pixui-example-todo
```

See the engine's
[action documentation](../../crates/engine/src/application/Actions.md) for
lifecycle details and current limitations.

## Native GUI

Run the graphical example on a desktop session:

```sh
./t cargo run -p pixui-example-todo --bin gui
```

The GUI opens two windows from one `UiDefinition`: English/light at 640 by 480
and German/dark at 420 by 640. Each window has its own worker-side `UiInstance`
with presentation settings, component state, layout, hit regions, focus, hover,
and scrolling. Both instances read the same todo collection.

Click **Add todo** to append a generated task, or click a checkbox to mark a
task done. Both windows update. Tab moves focus; Enter or Space activates the
focused control. The mouse wheel scrolls overflowing content. Resize windows to
see independent wrapping, and close either window without closing the other.

`gui_ui.rs` supplies label, button, and checkbox presentation callbacks on live
parts. The application worker walks, measures, lays out, and emits owned
`RenderOutput`s containing `DisplayList`s. The GUI thread owns winit windows and
softbuffer surfaces and paints the commands. Events identify the displayed
revision and return through the application queue. Stale clicks are rejected
rather than targeting an item at a changed position.

The initial font uses fixed bitmap cells with Latin extensions. Translation of
example labels is explicit; general localization, complex text shaping, and
editable text controls are future work. Todo row bindings currently resolve an
arena key by borrowed-value identity, with a linear search per row. Carrying
stable keys through loop contexts would remove that lookup and help implement
keyed component-state reconciliation.
