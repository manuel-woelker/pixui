# Widget showcase

A standalone gallery of PixUI features, with one shared UI definition displayed
in English/light and German/dark windows.

Run from the repository root:

```bash
./t cargo run -p pixui-example-showcase
./t cargo run -p pixui-example-showcase -- --renderer software
./t cargo run -p pixui-example-showcase -- --renderer femtovg
```

The default renderer selection is `auto`. Explicit `femtovg` selection reports
GPU initialization errors instead of falling back to software.

## Try it

- **Buttons:** increment and reset a shared counter. Both windows and their
  native titles update together.
- **Checkboxes:** toggle an independent boolean and show/hide an explanatory
  label. Removing conditional content closes the gap in the component tree.
- **Collections:** add sample rows at runtime. A live loop renders each entry.
  Add enough rows to try mouse-wheel scrolling.
- **Images:** the core image component loads the repository logo through the
  filesystem abstraction. The native window icon shares the same snapshot.
- **Presentation:** compare themes and localized widget labels side by side.
  Sample row values are shared application data and are not translated.
- **Interaction:** hover over controls, use Tab/Shift+Tab to move focus, and
  Enter/Space to activate. Hover, focus, and scroll state are shared.
- **Diagnostics:** press F11 for the performance overlay. The gallery is idle
  when unchanged; it does not request continuous animation.

Runtime window icons on Wayland require compositor support for
`xdg_toplevel_icon_v1`. macOS needs separate application-icon integration.

## Code map

- [model.rs](src/model.rs): named entities, collection, typed actions, generated
  action facade.
- [gallery.rs](src/gallery.rs): live parts, props, activation bindings,
  conditional rendering, and native window properties.
- [setup.rs](src/setup.rs): component/painter and filesystem registration.
- [main.rs](src/main.rs): native windows and renderer selection.
- [gallery tests](tests/gallery.rs): real worker rendering and input dispatch
  without native windows.

Layout currently uses the engine's constant-height vertical rows. The image
fits within one row; this example does not introduce a separate layout system.
Tests cover actions, keyboard activation, conditional content, resource sharing,
and synchronized window outputs. Native appearance needs manual verification.
