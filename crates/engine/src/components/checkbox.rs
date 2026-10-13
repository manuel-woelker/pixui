use crate::live_model::component::Component;

pub struct CheckboxComponent;
/// Checked reflects application data; painters do not mutate it on activation.
pub struct CheckboxProps {
    pub label: String,
    pub checked: bool,
}
#[derive(Default)]
pub struct CheckboxState;
impl Component for CheckboxComponent {
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
    type Props = CheckboxProps;
    type State = CheckboxState;
}
