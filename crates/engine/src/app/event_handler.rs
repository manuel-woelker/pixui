use pixui_base::result::PixuiResult;

pub struct ApplicationEventContext<E> {
    pub event: E,
}

pub trait ApplicationEventHandler: Send {
    type Event;
    fn handle_event(
        &mut self,
        context: &mut ApplicationEventContext<Self::Event>,
    ) -> PixuiResult<()>;
}
