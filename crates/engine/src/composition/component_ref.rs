//! Application-local reference to a reusable subtree, not a mounted component.
use pixui_reflect::{Reflect, TypeDescriptor};
/// Opaque identity. Copies share a definition; each rendered occurrence owns its
/// own state. Valid until application shutdown; no constructor or mutable fields
/// are reflected. A reference from another application is rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ComponentRef {
    pub(crate) owner: u64,
    pub(crate) index: usize,
}
impl Reflect for ComponentRef {
    fn type_descriptor() -> &'static TypeDescriptor {
        static DESCRIPTOR: std::sync::OnceLock<TypeDescriptor> = std::sync::OnceLock::new();
        DESCRIPTOR
            .get_or_init(|| TypeDescriptor::new::<Self>(vec![], vec![]).expect("opaque reference"))
    }
}
