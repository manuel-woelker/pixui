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
    type Props = ButtonProps;
    type State = ButtonState;
}
