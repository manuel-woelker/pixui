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
    pub styles: &'a [crate::styles::style::PaintStyle],
    pub text_edit: Option<&'a crate::ui::text_input::geometry::TextEditSnapshot>,
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

pub(crate) struct MeasureInput<'a> {
    pub styles: &'a [crate::styles::style::PaintStyle],
    pub props: &'a dyn Any,
    pub state: &'a GenericComponentState,
    pub settings: &'a PresentationSettings,
    pub constraints: super::measure::MeasureConstraints,
}
pub(crate) trait ErasedPainter: Send {
    fn style_states(
        &self,
        props: &dyn Any,
        state: &GenericComponentState,
    ) -> PixuiResult<crate::styles::style::StyleStates>;
    fn text_input_geometry(
        &self,
        input: MeasureInput<'_>,
    ) -> PixuiResult<Option<crate::ui::text_input::geometry::TextInputGeometry>>;
    fn measure(&self, input: MeasureInput<'_>) -> PixuiResult<crate::ui::geometry::Size>;
    fn paint(&self, input: PaintInput<'_>, display: &mut DisplayListBuilder) -> PixuiResult<()>;
}

struct Adapter<C: Component, P: Painter<C>> {
    painter: P,
    marker: std::marker::PhantomData<fn() -> C>,
}
impl<C: Component, P: Painter<C>> ErasedPainter for Adapter<C, P> {
    fn style_states(
        &self,
        props: &dyn Any,
        state: &GenericComponentState,
    ) -> PixuiResult<crate::styles::style::StyleStates> {
        Ok(P::style_states(
            props
                .downcast_ref::<C::Props>()
                .ok_or_else(|| pixui_error!("style props mismatch"))?,
            state
                .downcast_ref::<C::State>()
                .ok_or_else(|| pixui_error!("style state mismatch"))?,
        ))
    }

    fn measure(&self, input: MeasureInput<'_>) -> PixuiResult<crate::ui::geometry::Size> {
        let props = input
            .props
            .downcast_ref::<C::Props>()
            .ok_or_else(|| pixui_error!("measurement props type mismatch"))?;
        let state = input
            .state
            .downcast_ref::<C::State>()
            .ok_or_else(|| pixui_error!("measurement state type mismatch"))?;
        let size = self.painter.measure(&super::measure::MeasureContext {
            styles: input.styles,
            props,
            state,
            settings: input.settings,
            constraints: input.constraints,
        })?;
        if !size.width.is_finite()
            || !size.height.is_finite()
            || size.width < 0.0
            || size.height < 0.0
        {
            return Err(pixui_error!(
                "painter returned invalid measured size {size:?} for {:?}",
                input.constraints
            ));
        }
        Ok(size)
    }
    fn text_input_geometry(
        &self,
        input: MeasureInput<'_>,
    ) -> PixuiResult<Option<crate::ui::text_input::geometry::TextInputGeometry>> {
        self.painter
            .text_input_geometry(&super::measure::MeasureContext {
                styles: input.styles,
                props: input
                    .props
                    .downcast_ref::<C::Props>()
                    .ok_or_else(|| pixui_error!("input geometry props mismatch"))?,
                state: input
                    .state
                    .downcast_ref::<C::State>()
                    .ok_or_else(|| pixui_error!("input geometry state mismatch"))?,
                settings: input.settings,
                constraints: input.constraints,
            })
    }
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
            text_edit: input.text_edit,
            styles: input.styles,
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
    slots: std::collections::HashMap<usize, SlotBinding>,
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
}

type BoundConstructor = fn(
    &crate::styles::slots::PainterSlots,
    &[crate::styles::index::StyleIndex],
) -> PixuiResult<Box<dyn ErasedPainter>>;
struct SlotBinding {
    slots: crate::styles::slots::PainterSlots,
    assignments: Vec<crate::styles::family::StyleAssignment>,
    construct: BoundConstructor,
}
/// A raw painter borrow or a freshly constructed styled occurrence. Worker-local;
/// a raw painter may be Send without being Sync.
pub(crate) enum BoundPainter<'a> {
    Raw(&'a dyn ErasedPainter),
    Styled(Box<dyn ErasedPainter>),
}
impl BoundPainter<'_> {
    fn painter(&self) -> &dyn ErasedPainter {
        match self {
            Self::Raw(value) => *value,
            Self::Styled(value) => value.as_ref(),
        }
    }
    pub(crate) fn measure(
        &self,
        input: MeasureInput<'_>,
    ) -> PixuiResult<crate::ui::geometry::Size> {
        self.painter().measure(input)
    }
    pub(crate) fn text_input_geometry(
        &self,
        input: MeasureInput<'_>,
    ) -> PixuiResult<Option<crate::ui::text_input::geometry::TextInputGeometry>> {
        self.painter().text_input_geometry(input)
    }
    pub(crate) fn paint(
        &self,
        input: PaintInput<'_>,
        display: &mut DisplayListBuilder,
    ) -> PixuiResult<()> {
        self.painter().paint(input, display)
    }
}
pub(crate) struct BoundOccurrence<'a> {
    pub painter: BoundPainter<'a>,
    pub states: crate::styles::style::StyleStates,
    pub range: std::ops::Range<usize>,
}
impl PainterRegistry {
    pub(crate) fn validate_styles(
        &self,
        registry: &crate::styles::registry::StyleRegistry,
    ) -> PixuiResult<()> {
        for binding in self.slots.values() {
            for assignment in &binding.assignments {
                for theme in [
                    crate::ui::presentation::Theme::Light,
                    crate::ui::presentation::Theme::Dark,
                ] {
                    registry.resolve(assignment, theme, 1.0, Default::default())?;
                }
            }
        }
        Ok(())
    }
    pub(crate) fn validate_template_styles(
        &self,
        template: &crate::live_model::part::LivePart,
        registry: &crate::styles::registry::StyleRegistry,
    ) -> PixuiResult<()> {
        use crate::live_model::part::LivePart;
        let mut pending = vec![template];
        let mut composite_seen = std::collections::HashSet::new();
        while let Some(part) = pending.pop() {
            match part {
                LivePart::Invoke(part) => {
                    if composite_seen.insert(part.definition.id) {
                        pending.push(&part.definition.body);
                    }
                    pending.push(&part.children);
                }
                LivePart::Slot => {}
                LivePart::Dynamic(_) => {}

                LivePart::Composite(part) => pending.extend(&part.parts),
                LivePart::Container(part) => pending.extend(&part.children),
                LivePart::ForLoop(part) => pending.push(&part.body),
                LivePart::Match(part) => {
                    pending.extend(part.candidates.iter().map(|candidate| &candidate.part))
                }
                LivePart::Component(part) => {
                    let Some(address) = part.component_address() else {
                        if !part.styles.is_empty() {
                            return Err(pixui_error!("legacy components have no painter slots"));
                        }
                        continue;
                    };
                    let mut seen = std::collections::HashSet::new();
                    for value in &part.styles {
                        let binding = self
                            .slots
                            .get(&address.index)
                            .ok_or_else(|| pixui_error!("raw painter has no style slots"))?;
                        let index = binding.slots.index(&value.slot)?;
                        if !seen.insert(index) {
                            return Err(pixui_error!("duplicate slot override `{}`", value.slot));
                        }
                        for theme in [
                            crate::ui::presentation::Theme::Light,
                            crate::ui::presentation::Theme::Dark,
                        ] {
                            registry.resolve(&value.style, theme, 1.0, Default::default())?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
    /// Inspect reflected slot metadata; raw painters return None.
    pub fn slots<C: Component>(
        &self,
        components: &ComponentRegistry,
    ) -> PixuiResult<Option<&crate::styles::slots::PainterSlots>> {
        let id = components.component::<C>()?;
        self.require(id.address, components.descriptor(id)?)?;
        Ok(self
            .slots
            .get(&id.address.index)
            .map(|binding| &binding.slots))
    }
    /// Reflected painters use only StyleIndex fields and #[reflect(send)]. Defaults
    /// must cover every slot exactly once. Construction follows reflected order.
    pub fn register_styled<C: Component, P: Painter<C> + pixui_reflect::Reflect>(
        &mut self,
        components: &ComponentRegistry,
        styles: &crate::styles::registry::StyleRegistry,
        defaults: Vec<crate::styles::slots::SlotAssignment>,
    ) -> PixuiResult<()> {
        let slots = crate::styles::slots::PainterSlots::discover::<P>()?;
        let assignments = slots.assignments(defaults)?;
        for assignment in &assignments {
            for theme in [
                crate::ui::presentation::Theme::Light,
                crate::ui::presentation::Theme::Dark,
            ] {
                styles.resolve(assignment, theme, 1.0, Default::default())?;
            }
        }
        let indices = vec![crate::styles::index::StyleIndex::default(); assignments.len()];
        let painter = slots.construct::<P>(&indices)?;
        self.register::<C>(components, painter)?;
        let index = components.component::<C>()?.address.index;
        self.slots.insert(
            index,
            SlotBinding {
                slots,
                assignments,
                construct: |slots, indices| {
                    Ok(Box::new(Adapter::<C, P> {
                        painter: slots.construct::<P>(indices)?,
                        marker: std::marker::PhantomData,
                    }))
                },
            },
        );
        Ok(())
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "binding combines registration, physical state and output-local ownership"
    )]
    pub(crate) fn bind<'a>(
        &'a self,
        address: ComponentAddress,
        props: &dyn Any,
        state: &GenericComponentState,
        settings: &PresentationSettings,
        registry: &crate::styles::registry::StyleRegistry,
        overrides: &[crate::styles::slots::SlotAssignment],
        prepared: &mut Vec<crate::styles::registry::PreparedStyle>,
    ) -> PixuiResult<BoundOccurrence<'a>> {
        let (registered, painter) = self
            .entries
            .get(address.index)
            .and_then(Option::as_ref)
            .ok_or_else(|| pixui_error!("missing painter"))?;
        if *registered != address {
            return Err(pixui_error!("foreign painter"));
        }
        let states = painter.style_states(props, state)?;
        let start = prepared.len();
        let Some(binding) = self.slots.get(&address.index) else {
            if !overrides.is_empty() {
                return Err(pixui_error!("raw painter has no style slots"));
            }
            return Ok(BoundOccurrence {
                painter: BoundPainter::Raw(painter.as_ref()),
                states,
                range: start..start,
            });
        };
        let mut assignments = binding.assignments.clone();
        let mut seen = std::collections::HashSet::new();
        for value in overrides {
            let index = binding.slots.index(&value.slot)?;
            if !seen.insert(index) {
                return Err(pixui_error!("duplicate slot override `{}`", value.slot));
            }
            assignments[index] = value.style.clone();
        }
        let mut indices = Vec::new();
        for assignment in assignments {
            indices.push(crate::styles::index::StyleIndex {
                index: prepared.len(),
            });
            prepared.push(registry.prepare(&assignment, settings.theme)?);
        }
        Ok(BoundOccurrence {
            painter: BoundPainter::Styled((binding.construct)(&binding.slots, &indices)?),
            states,
            range: start..prepared.len(),
        })
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
            painter: ButtonPainter {
                body: crate::styles::index::StyleIndex { index: 0 },
                focus_ring: crate::styles::index::StyleIndex { index: 0 },
            },
            marker: std::marker::PhantomData,
        };
        let settings = PresentationSettings::default();
        let state = GenericComponentState::new(ButtonState::default());
        let styles = vec![crate::styles::style::PaintStyle::text(
            &crate::styles::theme::ThemeTokens::for_theme(settings.theme),
        )];
        let input = |props, state| PaintInput {
            styles: &styles,
            text_edit: None,
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
        let mut display = DisplayListBuilder::default();
        display.set_styles(&styles);
        assert!(adapter.paint(input(&props, &state), &mut display).is_ok());
    }
}
