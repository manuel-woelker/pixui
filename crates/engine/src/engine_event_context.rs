/// Mutable context passed to an engine event handler.
pub struct EngineEventContext<E> {
    /// Event being handled.
    pub event: E,
}
