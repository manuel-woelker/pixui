use facet::Facet;
use pixui_base::result::PixuiResult;
use pixui_engine::app::Application;
use pixui_engine::draw::brush::Brush;
use pixui_engine::draw::color::Color;
use pixui_engine::draw::command::DrawCommand;
use pixui_engine::draw::draw_bounds::DrawBounds;
use pixui_engine::draw::draw_list::DrawList;
use pixui_engine::draw::draw_style::DrawStyle;
use pixui_engine::draw::text_style::TextStyle;
use pixui_engine::engine::Engine;
use pixui_engine::viewport::Viewport;
use pixui_winit::WinitAdapter;

#[derive(Debug, Facet)]
struct Counter {
    count: u32,
}

#[derive(Debug)]
struct IncrementCount;

#[derive(Copy, Clone)]
struct SegmentRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Copy, Clone)]
struct SegmentStyles {
    off_style: pixui_engine::draw::style_id::StyleId,
    on_style: pixui_engine::draw::style_id::StyleId,
}

fn main() -> PixuiResult<()> {
    let engine = Engine::new()?;

    let count_ref = engine.run_application(|application| {
        let entity_key = application.add_entity(Counter { count: 0 })?;
        Ok(entity_key)
    })?;
    engine.on_event::<IncrementCount>(move |context| {
        let counter = context.get_entity_mut(count_ref)?;
        counter.count = counter.count.saturating_add(1);
        Ok(())
    })?;
    let winit_adapter = WinitAdapter::new(&engine)?;
    winit_adapter.register_component_renderer(
        "CounterApp",
        move |application: &Application, viewport: &Viewport| {
            let count = application.entity_store().get_entity(count_ref)?.count;
            Ok(build_counter_draw_list(count, viewport))
        },
    );
    engine.submit_event(IncrementCount)?;
    winit_adapter.create_window("CounterApp")?;
    Ok(())
}

fn build_counter_draw_list(count: u32, viewport: &Viewport) -> DrawList {
    let mut draw_list = DrawList::new(DrawBounds::new(0.0, 0.0, viewport.width, viewport.height));

    let background_style = draw_list.push_style(draw_style(Color::rgba(15, 18, 26, 255), 1.0));
    let panel_fill_style = draw_list.push_style(draw_style(Color::rgba(27, 32, 44, 255), 1.0));
    let panel_border_style = draw_list.push_style(draw_style(Color::rgba(78, 88, 117, 255), 3.0));
    let segment_styles = SegmentStyles {
        on_style: draw_list.push_style(draw_style(Color::rgba(255, 120, 64, 255), 1.0)),
        off_style: draw_list.push_style(draw_style(Color::rgba(70, 45, 40, 255), 1.0)),
    };

    draw_list.push_command(DrawCommand::SelectStyle {
        style_id: background_style,
    });
    draw_list.push_command(DrawCommand::FillRoundedRectangle {
        x: 0.0,
        y: 0.0,
        width: viewport.width,
        height: viewport.height,
        radius: 0.0,
    });

    let digit_text = count.to_string();
    let digit_count = digit_text.len().max(1) as f32;
    let digit_height = (viewport.height * 0.55).clamp(120.0, 280.0);
    let digit_width = digit_height * 0.56;
    let spacing = digit_width * 0.18;
    let total_width = digit_count * digit_width + (digit_count - 1.0) * spacing;
    let start_x = (viewport.width - total_width) * 0.5;
    let start_y = (viewport.height - digit_height) * 0.5;

    draw_counter_panel(
        &mut draw_list,
        panel_fill_style,
        panel_border_style,
        start_x - 32.0,
        start_y - 32.0,
        total_width + 64.0,
        digit_height + 64.0,
    );

    for (index, digit) in digit_text.chars().enumerate() {
        let x = start_x + index as f32 * (digit_width + spacing);
        draw_digit(
            &mut draw_list,
            segment_styles,
            x,
            start_y,
            digit_width,
            digit_height,
            digit,
        );
    }

    draw_list
}

fn draw_style(color: Color, width: f32) -> DrawStyle {
    DrawStyle {
        brush: Brush::SolidColor(color),
        width,
        text_style: TextStyle::new("DejaVuSans", 14.0),
    }
}

fn draw_counter_panel(
    draw_list: &mut DrawList,
    fill_style: pixui_engine::draw::style_id::StyleId,
    border_style: pixui_engine::draw::style_id::StyleId,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) {
    draw_list.push_command(DrawCommand::SelectStyle {
        style_id: fill_style,
    });
    draw_list.push_command(DrawCommand::FillRoundedRectangle {
        x,
        y,
        width,
        height,
        radius: 28.0,
    });
    draw_list.push_command(DrawCommand::SelectStyle {
        style_id: border_style,
    });
    draw_list.push_command(DrawCommand::OutlineRoundedRectangle {
        x,
        y,
        width,
        height,
        radius: 28.0,
    });
}

fn draw_digit(
    draw_list: &mut DrawList,
    segment_styles: SegmentStyles,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    digit: char,
) {
    let thickness = width * 0.18;
    let horizontal_length = width - thickness * 1.5;
    let vertical_height = (height - thickness * 3.0) * 0.5;
    let radius = thickness * 0.45;
    let active_segments = active_segments(digit);

    draw_segment(
        draw_list,
        SegmentRect {
            x: x + thickness * 0.75,
            y,
            width: horizontal_length,
            height: thickness,
        },
        radius,
        active_segments[0],
        segment_styles,
    );
    draw_segment(
        draw_list,
        SegmentRect {
            x: x + width - thickness,
            y: y + thickness * 0.6,
            width: thickness,
            height: vertical_height,
        },
        radius,
        active_segments[1],
        segment_styles,
    );
    draw_segment(
        draw_list,
        SegmentRect {
            x: x + width - thickness,
            y: y + height * 0.5 + thickness * 0.1,
            width: thickness,
            height: vertical_height,
        },
        radius,
        active_segments[2],
        segment_styles,
    );
    draw_segment(
        draw_list,
        SegmentRect {
            x: x + thickness * 0.75,
            y: y + height - thickness,
            width: horizontal_length,
            height: thickness,
        },
        radius,
        active_segments[3],
        segment_styles,
    );
    draw_segment(
        draw_list,
        SegmentRect {
            x,
            y: y + height * 0.5 + thickness * 0.1,
            width: thickness,
            height: vertical_height,
        },
        radius,
        active_segments[4],
        segment_styles,
    );
    draw_segment(
        draw_list,
        SegmentRect {
            x,
            y: y + thickness * 0.6,
            width: thickness,
            height: vertical_height,
        },
        radius,
        active_segments[5],
        segment_styles,
    );
    draw_segment(
        draw_list,
        SegmentRect {
            x: x + thickness * 0.75,
            y: y + (height - thickness) * 0.5,
            width: horizontal_length,
            height: thickness,
        },
        radius,
        active_segments[6],
        segment_styles,
    );
}

fn draw_segment(
    draw_list: &mut DrawList,
    rect: SegmentRect,
    radius: f32,
    is_on: bool,
    segment_styles: SegmentStyles,
) {
    draw_list.push_command(DrawCommand::SelectStyle {
        style_id: if is_on {
            segment_styles.on_style
        } else {
            segment_styles.off_style
        },
    });
    draw_list.push_command(DrawCommand::FillRoundedRectangle {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
        radius,
    });
}

fn active_segments(digit: char) -> [bool; 7] {
    match digit {
        '0' => [true, true, true, true, true, true, false],
        '1' => [false, true, true, false, false, false, false],
        '2' => [true, true, false, true, true, false, true],
        '3' => [true, true, true, true, false, false, true],
        '4' => [false, true, true, false, false, true, true],
        '5' => [true, false, true, true, false, true, true],
        '6' => [true, false, true, true, true, true, true],
        '7' => [true, true, true, false, false, false, false],
        '8' => [true, true, true, true, true, true, true],
        '9' => [true, true, true, true, false, true, true],
        _ => [false, false, false, false, false, false, false],
    }
}
