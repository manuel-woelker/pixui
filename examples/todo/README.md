# Todo example

Creates an application with a `todo` slice and a `todos` collection backed by
an erased `Arena<TodoItem>`. Ordinary action functions describe their inputs:

```rust
fn add_todo(todos: &mut Arena<TodoItem>, title: String) -> PixuiResult<Key<TodoItem>>;
fn mark_done(todo: &mut TodoItem);
```

`#[action]` generates reflected request types and dispatch adapters. Doc comments
become action descriptions. Registration checks that `todos` exists and contains
`TodoItem`; multiple collections of the same type are allowed.

- `add_todo` requests contain only `title`. Dispatch injects the `todos` arena
  from the target slice. Blank titles are rejected before inserting a task.
- `mark_done` requests contain only `todo: ObjectRef<TodoItem>`. This opaque,
  owned handle identifies the slice, collection, and arena key without retaining
  a borrow. Dispatch resolves it to `&mut TodoItem` and rejects stale references.

The application is owned directly. There is no captured application state or
interior mutability. Requests can be built from vectors of boxed, owned `Send` fields
and queued while the application changes. Dispatch acquires borrows for the
handler's duration. Repeated `mark_done` calls are harmless.

The binary creates an initial task, then moves the application into a
`DispatchLoop` on a worker thread. A bounded `Dispatch` sends calls from the main
thread and a second caller thread. Reply handles return action results. Dropping
all dispatchers drains accepted calls and returns the application for printing.

Run from the repository root:

```sh
./t cargo run -p pixui-example-todo
```

See the engine's [action documentation](../../crates/engine/src/application/Actions.md)
for registration rules and current limitations.
