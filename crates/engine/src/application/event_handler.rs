pub struct ApplicationEventContext<E> {
    pub event: E,
}

pub trait ApplicationEventHandler {
    type Event;
    fn handle_event(&mut self, context: &mut ApplicationEventContext<Self::Event>);
}
