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

Choose a component category in the left pane; the right pane shows only that
example. The title spans both columns. The selected category is shared across
both windows. Switching pages preserves counters, flags, and rows.

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

Layout uses a header above a two-column Grid: a 200-pixel navigation pane and
a flexible detail pane. Both panes align at the top. Navigation buttons fill the
fixed sidebar in every language. Usage hints sit in a full-width footer at the
bottom of the window; overflowing content remains scrollable.
Tests cover actions, keyboard activation, conditional content, resource sharing,
and synchronized window outputs. Native appearance needs manual verification.

## Translation workflow

Messages are declared in the definition's `showcase` domain. German text is
loaded from [de.po](translations/showcase/de.po), rather than locale branches in
Rust. The selected language index is independent of locale; startup uses
`presentation_language` to set both.

Export all messages, including conditional labels and the native title, without
opening windows or loading image/font resources:

```bash
./t cargo run -p pixui-example-showcase -- --export-translations /tmp/showcase.pot
```

Translate a copy with a PO editor or Weblate, preserving `{count}` placeholders.
Catalogs can be replaced on the worker with `install_translations`; switch a
live instance by sending `UiCommand::Present` with new indexed language
settings. Missing translations fall back to source text. Plurals and
locale-aware number formatting are not implemented.

## Hot reload

The example enables native directory watching by default. To disable it:

```bash
./t cargo run -p pixui-example-showcase -- --no-hot-reload
```

Edit `assets/images/pixui-logo.png` or this example's German PO file while the
windows are open. Images (including native icons) and translated text/titles
update after a short quiet period. Invalid/incomplete files keep the last good
version until a valid save. With `--no-hot-reload`, catalogs use the embedded
baseline. `--hot-reload` explicitly enables watching again.
Translation export never starts a watcher, regardless of these flags.
See [resource hot reload](../../crates/engine/src/resources/reload/README.md).

## Layout

The example uses explicit layout containers and painter intrinsic measurement.
Images have bounded logical sizes; window resizing recomputes independent
geometry. Containers clip by default. See the
[layout guide](../../crates/engine/src/layout/README.md). The header and footer
span both panes. Counter actions use a nested Flex row; images are shown only on
the Images page.
