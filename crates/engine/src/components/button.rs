use crate::live_model::component::Component;

pub struct ButtonComponent;
pub struct ButtonProps {
    pub label: String,
}
/// Local active styling, set by an optional component update. Native focus and
/// hover come from shared definition interaction supplied by PaintContext.
#[derive(Default)]
pub struct ButtonState {
    pub active: bool,
}
impl Component for ButtonComponent {
    const INPUT_INTEREST: crate::events::interface::InputInterest =
        crate::events::interface::InputInterest::ACTIVATION;
    type Events = dyn crate::events::standard::ActivationEvents;
    fn handle_input(
        _props: &Self::Props,
        _state: &mut Self::State,
        input: &crate::events::interface::ComponentInput,
        events: &Self::Events,
    ) -> pixui_base::PixuiResult<bool> {
        if matches!(input, crate::events::interface::ComponentInput::Activate) {
            events.activate()?;
            return Ok(true);
        }
        Ok(false)
    }
    type Props = ButtonProps;
    type State = ButtonState;
}
