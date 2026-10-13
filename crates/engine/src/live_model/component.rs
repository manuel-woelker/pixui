//! Typed identity of a component, independent of its painter implementation.

/// Props are freshly resolved per render. State is default-initialized once per
/// physical node and retained across renders. Both remain on the worker, so
/// `Sync`, `Clone`, and reflection are unnecessary.
pub trait Component: 'static {
    const INPUT_INTEREST: crate::events::interface::InputInterest =
        crate::events::interface::InputInterest::NONE;
    type Events: crate::events::interface::EventInterface + ?Sized;
    type Props: Send + 'static;
    type State: Default + Send + 'static;

    /// Targeted worker input. Return true to consume the input. Events are queued
    /// until this hook releases its state borrow; painters never call this hook.
    fn handle_input(
        _props: &Self::Props,
        _state: &mut Self::State,
        _input: &crate::events::interface::ComponentInput,
        _events: &Self::Events,
    ) -> pixui_base::PixuiResult<bool> {
        Ok(false)
    }

    /// Resolves component-owned state after props resolution and before the
    /// binding update or any painter runs. Called once per node per render.
    /// Resource components may load here; errors abort frame publication.
    /// Successful state changes are not rolled back if a later component fails.
    fn prepare(
        _props: &Self::Props,
        _state: &mut Self::State,
        _context: &crate::expression::context::ExpressionContext<'_>,
    ) -> pixui_base::PixuiResult<()> {
        Ok(())
    }
}
