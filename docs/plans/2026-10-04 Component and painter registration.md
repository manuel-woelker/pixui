# Component and painter registration plan

Status: proposed. This plan defines the next step beyond the fixed `Widget`
enum and renderer. It introduces no runtime code yet.

## Goal

Register arbitrary components with associated props and state types. Register
painters independently so applications can choose how the same component looks.
Run component preparation and painting on the application worker;
continue sending owned display lists to the existing native GUI host.

## Current implementation

`live_model/component.rs` declares `Component` with `Props` and `State`
associated types, but rendering does not use it. `ButtonComponent` currently
associates both with unit despite defining `ButtonProps` and `ButtonState`.

`ComponentPart` currently holds a state factory and an optional presentation
callback returning `Widget`. The renderer matches label, button, and checkbox
variants to measure, draw, and create hit regions. Adding a component therefore
requires editing this central renderer. Persistent state is erased through
`GenericComponentState` and is currently reconciled by node kind and position.

## Proposed API

Keep `Component` as the typed identity and associated-type declaration:

```rust
pub trait Component: 'static {
    type Props: Send + 'static;
    type State: Default + Send + 'static;
}

pub trait Painter<C: Component>: Send + 'static {
    fn paint(&self, context: &mut PaintContext<'_, C>) -> PixuiResult<()>;
}
```

`PaintContext<'a, C>` provides:

- `props: &'a C::Props` and `state: &'a C::State`.
- `width: f32` and `height: f32` in logical pixels.
- Borrowed `PresentationSettings` and instance focus/hover information.
- Shared text metrics and helpers for emitting owned `DrawCommand`s.

The context appends to the render's `DisplayList`; it does not own application
state or native resources. Helpers should make fill, stroke, text, and balanced
clipping straightforward. Existing clip validation remains the final gate.

Painters draw in local coordinates with origin `(0, 0)`. The renderer clips
each component to its supplied width and height, then translates its commands
to the component's position in the window.

Defer layout and measurement APIs. Initially give every presented component a
constant height of 36 logical pixels and the available viewport width after
outer padding. Keep a simple vertical sequence with fixed spacing. Painters
use the supplied dimensions to position their drawing; content does not change
the allocated height. Overflow is clipped, and zero available width is valid.

Illustrative setup, with names and exact signatures finalized in implementation:

```rust
let button = components.register::<ButtonComponent>("button")?;
painters.register::<ButtonComponent>(ButtonPainter::new())?;
let part = ComponentPart::typed(button, resolve_button_props);
```

`resolve_button_props` reads the expression context and presentation settings
and returns owned `ButtonProps`. State is created automatically with
`C::State::default()` when a physical component is first reached or its type
changes. Registration takes no user-provided state factory. The registry's
erased adapter bridges default initialization to the walker.

## Registration and ownership

- Each application owns its component and painter registries on its worker.
  Configure them before registering UI definitions. Handles transfer owned,
  sendable registration data through the existing command queue.
- Component registration records the concrete component identity, name, props
  type, state type, default initializer, and checked adapters. Key by the
  component type, not its props/state types: two components may share those
  associated types.
- Return an opaque typed `ComponentId<C>` carrying application registry identity
  and an append-only index. Live parts retain the erased identity and adapter;
  validate ownership before using cached indexes.
- Painters are ordinary values implementing `Painter<C>`. Store a checked erased
  adapter internally. State requires `Default`; props do not. No `Reflect`,
  `Clone`, or `Sync` bound is required on props/state, and no `Sync` bound is
  required on painters.
- Painter registration requires a registered component. Reject duplicate
  component types/names and duplicate painters. A missing painter for a rendered
  component is a descriptive error. Validate registered UI definitions early,
  including components inside empty loop bodies.
- Initially register one painter per component per application. Two applications
  can use different painters for the same component type; instances in one
  application share painters and supply their own settings. Per-instance painter
  sets and replacement/removal APIs are deferred.
- Painter values may contain immutable drawing configuration. Their `&self`
  methods should be deterministic for the supplied context. Component state
  belongs to each physical node, rather than to the shared painter.

## Props, state, and rendering lifecycle

1. Walk the live-part tree and reconcile physical component identity as well as
   node kind. Initialize typed state on first reach or component-type
   replacement using `C::State::default()`. Keep state when props change for the
   same physical component; never call `Default` again for an ordinary rerender.
2. Resolve owned typed props once per physical component per render. Borrow
   persistent state during the visit; no props or state cloning is required.
3. Call the registered painter with those props/state and the supplied width
   and constant height. Collect owned local drawing commands and activation
   bindings. No application or physical-state borrows escape the visit.
4. Calculate content height from the component count and fixed spacing, clamp
   scrolling, and translate each component's commands into its row position.
   Apply component and viewport clipping and record hit regions.
5. Publish the existing `RenderOutput` atomically. Discard partial commands and
   geometry on error and preserve the last good output.

Painters borrow state immutably. State changes belong to component
initialization and component update/event behavior; painting must not change
interaction or invoke application actions. Define any initial update hook as a
separate typed callback and execute it once during preparation before painting.

Keep existing action bindings and worker-side hit testing. Initially an
interactive component's hit region is its allocated bounds intersected with the
viewport. Resolve activation bindings separately from the painter, so replacing
appearance preserves behavior. Paint context focus/hover information comes from
the instance's interaction state rather than duplicated component flags.

Preserve `PartState::Unknown`, independent per-loop-item state, and current
positional reconciliation. Typed component replacement must reset state even
when both old and new parts are `LivePart::Component`. Keyed loop reconciliation
remains separate future work.

## Built-in components and painters

- Wire `ButtonComponent` to `ButtonProps` and `ButtonState`; document which
  state is local and which values come from instance interaction.
- Add `LabelComponent` and `CheckboxComponent` with named props/state types.
  A checkbox's application-controlled checked value belongs in props.
- Put typed component definitions and behavior in the engine's component
  modules. Put default painters in a separate painter module hierarchy.
  Components must not depend on concrete painter implementations or register
  them implicitly.
- Offer an explicit helper to register the standard components and painters.
  Applications can instead register custom painters. Keep native command
  rasterization in `pixui-gui::painter`, distinct from component painters that
  generate commands on the worker.
- Migrate the todo GUI from `Widget` callbacks to typed props resolvers and
  independent action bindings. Remove the fixed enum and central drawing
  matches after migration; retain low-level walker behavior for existing tests.

## Implementation steps

- [ ] Finalize typed registration handles, default initialization, and optional
      update hook signatures. Document the one-painter-per-component application
      scope.
- [ ] Implement checked component registration, type erasure, ownership
  validation, and component-type-aware state reconciliation.
- [ ] Implement painter registration and typed paint contexts, command
  helpers, and clear missing-painter/type-mismatch errors.
- [ ] Replace fixed-widget drawing with typed painter calls during traversal.
  Supply width and constant height, then translate and clip local commands into
  fixed-height rows. Add no layout or measurement API.
- [ ] Preserve revision-aware input, activation bindings, and frame publication
  semantics, including rendering failure behavior.
- [ ] Add standard component types and separately registered default painters.
- [ ] Migrate the two-window todo GUI and demonstrate a custom painter for an
  existing component without changing its behavior or the renderer.
- [ ] Update `docs/Architecture.md`, the plain `.drawio` architecture source,
  component/painter API documentation, and a decision record. Generate the
  ignored `.generated.svg` preview using the diagram task.
- [ ] Run `./n check` after every implementation unit.

## Verification

- [ ] Test custom registration and two components sharing props/state types.
- [ ] Test duplicate registration, missing painters in empty loops, foreign
  registration handles, and wrong erased props/state with descriptive errors.
- [ ] Verify props refresh without state reset, per-instance/per-loop
      independence, and state reset on component-type replacement at the same
      position.
- [ ] Verify default initialization and update hooks execute at their documented
      times, and props resolution occurs once per component per render.
- [ ] Test different painters for the same component in separate applications.
  Show custom draw commands while retaining the same activation behavior.
- [ ] Test supplied width and constant height, local-coordinate translation,
      scaling, focus/hover appearance, clipping,
      command order, invalid painter output, and painter errors retaining good
      output.
- [ ] Verify non-cloneable props/state and sendable painters work without
      `Sync`. Add compile-fail examples for mismatched typed props and painter
      registration.
- [ ] Run the todo GUI and retain its shared add/mark actions and independent
  presentation settings, resizing, scrolling, and closure behavior.

## Open questions and limits

- State initialization uses `Default` without props or expression inputs.
  Any synchronization with props belongs in component updates.
- Should a component update hook be part of initial registration, or wait until
  a component actually needs local state transitions? Painters remain read-only
  either way.
- Do applications need multiple painter sets within one application? The initial
  scope permits different applications and settings-aware painters; named sets
  would add selection and validation rules.
- Constant-height rows deliberately clip oversized content. Defer content-based
  sizing, measurement, wrapping policy, and general layout until that work is
  explicitly requested.
- Erased props and local command buffers have a cost. Start with safe adapters
  and measure before adding retained render nodes or state cloning.
- Content invalidation still clears positional focus/hover. Stable item
  identity, arbitrary component children, advanced layout, and complex text
  shaping remain separate work.
