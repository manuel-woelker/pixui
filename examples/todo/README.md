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
todo loop. The loop body is a composite with checkbox and label components.
`TodoUi` retains the same template and `LiveState` across renders.

Each render retrieves an owned snapshot with `inspect`, then walks the template
using that reflected snapshot as context. The visitor refreshes checkbox and
label state and prints the physical tree with one row subtree per todo. New
entries initialize on demand; existing presentation state is retained. Snapshots
copy display fields and do not retain worker borrows or modify application data.

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
