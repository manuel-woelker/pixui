//! Typed props resolution and updates erased only at the heterogeneous tree boundary.

use super::component_id::ComponentAddress;
use crate::{
    expression::context::ExpressionContext,
    live_model::{component::Component, state::GenericComponentState},
    ui::presentation::PresentationSettings,
};
use pixui_base::{PixuiResult, pixui_error};
use pixui_reflect::DynamicObject;
use std::{any::Any, marker::PhantomData};

pub type PropsResolver<C> = for<'a> fn(
    &ExpressionContext<'a>,
    &PresentationSettings,
) -> PixuiResult<<C as Component>::Props>;
/// Explicit declaration values evaluated before the typed props callback.
pub type ExpressionPropsResolver<C> = for<'a> fn(
    &ExpressionContext<'a>,
    &PresentationSettings,
    &[DynamicObject<'a>],
) -> PixuiResult<<C as Component>::Props>;

pub type ComponentUpdate<C> =
    fn(&<C as Component>::Props, &mut <C as Component>::State) -> PixuiResult<()>;

pub(crate) trait ErasedBinding: Send + Sync {
    fn address(&self) -> ComponentAddress;
    fn prepare(
        &self,
        context: &ExpressionContext<'_>,
        settings: &PresentationSettings,
        state: &mut GenericComponentState,
        values: &[DynamicObject<'_>],
    ) -> PixuiResult<Box<dyn Any + Send>>;
}

pub(crate) struct Binding<C: Component> {
    pub address: ComponentAddress,
    pub resolve: PropsResolver<C>,
    pub update: ComponentUpdate<C>,
    pub marker: PhantomData<fn() -> C>,
}

impl<C: Component> ErasedBinding for Binding<C> {
    fn address(&self) -> ComponentAddress {
        self.address
    }
    fn prepare(
        &self,
        context: &ExpressionContext<'_>,
        settings: &PresentationSettings,
        state: &mut GenericComponentState,
        _values: &[DynamicObject<'_>],
    ) -> PixuiResult<Box<dyn Any + Send>> {
        let props = (self.resolve)(context, settings)?;
        let state = state.downcast_mut::<C::State>().ok_or_else(|| {
            pixui_error!(
                "component `{}` has incorrect state type; expected `{}`",
                std::any::type_name::<C>(),
                std::any::type_name::<C::State>()
            )
        })?;
        C::prepare(&props, state, context)?;
        (self.update)(&props, state)?;
        Ok(Box::new(props))
    }
}

pub(crate) struct ExpressionBinding<C: Component> {
    pub address: ComponentAddress,
    pub resolve: ExpressionPropsResolver<C>,
}
impl<C: Component> ErasedBinding for ExpressionBinding<C> {
    fn address(&self) -> ComponentAddress {
        self.address
    }
    fn prepare(
        &self,
        context: &ExpressionContext<'_>,
        settings: &PresentationSettings,
        state: &mut GenericComponentState,
        values: &[DynamicObject<'_>],
    ) -> PixuiResult<Box<dyn Any + Send>> {
        let props = (self.resolve)(context, settings, values)?;
        let state = state
            .downcast_mut::<C::State>()
            .ok_or_else(|| pixui_error!("incorrect component state type"))?;
        C::prepare(&props, state, context)?;
        Ok(Box::new(props))
    }
}
