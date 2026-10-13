//! Custom component behavior emits a typed event; its painter only draws.
use pixui_base::PixuiResult;
use pixui_engine::{
    events::interface::{ComponentInput, InputInterest},
    live_model::component::Component,
    painters::{context::PaintContext, painter::Painter},
    ui::{display_list::Color, input::MouseButton},
};
use pixui_engine_macros::component_events;

#[component_events]
pub trait ClickEvents {
    fn click(&self, x: f32, y: f32) -> PixuiResult<()>;
}
#[pixui_reflect::reflect(send)]
mod data {
    pub struct ClickAreaProps {
        pub label: String,
    }
}
pub struct ClickArea;
impl Component for ClickArea {
    type Props = data::ClickAreaProps;
    type State = ();
    type Events = dyn ClickEvents;
    const INPUT_INTEREST: InputInterest = InputInterest::CLICK;
    fn handle_input(
        _props: &Self::Props,
        _state: &mut Self::State,
        input: &ComponentInput,
        events: &Self::Events,
    ) -> PixuiResult<bool> {
        if let ComponentInput::Click {
            position,
            button: MouseButton::Left,
            ..
        } = input
        {
            events.click(position.x, position.y)?;
            return Ok(true);
        }
        Ok(false)
    }
}
pub struct ClickAreaPainter;
impl Painter<ClickArea> for ClickAreaPainter {
    fn measure(
        &self,
        context: &pixui_engine::painters::measure::MeasureContext<'_, ClickArea>,
    ) -> PixuiResult<pixui_engine::ui::geometry::Size> {
        Ok(context.constrain(pixui_engine::ui::geometry::Size {
            width: 300.0,
            height: 40.0,
        }))
    }
    fn paint(&self, context: &mut PaintContext<'_, ClickArea>) -> PixuiResult<()> {
        let fill = if context.hovered {
            Color(50, 90, 140)
        } else {
            Color(35, 65, 100)
        };
        context.fill_rect(context.bounds(), fill);
        context.text(
            pixui_engine::ui::geometry::Point { x: 8.0, y: 24.0 },
            context.props.label.clone(),
            16.0,
            Color(255, 255, 255),
        )
    }
}
