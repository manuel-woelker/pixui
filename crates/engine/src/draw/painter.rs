use crate::component::Component;
use crate::draw::draw_list::DrawList;
use pixui_base::result::PixuiResult;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

/// Paint context passed to a component painter.
pub struct PaintContext<'a, T: Component> {
    /// Draw list being built for the component.
    pub draw_list: &'a mut DrawList,
    phantom: PhantomData<T>,
}

impl<'a, T: Component> PaintContext<'a, T> {
    /// Creates a paint context for a component-specific draw pass.
    pub fn new(draw_list: &'a mut DrawList) -> PaintContext<'a, T> {
        PaintContext {
            draw_list,
            phantom: PhantomData,
        }
    }
}

impl<'a, T: Component> Deref for PaintContext<'a, T> {
    type Target = DrawList;

    fn deref(&self) -> &Self::Target {
        self.draw_list
    }
}

impl<'a, T: Component> DerefMut for PaintContext<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.draw_list
    }
}

/// Emits draw commands for a component.
pub trait ComponentPainter {
    /// Component type painted by this implementation.
    type Component: Component;

    /// Appends draw commands for the component to the paint context.
    fn paint(paint_context: &mut PaintContext<Self::Component>) -> PixuiResult<()>;
}
