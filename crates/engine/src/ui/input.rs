//! Semantic input routed to the instance and revision actually presented.

use super::{
    display_list::RenderRevision, geometry::Point, instance::UiInstanceId,
    presentation::PresentationSettings,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UiInput {
    PointerMoved(Point),
    Activate(Point),
    Scroll(f32),
    FocusNext,
    ActivateFocused,
}

#[derive(Clone, Debug)]
pub enum UiCommand {
    Input {
        instance: UiInstanceId,
        revision: RenderRevision,
        input: UiInput,
    },
    Present {
        instance: UiInstanceId,
        settings: PresentationSettings,
    },
    Close {
        instance: UiInstanceId,
    },
}

impl UiCommand {
    pub fn instance(&self) -> UiInstanceId {
        match self {
            Self::Input { instance, .. }
            | Self::Present { instance, .. }
            | Self::Close { instance } => *instance,
        }
    }

    /// Only adjacent motion or presentation messages can replace each other.
    /// Discrete input and close ordering are preserved.
    pub fn replaces(&self, earlier: &Self) -> bool {
        self.instance() == earlier.instance()
            && matches!(
                (self, earlier),
                (Self::Present { .. }, Self::Present { .. })
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
