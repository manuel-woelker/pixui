//! Typed identity of a component, independent of its painter implementation.

/// Props are freshly resolved per render. State is default-initialized once per
/// physical node and retained across renders. Both remain on the worker, so
/// `Sync`, `Clone`, and reflection are unnecessary.
pub trait Component: 'static {
    type Props: Send + 'static;
    type State: Default + Send + 'static;

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
