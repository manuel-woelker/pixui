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
    type Props = CheckboxProps;
    type State = CheckboxState;
}
