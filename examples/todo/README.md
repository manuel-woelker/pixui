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
