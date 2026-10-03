use crate::live_model::component::Component;

pub struct ButtonComponent {

}

pub struct ButtonProps {
    pub label: String,
}

pub struct ButtonState {
    pub active: bool,
}


impl Component for ButtonComponent {
    type Props = ();
    type State = ();
}