# What problem does this plan solve?

`crates/winit/examples/counter.rs` outlines a `WinitAdapter` API, but the `winit` crate does not yet provide any adapter implementation.
The repository already has:

- an engine entry point
- a component concept and registry
- a draw-command model in `crates/engine/src/draw`
- a standalone `winit` example in `crates/winit/examples/counter_winit_only.rs`

What is missing is the bridge that creates a native window from a component name and renders engine-owned draw commands into that window.

# What should the first adapter slice do?

The first adapter slice should support two core responsibilities:

- create a window using a component name, as suggested by `winit_adapter.create_window("CounterApp")`
- render the component content by executing engine draw commands on a `femtovg` canvas inside a `winit` window

This phase should stay intentionally narrow.
It does not need to solve multi-window orchestration, advanced input routing, retained widget trees, or a complete declarative component runtime.

# What existing code should this plan build on?

The plan should reuse the infrastructure already present in the repository:

- `crates/winit/examples/counter_winit_only.rs` for the basic `winit` + `glutin` + `femtovg` window/render loop
- `crates/engine/src/draw/command.rs` and `crates/engine/src/draw/draw_list.rs` for the draw-command protocol
- `crates/engine/src/component/registry.rs` for component registration direction, if the component-name lookup remains engine-owned

The adapter should avoid duplicating the raw window/bootstrap code from the standalone example as one giant file.
Split the adapter into small files under `crates/winit/src` so the responsibilities stay readable and testable.

# What API shape should the adapter expose?

The adapter should expose a small public entry point in `crates/winit`, centered around a `WinitAdapter` type.

The first public surface should likely include:

- `WinitAdapter::new(&Engine) -> PixuiResult<Self>`
- `WinitAdapter::create_window(&self, component_name: &str) -> PixuiResult<()>`

The adapter should own enough runtime state to:

- remember the engine handle
- resolve a component name into something renderable
- create and manage the `winit` window and GPU/canvas state
- request redraws and execute the current draw list during redraw

The exact threading model should be decided during implementation, but the public API should stay minimal.

# How should component names turn into draw lists?

This is the main architectural seam and should be made explicit in the implementation.

The adapter needs a way to turn `"CounterApp"` into renderable draw content.
The repository does not currently expose a complete engine-side API for:

- registering components by string name
- resolving a component by name at runtime
- producing a `DrawList` for that component

The implementation should therefore add the smallest engine-facing abstraction that makes the adapter viable.
One reasonable first slice is:

1. introduce an engine-side render entry point that accepts a component name and viewport information
2. have that entry point return a `DrawList`
3. let `WinitAdapter` call that API during redraw

If a component-name registry is needed, prefer making it explicit and narrow rather than baking string lookups directly into the adapter.

# How should draw commands be rendered in the adapter?

The adapter should interpret the engine draw-command stream on a `femtovg::Canvas`.

The first renderer should support the command set that already exists in `crates/engine/src/draw/command.rs`:

- `SelectStyle`
- `FillRoundedRectangle`
- `OutlineRoundedRectangle`
- `DrawText`

Implementation should include:

- style tracking for the active style id
- conversion from `DrawStyle`, `Brush`, `Color`, and `TextStyle` into `femtovg` paints
- bounds-aware canvas clearing and sizing
- useful error handling when commands are invalid, such as drawing without an active style

Keep the renderer local to the `winit` crate rather than coupling `engine` directly to `femtovg`.

# What module layout should the work use?

Prefer a small module tree under `crates/winit/src`.
One reasonable starting layout is:

- `crates/winit/src/lib.rs`
- `crates/winit/src/winit_adapter.rs`
- `crates/winit/src/window_runtime.rs`
- `crates/winit/src/draw_list_renderer.rs`
- `crates/winit/src/gl_window.rs`

If extra types are needed for window state, renderer state, or adapter commands, keep the one-item-per-file rule in mind and split them out instead of growing a monolithic adapter file.

# What implementation order should be used?

1. Define the public `WinitAdapter` API and the minimal runtime state it needs.
2. Extract reusable GL/window bootstrap logic from `counter_winit_only.rs` into `crates/winit/src`.
3. Add a `DrawList` renderer in `crates/winit` that can execute the current engine draw commands on a `femtovg` canvas.
4. Add or expose the minimal engine-side API needed to resolve a component name and obtain a `DrawList`.
5. Wire `WinitAdapter::create_window` to create a window, trigger redraws, and render the resolved component content.
6. Update `crates/winit/examples/counter.rs` to use the real adapter instead of the placeholder API.
7. Add verification for the adapter renderer and run repository-wide checks.

# How will progress be tracked?

- [x] Add a `WinitAdapter` public entry point in `crates/winit/src` and expose it from `lib.rs`.
- [x] Extract reusable `winit`/`glutin`/`femtovg` window bootstrap code from the standalone example into adapter-owned modules.
- [x] Add a `DrawList` renderer in `crates/winit` for the existing engine draw-command set.
- [x] Add tests for draw-command rendering helpers where they can be verified without driving a full interactive window.
- [x] Add or expose an engine API that resolves a component name into a `DrawList` for a given viewport.
- [x] Keep the component-name-to-renderable-content mapping explicit instead of hiding string lookups inside the adapter.
- [x] Implement `WinitAdapter::create_window` so it creates a window using the requested component name.
- [x] Render the component content on redraw using engine draw commands executed through the adapter renderer.
- [x] Update `crates/winit/examples/counter.rs` to use the implemented adapter API end-to-end.
- [x] Run focused tests for `pixui-engine` and `pixui-winit`.
- [x] Run `nao check`.

# How should the work be verified?

Verification should mix narrow unit tests with one example-compilation path.

Recommended verification:

- add colocated tests for any draw-command-to-`femtovg` translation helpers that do not require a live OS window
- add tests for any engine-side component-name resolution or draw-list production APIs introduced by this work
- compile `crates/winit/examples/counter.rs` against the real adapter
- run `nao check`

Completed verification:

- `cargo test -p pixui-engine`
- `cargo test -p pixui-winit`
- `cargo test -p pixui-winit --example counter --no-run`
- `nao check`

# What assumptions and risks should stay explicit?

- The implementation now uses an explicit engine-side named component renderer registry that maps a component name to a `DrawList` closure.
- Interactive `winit` behavior is harder to test than pure engine code. Keep logic that can be unit tested outside the live event loop.
- The standalone `counter_winit_only.rs` example is a good bootstrap reference, but copying it wholesale into the adapter would be overengineered junk debt almost immediately.
- Font loading and text rendering currently use pragmatic path-based fallback lookup in the `winit` renderer. That is good enough for this first slice, but it is still a follow-up hotspot if the project wants deterministic font assets.
- This plan assumes a single-window first slice. If multi-window support is needed soon, the runtime state should still be structured so it can grow into that later.

# What follow-up questions remain after implementation?

- Should the named component renderer registry stay as the long-term engine abstraction, or should it later be replaced by a richer component/scene runtime?
- Should `create_window("CounterApp")` keep owning the event loop directly, or should adapter construction and event-loop execution be split once multi-window or embedding scenarios matter?
- Should font loading move to bundled assets or an engine-managed font registry so text rendering becomes deterministic across machines?
