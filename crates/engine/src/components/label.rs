use crate::live_model::component::Component;

pub struct LabelComponent;
pub struct LabelProps {
    pub text: String,
}
#[derive(Default)]
pub struct LabelState;
impl Component for LabelComponent {
    type Events = crate::events::interface::NoEvents;
    type Props = LabelProps;
    type State = LabelState;
}
