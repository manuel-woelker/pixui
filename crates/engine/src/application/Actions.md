# Application actions

The
[application state decision record](../../../../docs/decisions/DR-002%20Organize%20application%20state%20into%20slices%20and%20typed%20collections.md)
explains why state is organized into slices and typed collections, including the
tradeoffs compared with a plain Rust root struct.

`Application::new()` immediately starts an owner thread and returns an
`ApplicationHandle`. The handle contains only a cheaply clonable bounded MPSC
sender. Add configured slices and dispatch actions through that handle; the
worker owns the application state and executes commands sequentially.

The
[worker and queue decision record](../../../../docs/decisions/DR-003%20Own%20application%20state%20on%20a%20worker%20thread%20with%20a%20bounded%20MPSC%20queue.md)
documents ownership, backpressure, lifecycle, and the tradeoffs against shared
locks.

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
let slice_id = application.add_slice(ApplicationSlice::new("todo"))?;
application.add_collection(slice_id, Collection::new::<TodoItem>("todos"))?;
application.register_action(slice_id, add_todo_action::descriptor())?;
application.register_action(slice_id, mark_done_action::descriptor())?;
let add_todo = application.action(slice_id, "add_todo")?;
let mark_done = application.action(slice_id, "mark_done")?;

let call = add_todo.call(vec![
    Box::new(String::from("Buy milk")),
])?;
let key = *application.dispatch(call)?.wait()?.downcast::<Key<TodoItem>>().unwrap();
let todo = application.object_ref(slice_id, "todos", key)?;
let call = mark_done.call(vec![Box::new(todo)])?;
let caller_handle = application.clone();
std::thread::spawn(move || caller_handle.dispatch(call)?.wait()).join().unwrap()?;
let completed = application.inspect(move |state| {
    Ok(state.collection(slice_id, "todos")?.arena::<TodoItem>().unwrap()
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
| `hide_done: EntityMut<bool>` | Omitted | Borrow the entity named `hide_done` in the target slice |

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

`Application` owns an append-only collection store. `register_collection`
returns an opaque `CollectionIndex`; its position supports direct lookup and its
identity rejects foreign application indices. Collection names are diagnostic
defaults, not globally unique names. `ApplicationSlice::bind_collection` maps a
local name to an index, and multiple names or slices may share the same storage.
`add_slice` validates all index identities before attaching the slice.

Alternatively, add an empty slice and call
`add_collection(slice_id, collection)` to register storage and bind its default
name atomically. Duplicate or empty binding names leave storage and bindings
unchanged. `bind_collection` attaches existing storage. Collection item types
and bindings cannot be replaced.

Register actions through `Application` or `ApplicationHandle` after the slice
and its collections exist. `register_action` validates injected names and types;
`register_actions` validates its entire batch before modifying registration.
Missing names, wrong types, duplicate action names, and multiple mutable
collection parameters resolving to the same index return errors. The generated
adapter currently supports at most one mutable parameter; manual descriptors
with multiple collection bindings still undergo alias checks.

Action registration is append-only. Names resolve to `ActionIndex`; indexed
access checks bounds. Retain action indices with their slice identity. Action
name lookup is linear and case-sensitive; slice-local collection bindings use a
hash map. `CollectionKey` is an alias for the application-level
`CollectionIndex`. `collection_key(slice_name, binding_name)` resolves a
reusable index once.

`ObjectRef<T>` combines a collection index and typed generational arena key.
`object_ref(slice_id, name, key)` resolves a local binding;
`object_ref_at(index, key)` addresses storage directly. Both validate the item
before returning a borrow-free handle. Dispatch rechecks identities, types and
generations. Foreign handles and removed items fail. Removing a slice
invalidates calls targeting it, but collections, collection expressions and item
references remain valid until application shutdown. References can be used by
actions in other slices; they are addresses, not serialization or authorization
tokens.

## Named entities

Create individual named values ergonomically on a slice. They are staged until
`add_slice`, then inserted into one lazily created unnamed collection per
concrete type. Multiple booleans occupy distinct arena slots; explicit
collections remain separate. Values require `Reflect + Send`, without a `Sync`
requirement. Entity, collection and action names use independent namespaces.

```rust
use pixui_engine::application::{
    action::slice_actions, app::Application, application_slice::ApplicationSlice,
};
#[slice_actions(slice = "flags", facade = FlagActions)]
mod actions {
    use pixui_engine::application::entity_mut::EntityMut;
    #[action]
    pub fn toggle(mut hide_done: EntityMut<bool>) { *hide_done = !*hide_done; }
}
let application = Application::new();
let mut slice = ApplicationSlice::new("flags");
slice.bind("hide_done", false)?;
slice.bind("show_details", true)?;
let id = application.add_slice(slice)?;
actions::FlagActions::register(&application, id)?;
let actions = actions::FlagActions::bind(&application)?;
actions.toggle()?;
let reference = application.entity_ref::<bool>(id, "hide_done")?;
let enabled = application.inspect(move |app| Ok(*app.resolve(reference)?))?;
assert!(enabled);
application.bind(id, "allow_editing", true)?;
# Ok::<(), pixui_base::PixuiError>(())
```

`slice.bind_entity(name, existing_ref)` shares an existing item. Attachment
validates all supplied references and collection indices before inserting staged
values. Empty/duplicate names fail before insertion, both during staging and
through `application.bind(id, name, value)`. Staged values cannot expose a ref
before allocation; `entity_ref::<T>(id, name)` retrieves it afterward.
`create_entity(value)` creates an unbound value for explicit sharing; registered
slices also support `application.bind_entity(id, name, reference)`.

Bindings are append-only. Removing a slice removes its names, not its stored
items. Removing an item through its arena makes its bindings and expressions
stale; type and generation validation return errors instead of retargeting a
replacement item. Anonymous collection and item allocations remain until
application shutdown unless explicitly removed through arena operations.

`EntityMut<T>` implements `Deref` and `DerefMut`, and
`EntityMut::new(&mut value)` allows directly calling the same ordinary action
function. Its parameter name selects the entity exactly, including case. It is
injected and omitted from both request fields and facade parameters.
Registration validates existence, type, and conflicting mutable aliases;
dispatch checks liveness again. Generated handlers still support at most one
mutable parameter, including EntityMut. Qualified EntityMut paths work; aliases
and explicit lifetime arguments do not. `&mut T` retains its caller-supplied
ObjectRef behavior.

Use `Expression::entity(reference)` to read a live reflected value without a
singleton collection loop. Struct field expressions can still read fields of
entities when that value is the current expression context. Erased named
bindings use `ErasedObjectRef`, which shares only address metadata via Arc; the
actual values remain ordinary items in application-owned arenas. See
[DR-013](<../../../../docs/decisions/DR-013 Bind named entities to items in per type application collections.md>)
for the storage and staging rationale.

## Queueing and inspection

`Application::new` uses a capacity of 128 commands. `with_capacity` selects
another bound; zero is rendezvous. Construction panics if the worker cannot
start or the channel capacity cannot be allocated. The handle retains no
worker join handle or shared state.

`ActionHandle` caches a slice ID, action index, and static descriptor. It is
`Copy`, can be shared across caller threads, and does not keep the worker alive.
`call(fields)` validates and constructs requests locally without channels or
locks. Obtain handles through `ApplicationHandle::action` or from a registered
`ApplicationSlice` while already on the owner thread. Append-only registration
keeps cached indices stable. Construction does not check whether the slice is
currently attached or alive; dispatch validates the target and any object
references.

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
let slice_id = application.add_slice(ApplicationSlice::new("math"))?;
application.register_action(slice_id, handlers::sum_action::descriptor())?;
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

`SendValue` and `SendValues` live in `pixui_base::erased_value`.
`ActionRequest`, `ActionOutput`, `ActionResult`, and `ActionHandler` live in the
action module. Channel endpoints use internal aliases. Requests, outputs, and
collection values require `'static + Send`; `Sync` is not required. Outputs are
boxed owned values. `PixuiResult<T>` propagates errors and boxes only `T`; other
result types are ordinary return values. Void handlers produce boxed `()`.

Generated handlers must be safe, synchronous, non-generic free functions at
module scope with simple parameter names. Initially only one mutable parameter
is supported. Shared reference parameters, explicit lifetimes, borrowed outputs,
destructuring, and conditional parameters are unsupported. Collection injection
recognizes `&mut Arena<T>` (qualified paths work), not aliases. Consumers must
use canonical `pixui_engine` and `pixui_reflect` dependency names.

`Application::default` creates bare state without starting a worker, for direct
adapter tests and advanced integrations. The primary API is `Application::new`
and its sender-only handle.

## Generated slice facades

Use an inline module to collect ordinary action functions and generate a typed
client facade. The macro is available alongside `action` in
`pixui_engine::application::action`. Consumers need the `pixui_base`,
`pixui_engine`, and `pixui_reflect` crate names, as with standalone actions.

```rust
use pixui_base::Key;
use pixui_engine::application::{
    action::slice_actions, app::Application,
    application_slice::ApplicationSlice, collection::Collection,
};

#[slice_actions(slice = "notes", facade = NoteActions)]
mod actions {
    use pixui_base::{Arena, Key};

    /// Adds a note to the injected notes collection.
    #[action]
    pub fn add(notes: &mut Arena<String>, title: String) -> Key<String> {
        notes.insert(title)
    }
}

let application = Application::new();
let slice = application.add_slice(ApplicationSlice::new("notes"))?;
application.add_collection(slice, Collection::new::<String>("notes"))?;
actions::NoteActions::register(&application, slice)?;
let actions = actions::NoteActions::bind(&application)?;
let key: Key<String> = actions.add("A note")?;
# Ok::<(), pixui_base::PixuiError>(())
```

The facade is generated **inside the annotated module**, with a public method
for each `#[action]` function. Other module items remain ordinary Rust items.
Registration discovers enabled actions automatically and validates the whole
batch before adding any action. Configure collections first. Use
`register(&application_handle, slice_id)` from callers or
`register_in(&mut application, slice_id)` when already on the owner thread.

`bind` uses the attribute's exact slice name; application slice names must be
nonempty, unique, and immutable. `bind_to` accepts a `SliceId` to use another
instance of the same actions. Both check all exact handler descriptors in one
worker round trip, then cache their indices. A matching action name alone is
insufficient: binding to a different handler returns an error.

Generated parameters follow the request schema:

- `&mut Arena<T>` and `EntityMut<T>` are injected and omitted.
- `&mut T` becomes an owned `ObjectRef<T>`; resolve it with the application's
  `object_ref` method and the facade's `slice_id()`.
- Owned `String` becomes `impl Into<String>`, accepting string literals,
  owned strings, and custom conversions. Qualified `std::string::String` works;
  type aliases retain their original parameter type.
- Other owned parameters retain their concrete types.

Calls construct the generated typed request locally, enqueue it through the
bounded channel, and wait for completion. They return `PixuiResult<T>` for an
owned output `T`, `PixuiResult<()>` for no output, and flatten a handler's
syntactically named `PixuiResult<T>`. Other result types are ordinary outputs.
Handler errors and worker failures propagate. No reflective field vector or name
lookup is needed per call.

Facades can be cloned and called from multiple threads. A facade retains a
sender, keeping its worker alive. Calls are synchronous and may wait for queue
capacity; never call them from that application's worker, including an inspect
callback. Existing standalone action restrictions still apply, including at most
one mutable argument. The method names `bind`, `bind_to`, `bind_target`,
`register`, `register_in`, and `slice_id` are reserved. Conditional compilation
attributes on action functions also apply to their generated facade members.
