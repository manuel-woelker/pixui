//! Explicit opt-in setup. Components never register their painters implicitly.

use super::{
    button::ButtonPainter, checkbox::CheckboxPainter, label::LabelPainter,
    registry::PainterRegistry,
};
use crate::{
    component_registry::{component_id::ComponentId, registry::ComponentRegistry},
    components::{button::ButtonComponent, checkbox::CheckboxComponent, label::LabelComponent},
};
use pixui_base::PixuiResult;

#[derive(Clone, Copy, Debug)]
pub struct StandardComponents {
    pub button: ComponentId<ButtonComponent>,
    pub label: ComponentId<LabelComponent>,
    pub checkbox: ComponentId<CheckboxComponent>,
}

/// Registers all three standard types. Setup errors retain earlier successful
/// registrations; configure a fresh application and call this helper once.
pub fn register_components(registry: &mut ComponentRegistry) -> PixuiResult<StandardComponents> {
    Ok(StandardComponents {
        button: registry.register("button")?,
        label: registry.register("label")?,
        checkbox: registry.register("checkbox")?,
    })
}

/// Requires prior component registration. Applications can register individual
/// painters instead when supplying their own presentation for a standard type.
pub fn register_painters(
    registry: &mut PainterRegistry,
    components: &ComponentRegistry,
) -> PixuiResult<()> {
    registry.register::<ButtonComponent>(components, ButtonPainter)?;
    registry.register::<LabelComponent>(components, LabelPainter)?;
    registry.register::<CheckboxComponent>(components, CheckboxPainter)
}
