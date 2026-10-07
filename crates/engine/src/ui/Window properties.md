# Window properties

Window titles and icons are application output, separate from drawing.
`UiDefinition::with_window_properties` accepts a resolver receiving application
data and per-instance presentation settings on the worker. It returns
`WindowProperties` with a `PixuiString` title and optional `ResourcePath` icon.

```rust,no_run
use pixui_base::PixuiResult;
use pixui_engine::{
    expression::context::ExpressionContext,
    live_model::part::{CompositePart, LivePart},
    resources::path::ResourcePath,
    ui::{definition::UiDefinition, presentation::PresentationSettings,
         window_properties::WindowProperties},
};
fn properties(_: &ExpressionContext<'_>, _: &PresentationSettings)
    -> PixuiResult<WindowProperties>
{
    Ok(WindowProperties {
        title: "My application".into(),
        icon: Some(ResourcePath::new("images/pixui-logo.png")?),
    })
}
let definition = UiDefinition::new("main", LivePart::Composite(CompositePart {
    parts: vec![],
})).with_window_properties(properties);
```

Configure the application's image loader before resolving an icon. Icon paths
use the same layered source and weak image cache as core image components. Live
snapshots can be shared between a displayed image and native icon.

## Change detection and delivery

Each `UiInstance` retains its last successfully resolved properties. The worker
compares titles by value and icons by snapshot identity, sending only changed
properties. The first successful resolution sends both, including
`SetIcon(None)` to explicitly clear an icon. Definitions without a resolver send
no commands, preserving host startup defaults.

`OutputReceiver::window_commands()` provides a separate `WindowCommandReceiver`.
Its commands are `SetTitle(PixuiString)` and `SetIcon(Option<Image>)`. The
mailbox has one pending slot per property: newer titles replace pending titles
without losing pending icon updates, and vice versa. No acknowledgements are
necessary. Ordered events such as request-focus do not belong in this
latest-value mailbox. Closing an instance disconnects delivery after pending
commands are drained.

`OutputReceiver::set_waker` attaches the existing event-loop notification to
both mailboxes. Callbacks run outside mailbox locks; installation and
disconnection also notify. Notifications can be coalesced. The GUI applies
metadata even when a window is hidden, without painting or requiring a rendered
frame. Only changed icons need RGBA conversion.

Content and presentation invalidations reevaluate properties, as do explicit
redraws and shared interaction changes. Animation-only and diagnostic refreshes
do not. Hidden instances still process metadata while skipping tree preparation,
painting and display-list finalization. Failed resolution/loading preserves all
previous properties and publishes no partial update; inspect
`UiInstance::window_properties_error()` separately from rendering errors. A
later invalidation retries. Replacing the image loader invalidates properties
too.

## Native support

The host uses winit's native title and window-icon setters. Runtime window icons
are unsupported on Wayland and macOS in the pinned winit version, along with
mobile/web platforms. Those systems need application-icon integration (such as
desktop entries or application bundles). Receiving an icon command does not
guarantee it appears in native decorations. This implementation does not install
desktop entries or bundle application icons.

The todo UI publishes localized titles such as `Todos — 2 open` and
`Aufgaben — 2 offen`, and uses `images/pixui-logo.png` for its icon.
Completed-item filtering and animation toggles do not change the open count.
