//! Explicitly declared host text, resolved separately for each presentation.
use pixui_reflect::{Reflect, TypeDescriptor};
/// A parameterless message in the application's existing translation registry.
/// Strings remain ordinary application data; this type opts into localization.
#[derive(Clone)]
pub struct LocalizedText {
    pub(crate) expression: crate::expression::expression::Expression,
    pub(crate) domain: String,
}
impl Reflect for LocalizedText {
    fn type_descriptor() -> &'static TypeDescriptor {
        static DESCRIPTOR: std::sync::OnceLock<TypeDescriptor> = std::sync::OnceLock::new();
        DESCRIPTOR.get_or_init(|| {
            TypeDescriptor::new::<Self>(vec![], vec![]).expect("opaque localized text")
        })
    }
}
