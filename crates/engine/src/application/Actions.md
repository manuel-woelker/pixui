# Application actions

`Application::new()` immediately starts an owner thread and returns an
`ApplicationHandle`. The handle contains only a cheaply clonable bounded MPSC
sender. Add configured slices and dispatch actions through that handle; the
worker owns the application state and executes commands sequentially.

Actions are ordinary functions. `#[action]` generates owned request types,
reflection metadata, and adapters that borrow data during dispatch. Handlers
capture no application state and can also be called directly.

```rust
use pixui_base::Key;
use pixui_engine::application::{
    app::Application, application_slice::ApplicationSlice, collection::Collection,
};

mod handlers {
    use pixui_base::{Arena, Key, PixuiResult};
    use pixui_engine::application::action::action;
    pub struct TodoItem { pub title: String, pub completed: bool }

    /// Creates an incomplete todo.
    #[action]
    pub fn add_todo(todos: &mut Arena<TodoItem>, title: String) -> PixuiResult<Key<TodoItem>> {
        Ok(todos.insert(TodoItem { title, completed: false }))
    }

    /// Marks an existing todo complete.
    #[action]
    pub fn mark_done(todo: &mut TodoItem) { todo.completed = true; }
}
use handlers::{TodoItem, add_todo_action, mark_done_action};

let application = Application::new();
let mut slice = ApplicationSlice::new("todo");
slice.add_collection(Collection::new::<TodoItem>("todos"))?;
slice.register_action(add_todo_action::descriptor())?;
slice.register_action(mark_done_action::descriptor())?;
let add_todo = slice.action_handle_named("add_todo")?;
let mark_done = slice.action_handle_named("mark_done")?;
let slice_id = application.add_slice(slice)?;

let call = add_todo.call(vec![
    Box::new(String::from("Buy milk")),
])?;
let key = *application.dispatch(call)?.wait()?.downcast::<Key<TodoItem>>().unwrap();
let todo = application.object_ref(slice_id, "todos", key)?;
let call = mark_done.call(vec![Box::new(todo)])?;
let caller_handle = application.clone();
std::thread::spawn(move || caller_handle.dispatch(call)?.wait()).join().unwrap()?;
let completed = application.inspect(move |state| {
    Ok(state.slice(slice_id)?.collection("todos")?.arena::<TodoItem>().unwrap()
        .get(key).unwrap().completed)
})?;
assert!(completed);
# Ok::<(), pixui_base::PixuiError>(())
```

## Handler parameters and request fields

| Handler parameter | Request field | Dispatch behavior |
|---|---|---|
| `title: String` | `title: String` | Move the supplied value |
| `todo: &mut TodoItem` | `todo: ObjectRef<TodoItem>` | Resolve and exclusively borrow the item |
| `todos: &mut Arena<TodoItem>` | Omitted | Inject the collection named `todos` in the target slice |

The generated module is named `<function>_action`. Its `request::Request`
implements `Reflect`; `descriptor()` returns a static `ActionDescriptor`
initialized once. Doc comments supply the description, and the function name
supplies the action name. Renaming an injected parameter changes its collection
binding. `collections()` exposes bindings and expected item types.

`arguments()` describes the request, not the borrowed handler arguments.
Request fields follow parameter order with injected collections omitted.
Construction uses `TypeDescriptor::construct_send` with exact types, owned
sendable values, and no coercions or defaults. Requests can also be constructed
directly and boxed. `#[action]` opts the request into `#[reflect(send)]`.
Ordinary borrowed reflection remains available separately.

## Registration and identity

Configure collections and actions on an `ApplicationSlice`, then pass it to
`ApplicationHandle::add_slice`. Registration waits for the worker's reply and
returns a stable `SliceId`. `add_collection` rejects empty or duplicate names.
Collection names and arena types are immutable; several collections of the
same type may coexist. `register_action` checks every injected collection's
name and type before adding the action. Missing collections, wrong types,
empty names, and duplicate action names return errors without changing
registration. Add collections before actions.

Collection and action registration is append-only. Typed mutation exposes an
arena without permitting collection replacement. Action names resolve to
`ActionIndex`; indexed access checks bounds. An in-range index from another
slice cannot be distinguished, so retain indices with their slice identity.
Name lookup is linear and case-sensitive.

`ObjectRef<T>` combines a process-local `SliceId`, collection arena identity,
and typed generational key. `ApplicationHandle::object_ref` validates the item
on the worker before returning a handle. These opaque reflected values retain
no borrow and have no field constructor. Dispatch rechecks identities, types,
and generations. Removed items, removed slices, and foreign handles return
errors. Slice reordering preserves handles. Handles are addresses, not
serialization or authorization tokens. They may address another slice in the
same application.

## Queueing and inspection

`Application::new` uses a capacity of 128 commands. `with_capacity` selects
another bound; zero is rendezvous. Construction panics if the worker cannot
start or the channel capacity cannot be allocated. The handle retains no
worker join handle or shared state.

`ActionHandle` caches a slice ID, action index, and static descriptor. It is
`Copy`, can be shared across caller threads, and does not keep the worker alive.
`call(fields)` validates and constructs requests locally without channels or
locks. Obtain handles from `ApplicationSlice::action_handle(index)` or
`action_handle_named(name)` before transferring the slice to the worker. This
also avoids any initial worker lookup. Append-only registration keeps cached
indices stable. Construction does not check whether the slice is currently
attached or alive; dispatch validates the target and any object references.

For an already attached slice, `ApplicationHandle::action(slice_id, name)`
resolves a handle with one worker round trip. Reuse that handle for later calls:

```rust
use pixui_engine::application::{app::Application, application_slice::ApplicationSlice};
mod handlers {
    use pixui_engine::application::action::action;
    #[action]
    pub fn sum(left: i32, right: i32) -> i32 { left + right }
}
let application = Application::new();
let mut slice = ApplicationSlice::new("math");
slice.register_action(handlers::sum_action::descriptor())?;
let slice_id = application.add_slice(slice)?;
let sum = application.action(slice_id, "sum")?;
let call = sum.call(vec![Box::new(2i32), Box::new(3i32)])?;
let result = application.dispatch(call)?.wait()?;
assert_eq!(*result.downcast::<i32>().unwrap(), 5);
# Ok::<(), pixui_base::PixuiError>(())
```

`ApplicationHandle::action_call` remains a convenience: it resolves the handle
on the worker each time, then constructs the request locally. Prefer caching
an action handle for repeated calls. `descriptor()` exposes request fields and
action descriptions for dynamic input discovery.

`dispatch(call)` blocks for queue capacity, then returns `PendingAction`.
`wait()` consumes the reply and waits for completion. `try_dispatch(call)`
returns immediately; full or disconnected errors preserve the call for retry
with `into_inner()`. Blocking send errors drop the unsent call. The queue
carries actions, registration, action lookup, and inspection commands.
Concurrent producers have no predefined ordering between them.

`inspect` runs a read callback on the worker and returns owned, sendable data.
References cannot escape. Reads observe preceding commands in queue order and
are not concurrent with actions. Do not call blocking handle methods from
inside a handler or inspection on that same worker: it cannot process its own
pending commands.

Each reply queue holds one result, so late or abandoned replies do not block
the worker. Dropping a reply does not cancel accepted work. The request bound
limits queued commands, not the results retained by callers.

Drop every handle clone to close the queue. Accepted commands drain before the
worker exits and drops its state. Dropping handles does not wait for thread
termination; wait for replies when completion matters. Handler errors are
returned without stopping the worker. A panicking command is caught, its state
is discarded, and queued and future commands are dropped as failures until
all handles are gone. Replies then return errors instead of hanging on senders
retained by buffered commands. State is never reused after a panic, and no
rollback is provided. The normal Rust panic hook still runs.

## Types and current limits

`SendValue` and `SendValues` live in `pixui_base::erased_value`. `ActionRequest`,
`ActionOutput`, `ActionResult`, and `ActionHandler` live in the action module.
Channel endpoints use internal aliases. Requests, outputs, and collection
values require `'static + Send`; `Sync` is not required. Outputs are boxed owned
values. `PixuiResult<T>` propagates errors and boxes only `T`; other result types
are ordinary return values. Void handlers produce boxed `()`.

Generated handlers must be safe, synchronous, non-generic free functions at
module scope with simple parameter names. Initially only one mutable parameter
is supported. Shared reference parameters, explicit lifetimes, borrowed outputs,
destructuring, and conditional parameters are unsupported. Collection injection
recognizes `&mut Arena<T>` (qualified paths work), not aliases. Consumers must
use canonical `pixui_engine` and `pixui_reflect` dependency names.

`Application::default` creates bare state without starting a worker, for direct
adapter tests and advanced integrations. The primary API is `Application::new`
and its sender-only handle.
