# Controlled text input

`TextInputComponent` is a single-line core component. Register it and its
standard painter with the ordinary standard registration helpers.
`TextInputProps` has one field, `content: String`, which is the sole source of
committed displayed text. Add `ComponentPart::with_change` to make it editable.
Without this binding, the field supports focus, selection and copying but cannot
change content.

## Binding an application value

This executable example registers a named string, a change action and a field:

```rust
use pixui_base::PixuiResult;
use pixui_engine::{
    application::{app::Application, application_slice::ApplicationSlice,
        entity_mut::EntityMut, action::slice_actions},
    components::text_input::TextInputProps,
    expression::expression::Expression,
    live_model::part::ComponentPart,
    ui::definition::UiDefinition,
};

#[slice_actions(slice = "editor", facade = EditorActions)]
mod actions {
    use super::*;
    #[action]
    pub fn change(mut draft: EntityMut<String>, content: String) {
        *draft = content;
    }
}

# fn main() -> PixuiResult<()> {
let application = Application::new();
let mut slice = ApplicationSlice::new("editor");
slice.bind("draft", String::new())?;
let slice = application.add_slice(slice)?;
actions::EditorActions::register(&application, slice)?;
let components = application.register_standard_components()?;
application.register_standard_painters()?;
let draft = application.entity_ref::<String>(slice, "draft")?;
let input = ComponentPart::typed_with_expressions(
    components.text_input,
    vec![Expression::entity(draft)],
    |_, _, values| Ok(TextInputProps {
        content: values[0].downcast_ref::<String>().expect("draft string").clone(),
    }),
).with_change(|context, _| {
    let action = context.application()?.slice_named("editor")?
        .action_handle_named("change")?;
    Ok(Box::new(move |_, content| action.call(vec![Box::new(content)])))
});
application.register_ui(UiDefinition::new("editor", input.into()))?;
# Ok(())
# }
```

The change factory runs during preparation. Its worker-local closure receives
`(&Application, String)` and creates an ordinary owned `ActionCall`. Capture
cached action handles and opaque entity references; never call blocking
`ApplicationHandle` methods from a worker callback.

Each edit proposes the **complete new value** exactly once. No-op edits,
selection, navigation, copying and IME preedit do not call change. Actions may
accept, normalize, reject or ignore a proposal. A render barrier resolves actual
props before the next edit, even when an action mutates data and then returns an
error. Errors are returned through the existing command/action reply path; the
native pending-input adapter reports them without terminating the host.

There is no optimistic text buffer. Accepted values use the proposed caret;
normalization clamps that caret to the returned value's grapheme boundaries.
Rejected or ignored edits retain the old selection. External replacements clamp
existing endpoints without guessing a text diff.

## Identity and ownership

Selection anchor/head are UTF-8 byte offsets at extended grapheme boundaries,
computed with `unicode-segmentation` 1.13.3. `UiDefinitionState::text_inputs`
retains authoritative snapshots, selection, revisions and composition flags by
structural component path within that definition. Keys follow reordered loop
items. Successful preparation of the focus source prunes removed occurrences;
a peer with another active tree cannot erase them. Returning match arms start
fresh. Failed preparation preserves previously published state and geometry.

`UiInstance::layout().text_inputs` owns measured caret stops, clipping and
horizontal scroll. Instances share selection but have their own font scale and
width. `PaintContext::text_edit` is a read-only snapshot for an input painter.
A custom input painter must implement `Painter::text_input_geometry`, supplying
one ordered stop per grapheme boundary and the same font advances it paints.
Invalid geometry rejects preparation.

The standard painter has a bounded natural width of 240 logical pixels and
uses normal layout constraints. It paints chrome, selection, one atlas-backed
text command and a caret. Horizontal scroll reveals the caret. Blink uses master
time, with a 500 ms deadline only for a collapsed selection in the active source
window. Frozen presentation timestamps disable blink scheduling; blurred,
hidden and unfocused inputs do not keep rendering for blinking.

## Keyboard, pointer and sessions

- Produced keyboard text and IME commits replace the selection.
- Left/Right move by grapheme; Shift extends selection. Without Shift, arrows
  collapse a selection toward the corresponding edge.
- Home/End move to the ends; Backspace/Delete remove a selection or one
  grapheme.
- Control is the primary shortcut modifier on Linux/Windows; Command on macOS.
  A/C/X/V select all, copy, cut and paste. Word movement/deletion uses Control
  on Linux/Windows and Option on macOS. Command+Left/Right are macOS end
  equivalents.
- Enter is consumed; submission, multiline editing and undo/redo are deferred.
  Tab/Shift+Tab and F11 retain normal focus-navigation/diagnostics behavior.
- Repeated typing, movement and deletion work. Clipboard shortcuts do not
  repeat. Synthetic text and releases are ignored. Logical key names never
  synthesize insertion text; produced text supports alternate layouts/AltGr.
- Press chooses the nearest grapheme stop; Shift-click extends the anchor.
  Drag capture remains attached to that field outside its bounds. Release keeps
  focus. Capture cancels on blur, hide, close, target removal, resizing or an
  application action. Native pointer grabs are best effort; platform implicit
  capture can supply outside-window events where explicit grabs are unsupported.

`WindowCommand::SetTextInput` publishes latest-value metadata with an opaque
`EditingSessionId`, editability, candidate caret rectangle and capture request.
Hosts send keyboard/IME and captured selection events with
`UiCommand::TextInput` and the session token. The session belongs to one stable
target and source instance. Content edits retain it; focus
transfer/removal/blur/hide ends it. Native focus in a peer transfers editing
ownership without changing shared logical focus. Ordinary button/pointer input
still requires compatible presented geometry.

For ordered click-then-type and Tab-then-type batches, the native host clears
its keyboard token locally. An initial tokenless event is accepted only against
the new session's originating presented revision. This avoids blocking/buffering
on a worker reply while keeping the event attached to its serialized focus
request. Programmatic focus changes create another token; previous tokens cannot
retarget that input. Captured motion uses the gesture's session while its own
horizontal scroll changes; unrelated presentation/content invalidation cancels
the gesture.

## Clipboard and composition

`OutputReceiver::effects()` carries an ordered bounded queue of clipboard reads
and writes. Attach a host/test adapter with `UiCommand::HostAttached`; headless
instances default to clipboard unavailable. Replies use `UiCommand::Clipboard`
with request ID and originating instance. Pending requests are bounded to 16
across the worker; each instance's effect queue holds at most 16.

The GUI's injectable `ClipboardBackend` uses arboard 3.6.1 for plain text, with
Wayland data-control enabled in addition to X11 and native Windows/macOS
support. An executor thread keeps clipboard I/O off the application and GUI
threads, retains native clipboard ownership, and processes requests in order.
Its request and reply queues each hold 16. Saturation and OS failures return
error replies. Actual clipboard availability depends on the platform/session;
Wayland support requires the corresponding compositor clipboard protocol.

A paste/cut captures source session, target, content revision and selection
revision. A delayed result is discarded after any of these change. Cut proposes
deletion only after a successful clipboard write. Copy never invokes change.
Non-text clipboard data produces no edit. Clipboard library reads may allocate
the OS string before the size check.

Native IME is enabled only for an editable input in the active source, and its
candidate rectangle follows current instance geometry. Preedit changes
composition state without changing or replacing rendered props. Ordinary
keyboard insertion is suppressed during composition; winit supplies commit text
separately. Only a commit proposes content. Focus/session changes and external
replacement cancel composition; the native IME token stays separate from a
pending keyboard focus handshake so old commits cannot target a new field.

## Bounds and limits

Committed props and individual insertions are limited to 1 MiB of UTF-8. Pasted
or committed CR/LF/tab runs become one space; other control characters are
discarded. Programmatic props with controls fail preparation rather than
silently painting another committed value. Oversized data is rejected, never
truncated.

Editing is grapheme-safe, but text is currently unshaped, left-to-right and
unkerned, with the same glyph fallback as ordinary text commands. Combining
marks and complex scripts may render imperfectly; visual correctness does not
imply bidi, shaping, accessibility, inline preedit, rich text, password masking,
primary-selection clipboard or double/triple-click selection.

See the showcase's text page for acceptance, uppercase normalization and
rejected longer values, and the todo example for an entity-bound entry field.
