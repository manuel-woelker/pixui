# Typed components and painters

A component declares identity, owned props, and persistent state. State
implements `Default`; props and state implement `Send + 'static`. Neither needs
`Clone`, `Sync`, or reflection. Two component types can share associated types
and still have independent identities.

Register components and then painters before registering UI definitions:

```rust
# use pixui_base::PixuiResult;
# use pixui_engine::{application::app::Application,
#     components::button::{ButtonComponent, ButtonProps},
#     expression::context::ExpressionContext,
#     live_model::part::ComponentPart, painters::button::ButtonPainter,
#     ui::presentation::PresentationSettings};
# fn resolve_props(_: &ExpressionContext<'_>, _: &PresentationSettings) -> PixuiResult<ButtonProps> {
#     Ok(ButtonProps { label: "Save".into() })
# }
# fn main() -> PixuiResult<()> {
let app = Application::new();
let button = app.register_component::<ButtonComponent>("button")?;
app.register_painter::<ButtonComponent>(ButtonPainter)?;
let part = ComponentPart::typed(button, resolve_props);
# Ok(())
# }
```

The resolver is an ordinary function taking `&ExpressionContext` and
`&PresentationSettings`, returning `PixuiResult<ButtonProps>`. It runs exactly
once per physical component per render. `ComponentPart::typed_with_update`
additionally accepts a function taking `&Props` and `&mut State`, returning
`PixuiResult<()>`. Updates run once after resolving props and before painting.
All components finish preparation before any painter runs; physical state is
then reborrowed in tree order without another expression walk or state cloning.

`ComponentId<C>` contains a registry identity and append-only index. It is
copyable without placing extra bounds on `C`. Component names and concrete types
must be unique per application. Painter registration requires a registered
component and rejects duplicates. UI registration validates ownership and
painter availability throughout the template, including empty loop bodies.
Erased adapters check types before borrowing; errors describe the expected type.

## State and ownership

The walker creates state using `State::default()` at first reach. It preserves
state for the same registered component at the same physical position and resets
it on component identity changes. Each instance and loop element owns its state.
Loop reconciliation is positional; reordered items can inherit positional state.

The worker owns both registries and all physical state. The application handle
still contains only its cloneable sender. Props resolvers and update callbacks
are function pointers; activation factories can return owned capturing
callbacks. No application or component borrows leave the worker.

## Painting

Implement `Painter<C>` with `paint(&self, &mut PaintContext<'_, C>)`. The
painter needs `Send + 'static`, but not `Sync`. The context exposes immutable
props/state, settings, focus/hover, logical pixel width/height, and a shared
`u64` microsecond `timestamp_us`. The application clock is sampled once per
render; settings can override time for deterministic drawing or seeking. Helpers
append fill, stroke, text, and clip commands. `with_clip` balances its own clip
on ordinary errors. Directly emitted clips must also balance within the
component.

Draw relative to `(0, 0)`. Every component currently receives 36 logical pixels
of height and the viewport width minus 32 pixels of outer padding, clamped to
zero. Rows have 8 pixels of spacing. The renderer clamps scrolling after
preparation. Paint contexts translate commands as they enter the shared builder
and enforce local clip balance; the renderer clips each component and the
viewport. There is no measurement or general layout API; oversized content is
clipped.

Painting must preserve interaction and should be deterministic for its inputs.
Use the separate update callback for local transitions. Attach actions through
`ComponentPart::with_activation`; painters do not invoke actions. Interactive
hitboxes are the allocated row intersected with the viewport.

One painter per component per application is supported. Different applications
can register different painters; instances in one application share painters.
Explicit standard-component and standard-painter helpers are independent, so
customizing a standard component does not require modifying the renderer.

Rendering errors discard partial commands and geometry and preserve the last
published revision. Initialization and updates already executed are not rolled
back. Panics follow the application worker's panic policy.

See the [image and animation guide](../ui/Images.md) for snapshot ownership,
color-key transparency, and painter-requested redraw scheduling.
