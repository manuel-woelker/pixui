//! Typed component preparation with field-ordered reflected construction.
use crate::{
    component_registry::{binding::ErasedBinding, component_id::ComponentAddress},
    expression::context::ExpressionContext,
    live_model::{component::Component, state::GenericComponentState},
    ui::presentation::PresentationSettings,
    ui_language::resolution::environment::ValueType,
};
use pixui_base::{PixuiResult, pixui_error};
use pixui_reflect::{DynamicObject, Reflect};
use std::{any::Any, marker::PhantomData};
pub(crate) struct LanguageBinding<C: Component> {
    pub address: ComponentAddress,
    pub types: Vec<ValueType>,
    pub marker: PhantomData<fn() -> C>,
}
impl<C: Component> ErasedBinding for LanguageBinding<C>
where
    C::Props: Reflect,
{
    fn address(&self) -> ComponentAddress {
        self.address
    }
    fn prepare(
        &self,
        context: &ExpressionContext<'_>,
        _settings: &PresentationSettings,
        state: &mut GenericComponentState,
        values: &[DynamicObject<'_>],
    ) -> PixuiResult<Box<dyn Any + Send>> {
        let fields = values
            .iter()
            .zip(&self.types)
            .map(|(value, ty)| {
                let snapshot = (ty.snapshot)(value)?;
                (ty.send)(snapshot.as_ref())
            })
            .collect::<PixuiResult<Vec<_>>>()?;
        let props = C::Props::type_descriptor()
            .construct_send(fields)?
            .downcast::<C::Props>()
            .map_err(|_| pixui_error!("incorrect reflected props constructor result"))?;
        let state = state
            .downcast_mut::<C::State>()
            .ok_or_else(|| pixui_error!("incorrect component state type"))?;
        C::prepare(&props, state, context)?;
        Ok(props)
    }
}

pub(crate) struct LocatedBinding {
    pub inner: std::sync::Arc<dyn ErasedBinding>,
    pub origin: crate::ui_language::source::origin::Origin,
}
impl ErasedBinding for LocatedBinding {
    fn address(&self) -> ComponentAddress {
        self.inner.address()
    }
    fn prepare(
        &self,
        context: &ExpressionContext<'_>,
        settings: &PresentationSettings,
        state: &mut GenericComponentState,
        values: &[DynamicObject<'_>],
    ) -> PixuiResult<Box<dyn Any + Send>> {
        self.inner
            .prepare(context, settings, state, values)
            .map_err(|error| self.origin.error(error))
    }
}
