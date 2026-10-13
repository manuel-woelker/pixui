# Component events

Components declare `type Events`, alongside `Props` and `State`. Use
`NoEvents` for inert components, or a trait annotated with
`pixui_engine_macros::component_events`. Event methods have `&self`, owned
reflected `Send + 'static` parameters, and `PixuiResult<()>` returns. Generics,
async methods, supertraits, default bodies and borrowed parameters are rejected.

```rust
use pixui_base::PixuiResult;
use pixui_engine_macros::component_events;
use pixui_engine::events::interface::{EventInterface, EventSink};

#[component_events]
trait EditorEvents {
    fn change(&self, content: String) -> PixuiResult<()>;
    fn submit(&self) -> PixuiResult<()>;
}

let sink = EventSink::new(<dyn EditorEvents>::descriptors(), vec![true, false])?;
<dyn EditorEvents>::with_proxy(&sink, |events| events.change("hello".into()))?;
let emissions = sink.into_pending();
assert_eq!(emissions.len(), 1);
assert_eq!(emissions[0].index, 0);
assert_eq!(emissions[0].payload.read_named("content")?.downcast_ref::<String>(),
    Some(&"hello".to_owned()));
# Ok::<(), pixui_base::PixuiError>(())
```

The macro generates a worker-local proxy, `EditorEventsChangePayload` and
`EditorEventsSubmitPayload`, immutable descriptors, and typed event keys in
`EditorEventsBindings`. Method order determines indices; names are resolved
before UI publication. Payload fields come from parameter names. The proxy
shares a local queue through `Rc<RefCell<_>>`; no mutex or application channel
is used. Neither the interface nor the proxy needs `Send`/`Sync` or cloning.
Parameter names `event`, `entities`, `collections`, `layouts` and `styles` are
reserved callback roots.

Declare `type Events = dyn EditorEvents` on the component. Its `handle_input`
hook receives `&Self::Events`, typed props, mutable retained state and a
`ComponentInput`. Call `events.change(proposed_content)?`, rather than
constructing an erased value. An emission result reports validation/enqueue
errors. The bound action runs later, so its failures are reported through the
worker error path. Emitting an unbound event succeeds without queuing work.

## Targeting and state

The host forwards backend-independent input. The worker chooses a target using
the source window's published, clipped geometry or the shared focused component.
Set `Component::INPUT_INTEREST` to the narrowest relevant interests. `Click`
means a matching press/release gesture over the same component; its coordinates
are logical pixels in the source window. `Activate` is semantic activation and
also arises from keyboard input. `Raw` preserves low-level input for custom
behavior. Return `true` to consume input and prevent generic handling.

Global Tab/F11 policy and built-in text editing take precedence. No ancestor
propagation or general drag capture is implemented. Existing text selection
capture remains separate. Hidden windows and stale geometry cannot retarget
input; lost focus, hiding and application invalidation cancel pending clicks.
Pointer movement does not imply focus participation.

Hooks access the source instance's retained component state through its stable
path, without reevaluating the definition. Shared focus/hover/editing remain in
definition state. Painters and measurement never emit events. Worker actions
execute after the hook releases its state borrow, in emission order. Failure
stops the remaining batch and preserves earlier mutations. Multiple windows do
not replay an emission from their peer.

## Binding callbacks

Rust declarations use generated typed keys:

```text
part.with_event(EditorEventsBindings::CHANGE, change_factory)
```

The factory returns `TypedBinding<EditorEventsChangePayload>`, taking
`(&Application, &EditorEventsChangePayload)` and returning an owned
`ActionCall`. Registration checks both the name and payload identity; an equally
named event from a different interface is rejected. `with_activation` and
`with_change` are migration helpers backed by the same event binding storage and
invocation path.

Source definitions use event methods and named action request fields:

```text
<Column slice='todos'>
    <TextInput content=entities.draft @change=set_draft(value: content) />
    <Button label="Add" @activate=add(title: entities.draft) />
</Column>
```

Bare event parameters are callback locals; `event.content` is equivalent to
`content`. They shadow enclosing loop names only inside that callback. Action
argument names refer to request fields, so `value:` need not match `content`.
There is no parameter-renaming/lambda syntax or automatic two-way binding.

Containers supply inherited slice scopes. `todos.add(...)` overrides the nearest
scope, and a bare `add(...)` requires one. Host environments explicitly export
permitted slice/action pairs; setting a scope never grants extra capabilities.
Callbacks initially invoke one action. Custom Rust behavior may emit multiple
independently bound events in one input turn.

## Snapshots and errors

Successful preparation publishes bindings atomically with their props and
geometry. Callback programs capture application values and opaque references;
event payloads are fresh at emission. Captured custom value roots require an
explicit `Environment::value_type` adapter, including when their fields are
used. These ownership adapters currently require `Clone + Send + Sync`; event
parameters themselves do not need Clone/Sync merely to emit or inspect fields.

Capture preserves lazy expression operators and deferred field access. Action
requests contain fresh owned arguments, reordered into descriptor field order.
Generational references are checked again during dispatch. Compatible visual
redraws retain published callbacks; application/presentation changes replace
them. Failed frames never install partial bindings.

Language callback evaluation and action errors retain the source filename and
location; action failures also identify the event. Controlled text changes
propose content and retain existing normalization/rejection, caret, clipboard
and IME behavior. See [text input](../ui/Text%20input.md).

## Compile-time restrictions

```compile_fail
use pixui_engine_macros::component_events;
#[component_events]
trait InvalidEvents {
    fn change(&mut self, content: String) -> pixui_base::PixuiResult<()>;
}
```

```compile_fail
use pixui_engine_macros::component_events;
#[component_events]
trait InvalidEvents {
    fn change(&self, content: &str) -> pixui_base::PixuiResult<()>;
}
```

The design rationale and rejected alternatives are recorded in
[DR-020](../../../../docs/decisions/DR-020%20Declare%20typed%20component%20events%20through%20generated%20trait%20proxies.md).
