use crate::component::Component;
use facet::Facet;

pub struct ButtonComponent {}

#[derive(Facet)]
pub struct ButtonProps {
    label: String,
}

impl Component for ButtonComponent {
    type Properties = ButtonProps;
}
