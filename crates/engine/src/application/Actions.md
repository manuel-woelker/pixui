# Application actions

Actions are ordinary functions. `#[action]` generates owned request types,
reflection metadata, and adapters that borrow application data during dispatch.
Handlers capture no application state and can also be called directly.

```rust
use pixui_base::Key;
use pixui_engine::application::{
    app::Application, application_slice::ApplicationSlice,
    collection::Collection,
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
pub fn mark_done(todo: &mut TodoItem) {
    todo.completed = true;
}
}
use handlers::{TodoItem, add_todo_action, mark_done_action};

let mut slice = ApplicationSlice::new("todo");
slice.add_collection(Collection::new::<TodoItem>("todos"))?;
slice.register_action(add_todo_action::descriptor())?;
slice.register_action(mark_done_action::descriptor())?;
let slice_id = slice.id();
let mut application = Application::new();
application.slices.push(slice);

let call = application.action_call(slice_id, "add_todo", vec![
    Box::new(String::from("Buy milk")),
])?;
let key = *application.dispatch(call)?.downcast::<Key<TodoItem>>().unwrap();
let todo = application.object_ref(slice_id, "todos", key)?;
let call = application.action_call(slice_id, "mark_done", vec![
    Box::new(todo),
])?;
application.dispatch(call)?;
assert!(application.resolve_mut(todo)?.completed);
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
binding. `collections()` exposes those bindings and expected item types.

`arguments()` describes the request, not the borrowed handler arguments.
Request fields follow parameter order with injected collections omitted.
Construction uses `TypeDescriptor::construct_send` with exact types, owned
sendable values, and no coercions or defaults. The generated request can also be
constructed directly and boxed. `#[action]` opts its generated request into
`#[reflect(send)]`. Ordinary borrowed reflection remains available separately.

## Registration and identity

`add_collection` rejects empty or duplicate collection names. Names and arena
types are immutable after insertion. Collections of the same type may coexist.
`register_action` checks every injected collection's name and type before adding
the action; missing collections, wrong types, empty names, and duplicate action
names return errors without changing registration. Add collections first.

Collection and action registration is append-only. Getters expose immutable
metadata; typed mutation exposes the arena without allowing replacement of the
collection. Action names resolve to `ActionIndex`; indexed access checks bounds.
An in-range index from another slice cannot be distinguished, so retain indices
with their slice identity. Name lookup is linear and case-sensitive.

`SliceId` and collection arena identities are never reused during the process.
`ObjectRef<T>` combines these identities with a typed, generational arena key.
`Application::object_ref` checks that the item exists before creating a handle.
Handles are opaque reflected values, without a field constructor. They can be
supplied to requests but cannot be fabricated from unchecked integer keys.
Dispatch rechecks identities, item types, and generations. Removed items,
removed slices, and foreign handles return errors. Slice reordering preserves
handles. Moving a slice into another application moves the data its handles
identify. Handles are process-local addresses, not serialization or authorization
tokens. Item references may address another slice in the same application.

## Dispatch and limits

`ActionCall` owns its target slice identity, action index, and reflected request.
Building a call retains no borrow, so it may wait while application data changes.
`dispatch` verifies the target and request type, resolves inputs, calls the
handler, and returns its owned output as `ActionOutput` (`SendValue`). A handler returning
`PixuiResult<T>` propagates its error and boxes only `T`; an ordinary owned return
is boxed as-is. A void handler returns boxed `()`. Other result types are ordinary
return values; their errors are not automatically propagated.

Generated handlers are safe, synchronous, non-generic free functions at module
scope with simple
parameter names. Initially only one mutable parameter is supported; supporting
multiple targets needs safe disjoint borrowing and alias checks. Shared reference
parameters, explicit lifetimes, borrowed outputs, destructured parameters, and
conditional parameters are unsupported. The macro recognizes collection injection
from the syntax `&mut Arena<T>` (qualified paths work); aliases are not recognized.
Owned input and output types must be `'static + Send`. Collection values must
also be `Send`, allowing the application to move to its owner thread; `Sync` is
not required. Errors do not roll back mutations,
and panics propagate. Direct dispatch performs no implicit scheduling. For calls
from multiple threads, use the bounded `Dispatch` queue described below.
Consumers must depend on `pixui_engine` and `pixui_reflect` under their canonical
crate names for generated adapters and reflection code.


## Bounded dispatch from multiple threads

`Dispatch::new(capacity)` returns a clonable producer and a single `DispatchLoop`.
Run the loop with an owned application on a dedicated thread. The caller chooses
the capacity; zero permits rendezvous without buffering.

Cache a slice ID, action index, and descriptor before moving the application.
Callers can then build requests without accessing application state:

```rust
use pixui_engine::application::{
    action::ActionCall, app::Application, application_slice::ApplicationSlice,
    collection::Collection, dispatch::Dispatch,
};

mod handlers {
    use pixui_base::{Arena, Key};
    use pixui_engine::application::action::action;
    #[action]
    pub fn append(numbers: &mut Arena<i32>, value: i32) -> Key<i32> {
        numbers.insert(value)
    }
}

let mut slice = ApplicationSlice::new("numbers");
slice.add_collection(Collection::new::<i32>("numbers"))?;
let action = slice.register_action(handlers::append_action::descriptor())?;
let descriptor = slice.action(action)?;
let slice_id = slice.id();
let mut application = Application::new();
application.slices.push(slice);
let (dispatch, owner) = Dispatch::new(16);
let worker = std::thread::spawn(move || owner.run(application));
let caller_dispatch = dispatch.clone();
let caller = std::thread::spawn(move || {
    let request = descriptor.arguments().construct_send(vec![Box::new(7i32)])?;
    caller_dispatch.dispatch(ActionCall { slice: slice_id, action, request })?.wait()
});
let result = caller.join().expect("caller panicked")?;
assert!(result.is::<pixui_base::Key<i32>>());
drop(dispatch);
let application = worker.join().expect("owner panicked");
assert_eq!(application.slice(slice_id)?.collection("numbers")?.arena::<i32>().unwrap().len(), 1);
# Ok::<(), pixui_base::PixuiError>(())
```

`dispatch(call)` blocks when the queue is full, then returns `PendingAction`.
`wait()` consumes that handle and blocks for the result. `try_dispatch(call)`
returns immediately; on full or disconnected queues, its error retains the call
for retry via `into_inner()`. Blocking enqueue errors drop the unsent call.
Calls execute sequentially in receive order. Concurrent producers have no
predefined ordering between them. Handler errors reach callers and do not stop
the loop. Dropping a pending handle discards its result but does not cancel the
accepted action. Reply channels hold one result each, so waiting late or abandoning
a reply never blocks the owner loop. The bounded request queue limits waiting
calls, not the memory retained by callers holding completed replies.

Drop every dispatcher clone to stop the loop gracefully. It drains accepted
calls and returns the application. Dropping the loop or a handler panic causes
unfinished replies and future sends to fail. A separate owner-lifetime channel
notifies reply waiters even when buffered requests retain their reply senders.
Completed replies take precedence over the owner's shutdown notification.
Panics are not caught, and no rollback is provided.

Avoid synchronously enqueueing or waiting on the same dispatcher from its owner
thread: the owner cannot drain its own full queue or execute its own pending reply.
No implicit thread is spawned and no serialization is implemented.

The aliases `SendValue` and `SendValues` live in `pixui_base::erased_value`.
`ActionRequest`, `ActionOutput`, `ActionResult`, and `ActionHandler` live in the
action module. Channel endpoints use internal aliases in the dispatch module.
All transported inputs, outputs, and application collections require `Send`;
none require shared application access or `Sync`.
