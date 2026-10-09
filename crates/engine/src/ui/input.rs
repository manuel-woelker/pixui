//! Low-level input routed to the instance and revision actually presented.

use super::{
    display_list::RenderRevision, geometry::Point, instance::UiInstanceId,
    presentation::PresentationSettings,
};

/// State shared by keyboard keys and mouse buttons. Both transitions are forwarded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonState {
    Pressed,
    Released,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

/// Logical wheel distance. Line units remain unscaled until application policy
/// translates them; pixel units are logical pixels, not physical screen pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WheelDelta {
    Lines { x: f32, y: f32 },
    Pixels { x: f32, y: f32 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub super_key: bool,
}

/// Logical key meaning. Named keys use platform-independent names such as
/// `"Tab"`, `"Enter"`, `"Space"`, and `"F11"` (the winit NamedKey vocabulary).
/// Character values may contain more than one Unicode scalar value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    Character(String),
    Named(String),
    Dead(Option<char>),
    Unidentified(String),
}

/// Physical key identity, independent of the active keyboard layout. Code names
/// follow winit's KeyCode vocabulary; native codes are opaque diagnostic strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhysicalKey {
    Code(String),
    Unidentified(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyLocation {
    #[default]
    Standard,
    Left,
    Right,
    Numpad,
}

/// Complete keyboard transition, including releases, repeats and produced text.
/// IME commits arrive separately; consumers must not insert both forms as text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyboardEvent {
    pub key: Key,
    pub physical_key: PhysicalKey,
    pub location: KeyLocation,
    pub state: ButtonState,
    pub repeat: bool,
    pub text: Option<String>,
    pub modifiers: Modifiers,
    pub synthetic: bool,
}

impl KeyboardEvent {
    /// Constructs a named key press for programmatic/headless input.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            key: Key::Named(name.into()),
            physical_key: PhysicalKey::Unidentified("unspecified".into()),
            location: KeyLocation::Standard,
            state: ButtonState::Pressed,
            repeat: false,
            text: None,
            modifiers: Modifiers::default(),
            synthetic: false,
        }
    }
}

/// Low-level, backend-independent input. The application interprets activation,
/// navigation and shortcuts. Pointer coordinates are logical pixels in the source
/// window. Unsupported events are preserved at this boundary, currently ignored
/// by default application policy; there is no component event propagation yet.
#[derive(Clone, Debug, PartialEq)]
pub enum UiInput {
    PointerMoved(Point),
    PointerEntered,
    PointerLeft,
    MouseButton {
        button: MouseButton,
        state: ButtonState,
        position: Point,
        modifiers: Modifiers,
    },
    MouseWheel {
        delta: WheelDelta,
        modifiers: Modifiers,
    },
    Keyboard(KeyboardEvent),
    ModifiersChanged(Modifiers),
    Focused(bool),
    ImeEnabled,
    /// Cursor range is in UTF-8 byte offsets, as supplied by the native IME.
    ImePreedit {
        text: String,
        cursor: Option<(usize, usize)>,
    },
    ImeCommit(String),
    ImeDisabled,
}

#[derive(Clone, Debug)]
pub enum UiCommand {
    Input {
        instance: UiInstanceId,
        revision: RenderRevision,
        input: UiInput,
    },
    /// Checked worker-side logical focus request. None clears focus. The target
    /// must belong to this instance's definition and be eligible in its layout.
    Focus {
        instance: UiInstanceId,
        target: Option<super::focus::ComponentInstanceId>,
    },
    Present {
        instance: UiInstanceId,
        settings: PresentationSettings,
    },
    /// Repaint without changing content or positional interaction identity.
    Redraw {
        instance: UiInstanceId,
    },
    /// Visual-only redraw with a host-generated, increasing request ID.
    /// The completed output acknowledges it, even when batching other commands.
    AnimationFrame {
        instance: UiInstanceId,
        request: u64,
    },
    /// Pauses worker preparation, painting and resource finalization while hidden.
    /// Showing the instance requests a fresh render with accumulated changes.
    Visibility {
        instance: UiInstanceId,
        visible: bool,
    },
    /// Successful native presentation feedback. `paint_revision` identifies the
    /// application painting pass, so repeated and diagnostic-only presentations
    /// do not inflate FPS. CPU timings describe the last renderer submission.
    FramePresented {
        instance: UiInstanceId,
        paint_revision: RenderRevision,
        timings: Option<super::performance::RendererTimings>,
        timestamp: std::time::Instant,
    },
    Close {
        instance: UiInstanceId,
    },
}

impl UiCommand {
    pub fn instance(&self) -> UiInstanceId {
        match self {
            Self::Input { instance, .. }
            | Self::Focus { instance, .. }
            | Self::Present { instance, .. }
            | Self::Redraw { instance }
            | Self::AnimationFrame { instance, .. }
            | Self::Visibility { instance, .. }
            | Self::FramePresented { instance, .. }
            | Self::Close { instance } => *instance,
        }
    }

    /// Adjacent motion, presentation and ordinary redraws can replace each other.
    /// Animation frame IDs are preserved for completion acknowledgement.
    /// Discrete input and close ordering are preserved.
    pub fn replaces(&self, earlier: &Self) -> bool {
        self.instance() == earlier.instance()
            && matches!(
                (self, earlier),
                (Self::Present { .. }, Self::Present { .. })
                    | (Self::Redraw { .. }, Self::Redraw { .. })
                    | (
                        Self::Input {
                            input: UiInput::PointerMoved(_),
                            ..
                        },
                        Self::Input {
                            input: UiInput::PointerMoved(_),
                            ..
                        }
                    )
            )
    }
}
