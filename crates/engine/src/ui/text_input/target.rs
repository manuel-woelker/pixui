//! Published worker-local input target: authoritative props and fresh action binding.
use super::{
    binding::ChangeBinding,
    editing::EditState,
    geometry::{TextEditSnapshot, TextInputGeometry},
};
use crate::{live_model::identity::ComponentPath, ui::geometry::Rect};
pub struct TextInputTarget {
    pub path: ComponentPath,
    pub component_index: usize,
    pub state: EditState,
    pub geometry: TextInputGeometry,
    pub bounds: Rect,
    pub clip: Rect,
    pub scroll: f32,
    pub(crate) change: Option<ChangeBinding>,
    pub(crate) change_factory: Option<usize>,
}
pub(crate) struct RenderEditing<'a> {
    pub states: &'a std::collections::HashMap<ComponentPath, EditState>,
    pub previous: &'a [TextInputTarget],
    pub active: bool,
}
impl TextInputTarget {
    pub(crate) fn snapshot(&self, active: bool) -> TextEditSnapshot {
        TextEditSnapshot {
            selection: self.state.selection,
            geometry: self.geometry.clone(),
            scroll: self.scroll,
            blink_reset_us: self.state.blink_reset_us,
            active,
        }
    }
}
