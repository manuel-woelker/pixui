#![doc = include_str!("../ui/Text input.md")]
use crate::{live_model::component::Component, ui::text_input::editing::validate};
/// Core single-line field; automatic focus is sequential, independently of activation.
pub struct TextInputComponent;
/// Authoritative content. Controls and content above 1 MiB reject preparation.
pub struct TextInputProps {
    /// The displayed committed value; edits only propose replacements through change.
    pub content: String,
}
/// Per-instance component state is empty: selection belongs to the shared definition.
#[derive(Default)]
pub struct TextInputState;
impl Component for TextInputComponent {
    const INPUT_INTEREST: crate::events::interface::InputInterest =
        crate::events::interface::InputInterest {
            keyboard: true,
            ..crate::events::interface::InputInterest::NONE
        };
    type Events = dyn crate::events::standard::TextInputEvents;
    fn handle_input(
        _props: &Self::Props,
        _state: &mut Self::State,
        input: &crate::events::interface::ComponentInput,
        events: &Self::Events,
    ) -> pixui_base::PixuiResult<bool> {
        if let crate::events::interface::ComponentInput::Raw(crate::ui::input::UiInput::Keyboard(
            event,
        )) = input
            && event.state == crate::ui::input::ButtonState::Pressed
            && !event.repeat
            && !event.synthetic
            && matches!(&event.key, crate::ui::input::Key::Named(name) if name == "Enter")
        {
            events.submit()?;
            return Ok(true);
        }
        Ok(false)
    }

    type Props = TextInputProps;
    type State = TextInputState;
    fn prepare(
        props: &Self::Props,
        _: &mut Self::State,
        _: &crate::expression::context::ExpressionContext<'_>,
    ) -> pixui_base::PixuiResult<()> {
        validate(&props.content)
    }
}
