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

The example caches copyable `ActionHandle`s from the configured slice before
adding it. Request construction is local, with no registry, locks, or channel
round trips. The main thread and a second caller thread dispatch through cloned
application handles.
Reply handles return results, and `inspect` retrieves an owned snapshot for
printing. Application state stays on its worker without `Rc` or `RefCell`.
Dropping all handles closes the queue and drains accepted commands.

Run from the repository root:

```sh
./t cargo run -p pixui-example-todo
```

See the engine's
[action documentation](../../crates/engine/src/application/Actions.md) for
lifecycle details and current limitations.
