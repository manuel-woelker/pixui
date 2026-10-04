use crate::live_model::component::Component;

pub struct LabelComponent;
pub struct LabelProps {
    pub text: String,
}
#[derive(Default)]
pub struct LabelState;
impl Component for LabelComponent {
    type Props = LabelProps;
    type State = LabelState;
}
