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
    Close {
        instance: UiInstanceId,
    },
}

impl UiCommand {
    pub fn instance(&self) -> UiInstanceId {
        match self {
            Self::Input { instance, .. }
            | Self::Present { instance, .. }
            | Self::Redraw { instance }
            | Self::AnimationFrame { instance, .. }
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
