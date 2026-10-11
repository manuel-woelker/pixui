//! Explicit opt-in setup. Components never register their painters implicitly.

use super::painter::Painter;
use super::{
    button::ButtonPainter, checkbox::CheckboxPainter, image::ImagePainter, label::LabelPainter,
    registry::PainterRegistry,
};
use crate::{
    component_registry::{component_id::ComponentId, registry::ComponentRegistry},
    components::{
        button::ButtonComponent, checkbox::CheckboxComponent, image::ImageComponent,
        label::LabelComponent,
    },
};
use pixui_base::PixuiResult;

#[derive(Clone, Copy, Debug)]
pub struct StandardComponents {
    pub text_input: ComponentId<crate::components::text_input::TextInputComponent>,
    pub button: ComponentId<ButtonComponent>,
    pub label: ComponentId<LabelComponent>,
    pub image: ComponentId<ImageComponent>,
    pub checkbox: ComponentId<CheckboxComponent>,
}

/// Registers all standard types. Setup errors retain earlier successful
/// registrations; configure a fresh application and call this helper once.
pub fn register_components(registry: &mut ComponentRegistry) -> PixuiResult<StandardComponents> {
    Ok(StandardComponents {
        text_input: registry.register("text_input")?,
        button: registry.register("button")?,
        label: registry.register("label")?,
        checkbox: registry.register("checkbox")?,
        image: registry.register("image")?,
    })
}

/// Requires prior component registration. Applications can register individual
/// painters instead when supplying their own presentation for a standard type.
pub fn register_painters(
    registry: &mut PainterRegistry,
    components: &ComponentRegistry,
    styles: &mut crate::styles::registry::StyleRegistry,
) -> PixuiResult<()> {
    super::text_input::TextInputPainter::default().register(registry, components, styles)?;
    ButtonPainter::default().register(registry, components, styles)?;
    LabelPainter::default().register(registry, components, styles)?;
    CheckboxPainter::default().register(registry, components, styles)?;
    registry.register::<ImageComponent>(components, ImagePainter)
}
