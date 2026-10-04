//! Typed identity of a component, independent of its painter implementation.

/// Props are freshly resolved per render. State is default-initialized once per
/// physical node and retained across renders. Both remain on the worker, so
/// `Sync`, `Clone`, and reflection are unnecessary.
pub trait Component: 'static {
    type Props: Send + 'static;
    type State: Default + Send + 'static;
}
