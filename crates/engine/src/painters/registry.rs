//! Indexed painter dispatch with checked props/state downcasts.

use super::{context::PaintContext, painter::Painter};
use crate::{
    component_registry::{
        component_id::ComponentAddress,
        registry::{ComponentDescriptor, ComponentRegistry},
    },
    live_model::{component::Component, state::GenericComponentState},
    ui::{
        display_list_builder::DisplayListBuilder, geometry::Point,
        presentation::PresentationSettings,
    },
};
use pixui_base::{PixuiResult, pixui_error};
use std::any::Any;

pub(crate) struct PaintInput<'a> {
    pub props: &'a dyn Any,
    pub state: &'a GenericComponentState,
    pub settings: &'a PresentationSettings,
    pub width: f32,
    pub height: f32,
    pub focused: bool,
    pub hovered: bool,
    pub origin: Point,
    pub timestamp_us: u64,
}

trait ErasedPainter: Send {
    fn paint(&self, input: PaintInput<'_>, display: &mut DisplayListBuilder) -> PixuiResult<()>;
}

struct Adapter<C: Component, P: Painter<C>> {
    painter: P,
    marker: std::marker::PhantomData<fn() -> C>,
}
impl<C: Component, P: Painter<C>> ErasedPainter for Adapter<C, P> {
    fn paint(&self, input: PaintInput<'_>, display: &mut DisplayListBuilder) -> PixuiResult<()> {
        let props = input.props.downcast_ref::<C::Props>().ok_or_else(|| {
            pixui_error!(
                "painter props type mismatch for `{}`; expected `{}`",
                std::any::type_name::<C>(),
                std::any::type_name::<C::Props>()
            )
        })?;
        let state = input.state.downcast_ref::<C::State>().ok_or_else(|| {
            pixui_error!(
                "painter state type mismatch for `{}`; expected `{}`",
                std::any::type_name::<C>(),
                std::any::type_name::<C::State>()
            )
        })?;
        let mut context = PaintContext {
            props,
            state,
            settings: input.settings,
            width: input.width,
            height: input.height,
            focused: input.focused,
            hovered: input.hovered,
            display,
            origin: input.origin,
            timestamp_us: input.timestamp_us,
            clip_depth: 0,
            clip_error: false,
        };
        self.painter.paint(&mut context)?;
        if context.clip_error || context.clip_depth != 0 {
            return Err(pixui_error!(
                "painter clips must balance without popping renderer clips"
            ));
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct PainterRegistry {
    registry: Option<u64>,
    entries: Vec<Option<(ComponentAddress, Box<dyn ErasedPainter>)>>,
}

impl PainterRegistry {
    /// Requires registered component C and rejects a second painter for it.
    pub fn register<C: Component>(
        &mut self,
        components: &ComponentRegistry,
        painter: impl Painter<C>,
    ) -> PixuiResult<()> {
        let id = components.component::<C>()?;
        if self
            .registry
            .is_some_and(|registry| registry != id.address.registry)
        {
            return Err(pixui_error!(
                "painter registry belongs to a different component registry"
            ));
        }
        if self
            .entries
            .get(id.address.index)
            .is_some_and(Option::is_some)
        {
            return Err(pixui_error!(
                "duplicate painter for component `{}`",
                components.descriptor(id)?.name
            ));
        }
        self.entries.resize_with(id.address.index + 1, || None);
        self.registry = Some(id.address.registry);
        self.entries[id.address.index] = Some((
            id.address,
            Box::new(Adapter::<C, _> {
                painter,
                marker: std::marker::PhantomData,
            }),
        ));
        Ok(())
    }

    pub(crate) fn require(
        &self,
        address: ComponentAddress,
        descriptor: &ComponentDescriptor,
    ) -> PixuiResult<()> {
        if self
            .entries
            .get(address.index)
            .and_then(Option::as_ref)
            .is_none_or(|(registered, _)| *registered != address)
        {
            return Err(pixui_error!(
                "missing painter for component `{}`",
                descriptor.name
            ));
        }
        Ok(())
    }

    pub(crate) fn paint(
        &self,
        address: ComponentAddress,
        components: &ComponentRegistry,
        input: PaintInput<'_>,
        display: &mut DisplayListBuilder,
    ) -> PixuiResult<()> {
        let descriptor = components.resolve(address)?;
        self.require(address, descriptor)?;
        self.entries[address.index]
            .as_ref()
            .expect("painter checked")
            .1
            .paint(input, display)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        components::button::{ButtonComponent, ButtonProps, ButtonState},
        painters::button::ButtonPainter,
    };

    #[test]
    fn erased_adapter_rejects_incorrect_props_and_state() {
        let adapter = Adapter::<ButtonComponent, _> {
            painter: ButtonPainter,
            marker: std::marker::PhantomData,
        };
        let settings = PresentationSettings::default();
        let state = GenericComponentState::new(ButtonState::default());
        let input = |props, state| PaintInput {
            props,
            state,
            settings: &settings,
            width: 100.0,
            height: 36.0,
            focused: false,
            hovered: false,
            origin: Point::default(),
            timestamp_us: 0,
        };
        let error = adapter
            .paint(input(&(), &state), &mut DisplayListBuilder::default())
            .err()
            .unwrap();
        assert!(error.to_string().contains("props type mismatch"));
        let props = ButtonProps {
            label: "Button".into(),
        };
        let wrong_state = GenericComponentState::new(());
        let error = adapter
            .paint(
                input(&props, &wrong_state),
                &mut DisplayListBuilder::default(),
            )
            .err()
            .unwrap();
        assert!(error.to_string().contains("state type mismatch"));
        assert!(
            adapter
                .paint(input(&props, &state), &mut DisplayListBuilder::default())
                .is_ok()
        );
    }
}
