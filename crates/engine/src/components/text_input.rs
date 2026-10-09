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
