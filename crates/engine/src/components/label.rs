use crate::component::Component;
use facet::Facet;

pub struct LabelComponent {}

#[derive(Facet)]
pub struct LabelProps {
    label: String,
}

impl Component for LabelComponent {
    type Properties = LabelProps;
}
