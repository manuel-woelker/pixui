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
Before the binding update, `Component::prepare` can resolve component-owned
state such as image resources. Its default implementation does nothing. All
components finish preparation before any painter runs; physical state is
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

Draw relative to the allocated content box's `(0, 0)`. `Painter::measure`
reports intrinsic content size through a read-only `MeasureContext`; convenient
text metrics share painting's font and scale policy. Measurement may run many
times and must not mutate state, emit commands, schedule work or perform I/O.
Preparation and updates still run once per physical component.

Definitions use explicit Flex/Grid containers and constant border-box styles.
The layout solver assigns final per-instance dimensions; padding reduces the
content box supplied to painting. Containers clip by default. See the
[layout guide](../layout/README.md) for composition, sizing and scroll
semantics. Paint contexts translate commands into the shared builder and enforce
local clip balance. Errors discard partial geometry and commands.

Painting must preserve interaction and should be deterministic for its inputs.
Use the separate update callback for local transitions. Attach actions through
`ComponentPart::with_activation`; painters do not invoke actions. Interactive
hitboxes are allocated border boxes intersected with ancestor clips and the
viewport. Keyboard targets are separate and include offscreen controls.

One painter per component per application is supported. Different applications
can register different painters; instances in one application share painters.
Explicit standard-component and standard-painter helpers are independent, so
customizing a standard component does not require modifying the renderer.

Rendering errors discard partial commands and geometry and preserve the last
published revision. Initialization and updates already executed are not rolled
back. Panics follow the application worker's panic policy.

See the [image and animation guide](../ui/Images.md) for snapshot ownership,
color-key transparency, and painter-requested redraw scheduling.

## Image component

`components::image::ImageComponent` is registered by the standard helpers. Its
`ImageProps` holds a validated relative `ResourcePath` (`ImageProps::new`
accepts a filename). Configure `Application::set_image_loader` or its handle
equivalent before rendering. Preparation resolves the path into `ImageState`;
the standard `ImagePainter` centers the snapshot with aspect-preserving scaling.
Custom painters can read `ImageState::image()` without doing filesystem I/O.

The worker's weak path cache shares live snapshots across nodes and windows.
Changing props resolves the new path; replacing the application loader clears
lookup and invalidates existing UIs. Failed loads abort frame publication,
retaining the previous output. See [resource loading](../resources/README.md).
