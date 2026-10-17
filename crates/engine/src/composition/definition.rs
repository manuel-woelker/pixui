//! Checked reusable source definitions. Definitions share code, never mounted state.
use super::component_ref::ComponentRef;
use crate::{
    events::value::ValueType, live_model::part::LivePart, ui_language::source::origin::Origin,
};
use pixui_base::{PixuiResult, pixui_error};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
#[derive(Clone)]
pub(crate) struct Prop {
    pub name: String,
    pub ty: ValueType,
    pub origin: Origin,
}
#[derive(Clone)]
/// Opaque checked source definition. Callers use CompositeExport to import it.
pub struct CompositeDefinition {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) props: Vec<Prop>,
    pub(crate) children: bool,
    pub(crate) body: LivePart,
    pub(crate) origin: Origin,
}
impl CompositeDefinition {
    pub fn next_id() -> u64 {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        NEXT.try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("composite identities exhausted")
    }
}
/// Named source export, including the checked signature needed by static calls.
/// Unlike a leaf ComponentId, it describes reusable structure with props/children.
/// Bundle installation additionally supplies an application-local ComponentRef.
#[derive(Clone)]
pub struct CompositeExport {
    pub(crate) definition: Arc<CompositeDefinition>,
    pub(crate) reference: Option<ComponentRef>,
}
impl CompositeExport {
    pub fn name(&self) -> &str {
        &self.definition.name
    }
    /// Only installed exports have application references. Dynamic currently
    /// accepts zero-argument definitions; required props/slots produce an error.
    pub fn reference(&self) -> PixuiResult<ComponentRef> {
        self.reference.ok_or_else(|| {
            pixui_error!("composite export must be installed before obtaining a ComponentRef")
        })
    }
}
/// A checked static call. Its props are evaluated in the caller, once per walk.
#[derive(Clone)]
pub struct InvocationPart {
    pub(crate) definition: Arc<CompositeDefinition>,
    pub(crate) values: Vec<(crate::expression::expression::Expression, ValueType)>,
    pub(crate) children: Box<LivePart>,
    pub(crate) site: u64,
    pub(crate) origin: Origin,
}
