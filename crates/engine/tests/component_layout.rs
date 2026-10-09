//! Observable layout contracts: physical hierarchy, sizing, clips and pure probes.
use pixui_base::PixuiResult;
use pixui_engine::{
    application::app::Application,
    component_registry::component_id::ComponentId,
    expression::expression::Expression,
    layout::{
        container::{ContainerLayout, ContainerPart},
        grid::{GridPlacement, Track, TrackMaximum, TrackMinimum},
        style::{Alignment, Insets, LayoutStyle, Length},
    },
    live_model::{
        component::Component,
        match_part::{MatchCandidate, MatchPart, MatchPattern},
        part::{ComponentPart, CompositePart, ForLoopPart, LivePart},
        state::LiveState,
    },
    painters::{context::PaintContext, measure::MeasureContext, painter::Painter},
    ui::{
        display_list::Color,
        geometry::Size,
        presentation::PresentationSettings,
        renderer::{self, RenderedUi},
    },
};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Block;
impl Component for Block {
    type Props = Size;
    type State = usize;
}
struct BlockPainter;
impl Painter<Block> for BlockPainter {
    fn measure(&self, context: &MeasureContext<'_, Block>) -> PixuiResult<Size> {
        Ok(context.constrain(*context.props))
    }
    fn paint(&self, context: &mut PaintContext<'_, Block>) -> PixuiResult<()> {
        context.fill_rect(context.bounds(), Color(100, 120, 140));
        Ok(())
    }
}
fn setup() -> (Application, ComponentId<Block>) {
    let mut app = Application::default();
    let id = app.register_component::<Block>("block").unwrap();
    app.register_painter::<Block>(BlockPainter).unwrap();
    (app, id)
}
fn leaf(id: ComponentId<Block>, style: LayoutStyle) -> LivePart {
    ComponentPart::typed(id, |_, _| {
        Ok(Size {
            width: 20.0,
            height: 10.0,
        })
    })
    .with_layout(style)
    .into()
}
fn render(app: &Application, tree: &LivePart, width: f32, height: f32) -> RenderedUi {
    renderer::render_measured(
        tree,
        &mut LiveState::new(),
        app,
        &PresentationSettings {
            viewport: Size { width, height },
            ..Default::default()
        },
        0.0,
        None,
        None,
    )
    .unwrap()
}
fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.02, "{actual} != {expected}");
}

#[test]
fn nested_flex_respects_border_box_padding_gaps_and_content_coordinates() {
    let (app, id) = setup();
    let tree = ContainerPart::row()
        .with_gap(8.0)
        .with_padding(4.0)
        .with_children(vec![
            leaf(id, LayoutStyle::fixed(30.0, 20.0).with_padding(3.0)),
            ContainerPart::column()
                .with_gap(2.0)
                .with_children(vec![
                    leaf(id, LayoutStyle::fixed(10.0, 5.0)),
                    leaf(id, LayoutStyle::fixed(10.0, 7.0)),
                ])
                .into(),
        ])
        .into();
    let output = render(&app, &tree, 200.0, 100.0);
    let bounds = &output.layout.component_bounds;
    close(bounds[0].x, 20.0);
    close(bounds[0].y, 20.0);
    close(bounds[1].x, 58.0);
    close(bounds[2].y, 27.0);
    let content = output.layout.content_bounds[0];
    close(content.x, 23.0);
    close(content.width, 24.0);
    close(content.height, 14.0);
    assert!(output.display_list.commands.iter().any(|command| matches!(command, pixui_engine::ui::display_list::DrawCommand::FillRect { rect, .. } if *rect == content)));
}
#[test]
fn flex_growth_shrink_wrap_and_alignment_use_parent_constraints() {
    let (app, id) = setup();
    let tree = ContainerPart::row()
        .with_gap(10.0)
        .with_children(vec![
            leaf(id, LayoutStyle::fixed(40.0, 20.0)),
            leaf(id, LayoutStyle::grow(1.0)),
        ])
        .into();
    let output = render(&app, &tree, 200.0, 100.0);
    close(output.layout.component_bounds[1].width, 118.0);
    let mut row = ContainerPart::row().with_gap(5.0);
    if let ContainerLayout::Flex(flex) = &mut row.layout {
        flex.wrap = true;
    }
    row.align = Alignment::Start;
    row.children = (0..3)
        .map(|_| leaf(id, LayoutStyle::fixed(60.0, 20.0)))
        .collect();
    let output = render(&app, &row.into(), 170.0, 100.0);
    close(output.layout.component_bounds[2].y, 41.0);
    let style = LayoutStyle {
        width: Length::Pixels(200.0),
        min_width: Length::Pixels(0.0),
        ..Default::default()
    };
    let output = render(
        &app,
        &ContainerPart::row()
            .with_children(vec![leaf(id, style.clone()), leaf(id, style)])
            .into(),
        200.0,
        100.0,
    );
    close(output.layout.component_bounds[0].width, 84.0);
}
#[test]
fn grid_auto_placement_fraction_tracks_spans_and_nested_flex() {
    let (app, id) = setup();
    let mut span = LayoutStyle::default();
    span.grid_column = GridPlacement {
        start: Some(1),
        span: 2,
    };
    let tree = ContainerPart::grid()
        .with_columns(vec![Track::length(40.0), Track::fraction(1.0)])
        .with_gap(8.0)
        .with_children(vec![
            leaf(id, LayoutStyle::default()),
            leaf(id, LayoutStyle::default()),
            ContainerPart::row()
                .with_layout(span)
                .with_children(vec![leaf(id, LayoutStyle::default())])
                .into(),
        ])
        .into();
    let output = render(&app, &tree, 200.0, 100.0);
    let bounds = &output.layout.component_bounds;
    close(bounds[0].width, 40.0);
    close(bounds[1].width, 120.0);
    close(bounds[1].x, 64.0);
    close(bounds[2].y, 34.0);
    let minmax = Track {
        min: TrackMinimum::Length(30.0),
        max: TrackMaximum::Fraction(1.0),
    };
    let tree = ContainerPart::grid()
        .with_columns(vec![Track::auto(), minmax])
        .with_children(vec![
            leaf(id, LayoutStyle::default()),
            leaf(id, LayoutStyle::default()),
        ])
        .into();
    let output = render(&app, &tree, 200.0, 100.0);
    close(output.layout.component_bounds[0].width, 20.0);
    close(output.layout.component_bounds[1].width, 148.0);
}
#[test]
fn fixed_containers_clip_descendants_without_expanding_outer_scroll_extent() {
    let (app, id) = setup();
    let tree = ContainerPart::column()
        .with_layout(LayoutStyle::fixed(80.0, 20.0))
        .with_children(vec![leaf(id, LayoutStyle::fixed(150.0, 100.0))])
        .into();
    let output = render(&app, &tree, 200.0, 100.0);
    close(output.layout.content_height, 100.0);
    close(output.layout.component_clips[0].width, 80.0);
    close(output.layout.component_clips[0].height, 20.0);
    let padded = leaf(id, LayoutStyle::fixed(10.0, 10.0).with_padding(20.0));
    let output = render(&app, &padded, 200.0, 100.0);
    assert_eq!(output.layout.content_bounds[0].width, 0.0);
    output.display_list.validate().unwrap();
}
#[test]
fn percentages_aspect_ratio_minmax_and_trailing_margins_are_explicit() {
    let (app, id) = setup();
    let style = LayoutStyle {
        width: Length::Percent(0.5),
        height: Length::Auto,
        aspect_ratio: Some(2.0),
        max_width: Length::Pixels(70.0),
        margin: Insets {
            bottom: 15.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let output = render(&app, &leaf(id, style), 200.0, 40.0);
    close(output.layout.component_bounds[0].width, 70.0);
    close(output.layout.component_bounds[0].height, 35.0);
    assert!(output.layout.content_height >= 82.0);
    let margins = leaf(
        id,
        LayoutStyle {
            margin: Insets {
                bottom: 15.0,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    close(
        render(&app, &margins, 200.0, 40.0).layout.content_height,
        57.0,
    );
    // Percent height has no definite parent height in the natural scroll root.
    let output = render(
        &app,
        &leaf(
            id,
            LayoutStyle {
                height: Length::Percent(1.0),
                ..Default::default()
            },
        ),
        200.0,
        100.0,
    );
    close(output.layout.component_bounds[0].height, 10.0);
    let output = render(
        &app,
        &ContainerPart::column()
            .with_layout(LayoutStyle::fixed(80.0, 60.0))
            .with_children(vec![leaf(
                id,
                LayoutStyle {
                    height: Length::Percent(0.5),
                    ..Default::default()
                },
            )])
            .into(),
        200.0,
        100.0,
    );
    close(output.layout.component_bounds[0].height, 30.0);
}
#[test]
fn loops_matches_and_fragments_splice_grid_items_without_phantom_gaps() {
    let (app, id) = setup();
    let sequence = Expression::computed(|_| {
        Ok(pixui_reflect::DynamicObject::from_reflect(vec![
            false, true, false,
        ]))
    });
    let body = LivePart::Match(
        MatchPart::new(
            Expression::computed(|context| {
                Ok(pixui_reflect::DynamicObject::from_reflect(
                    *context.value()?.downcast_ref::<bool>().unwrap(),
                ))
            }),
            vec![MatchCandidate {
                pattern: MatchPattern::value(false),
                part: leaf(id, LayoutStyle::default()),
            }],
        )
        .unwrap(),
    );
    let tree = ContainerPart::grid()
        .with_columns(vec![Track::fraction(1.0), Track::fraction(1.0)])
        .with_gap(8.0)
        .with_children(vec![
            LivePart::Composite(CompositePart { parts: vec![] }),
            LivePart::ForLoop(ForLoopPart {
                key: None,
                expression: sequence,
                body: Box::new(body),
            }),
        ])
        .into();
    let output = render(&app, &tree, 200.0, 100.0);
    assert_eq!(output.layout.component_bounds.len(), 2);
    close(
        output.layout.component_bounds[0].y,
        output.layout.component_bounds[1].y,
    );
    close(output.layout.component_bounds[1].x, 104.0);
}
static PREPARES: AtomicUsize = AtomicUsize::new(0);
static PAINTS: AtomicUsize = AtomicUsize::new(0);
struct CountingPainter;
impl Painter<Block> for CountingPainter {
    fn measure(&self, context: &MeasureContext<'_, Block>) -> PixuiResult<Size> {
        assert_eq!(*context.state, 1);
        Ok(context.constrain(*context.props))
    }
    fn paint(&self, _: &mut PaintContext<'_, Block>) -> PixuiResult<()> {
        PAINTS.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}
#[test]
fn solver_probes_do_not_repeat_preparation_or_painting() {
    let mut app = Application::default();
    let id = app.register_component::<Block>("counted").unwrap();
    app.register_painter::<Block>(CountingPainter).unwrap();
    let part = ComponentPart::typed_with_update(
        id,
        |_, _| {
            PREPARES.fetch_add(1, Ordering::Relaxed);
            Ok(Size {
                width: 20.0,
                height: 10.0,
            })
        },
        |_, state| {
            *state += 1;
            Ok(())
        },
    );
    let output = render(
        &app,
        &ContainerPart::grid()
            .with_columns(vec![Track::auto()])
            .with_children(vec![part.into()])
            .into(),
        200.0,
        100.0,
    );
    assert!(output.timings.measurements > 0);
    assert_eq!(PREPARES.load(Ordering::Relaxed), 1);
    assert_eq!(PAINTS.load(Ordering::Relaxed), 1);
}
#[test]
fn invalid_styles_and_measurements_fail_and_zero_viewport_is_valid() {
    let (app, id) = setup();
    for style in [
        LayoutStyle::fixed(f32::NAN, 20.0),
        LayoutStyle {
            grow: -1.0,
            ..Default::default()
        },
        LayoutStyle {
            grid_row: GridPlacement {
                start: Some(0),
                span: 1,
            },
            ..Default::default()
        },
    ] {
        assert!(
            renderer::render(
                &leaf(id, style),
                &mut LiveState::new(),
                &app,
                &PresentationSettings::default(),
                0.0,
                None,
                None
            )
            .is_err()
        );
    }
    render(&app, &leaf(id, LayoutStyle::default()), 0.0, 0.0)
        .display_list
        .validate()
        .unwrap();
    let bad = ComponentPart::typed(id, |_, _| {
        Ok(Size {
            width: f32::NAN,
            height: -1.0,
        })
    })
    .into();
    assert!(
        renderer::render(
            &bad,
            &mut LiveState::new(),
            &app,
            &PresentationSettings::default(),
            0.0,
            None,
            None
        )
        .is_err()
    );
}
#[test]
fn measure_context_text_matches_paint_metrics_and_long_text_can_shrink() {
    let mut app = Application::default();
    let label_id = app
        .register_component::<pixui_engine::components::label::LabelComponent>("label")
        .unwrap();
    app.register_painter::<pixui_engine::components::label::LabelComponent>(
        pixui_engine::painters::label::LabelPainter,
    )
    .unwrap();
    let label = ComponentPart::typed(label_id, |_, _| {
        Ok(pixui_engine::components::label::LabelProps {
            text: "UnbrokenText".repeat(30),
        })
    })
    .with_layout(LayoutStyle {
        min_width: Length::Pixels(0.0),
        ..Default::default()
    });
    let output = render(
        &app,
        &ContainerPart::grid()
            .with_columns(vec![Track::fraction(1.0)])
            .with_children(vec![label.into()])
            .into(),
        200.0,
        100.0,
    );
    close(output.layout.component_bounds[0].width, 168.0);
    let settings = PresentationSettings::default();
    let props = Size {
        width: 1.0,
        height: 1.0,
    };
    let context = MeasureContext::<Block> {
        props: &props,
        state: &0,
        settings: &settings,
        constraints: pixui_engine::painters::measure::MeasureConstraints {
            width: None,
            height: None,
            available_width: pixui_engine::painters::measure::AvailableSpace::MaxContent,
            available_height: pixui_engine::painters::measure::AvailableSpace::MaxContent,
        },
    };
    let font = pixui_engine::ui::text::font::FontConfig::new(
        pixui_engine::ui::text::font::FontFace::geist().unwrap(),
        16.0,
        1.0,
    )
    .unwrap();
    assert_eq!(
        context.measure_text("a\tb\r\nc", 16.0).unwrap(),
        font.measure_text("a\tb\r\nc").unwrap()
    );
    close(
        context.line_height(16.0).unwrap(),
        font.metrics().line_height,
    );
}

/// Observational timings, not a timing assertion: exercise real loop expansion
/// and full rendering at two master timestamps, including all offscreen leaves.
#[test]
fn representative_loop_layout_cost_is_observable_without_timing_thresholds() {
    let (app, id) = setup();
    for count in [100, 1000] {
        let expression = if count == 100 {
            Expression::computed(|_| {
                Ok(pixui_reflect::DynamicObject::from_reflect(vec![false; 100]))
            })
        } else {
            Expression::computed(|_| {
                Ok(pixui_reflect::DynamicObject::from_reflect(vec![
                    false;
                    1000
                ]))
            })
        };
        let root = ContainerPart::column()
            .with_gap(2.0)
            .with_children(vec![LivePart::ForLoop(ForLoopPart {
                key: None,
                expression,
                body: Box::new(leaf(id, LayoutStyle::default())),
            })])
            .into();
        let mut state = LiveState::new();
        for timestamp_us in [0, 16_667] {
            let settings = PresentationSettings {
                timestamp_us: Some(timestamp_us),
                viewport: Size {
                    width: 800.0,
                    height: 600.0,
                },
                ..Default::default()
            };
            let rendered =
                renderer::render_measured(&root, &mut state, &app, &settings, 0.0, None, None)
                    .unwrap();
            assert_eq!(rendered.layout.component_bounds.len(), count);
            assert!(rendered.timings.measurements > 0);
            eprintln!(
                "loop={count} timestamp={timestamp_us} viewport=800x600 scale=1 worker-only debug timings={:?}",
                rendered.timings
            );
        }
    }
}

#[test]
fn explicit_multiline_labels_fit_their_measured_height() {
    let mut app = Application::default();
    let id = app
        .register_component::<pixui_engine::components::label::LabelComponent>("label")
        .unwrap();
    app.register_painter::<pixui_engine::components::label::LabelComponent>(
        pixui_engine::painters::label::LabelPainter,
    )
    .unwrap();
    let part = ComponentPart::typed(id, |_, _| {
        Ok(pixui_engine::components::label::LabelProps {
            text: "First\r\nSecond".into(),
        })
    })
    .into();
    let output = render(&app, &part, 200.0, 100.0);
    let content = output.layout.content_bounds[0];
    let font = &output.display_list.fonts[0];
    let metrics = font.metrics();
    close(content.height, 2.0 * metrics.line_height);
    let origin = output
        .display_list
        .commands
        .iter()
        .find_map(|command| match command {
            pixui_engine::ui::display_list::DrawCommand::DrawText { origin, .. } => Some(*origin),
            _ => None,
        })
        .unwrap();
    assert!(origin.y - metrics.ascent >= content.y - 0.01);
    assert!(origin.y + metrics.line_height - metrics.descent <= content.y + content.height + 0.01);
}

#[test]
fn independently_registered_painters_determine_intrinsic_size() {
    struct WiderPainter;
    impl Painter<Block> for WiderPainter {
        fn measure(&self, context: &MeasureContext<'_, Block>) -> PixuiResult<Size> {
            Ok(context.constrain(Size {
                width: 80.0,
                height: 40.0,
            }))
        }
        fn paint(&self, _: &mut PaintContext<'_, Block>) -> PixuiResult<()> {
            Ok(())
        }
    }
    let (first, first_id) = setup();
    let mut second = Application::default();
    let second_id = second.register_component::<Block>("block").unwrap();
    second.register_painter::<Block>(WiderPainter).unwrap();
    let style = LayoutStyle {
        align_self: Some(Alignment::Start),
        ..Default::default()
    };
    let narrow = render(&first, &leaf(first_id, style.clone()), 200.0, 100.0);
    let wide = render(&second, &leaf(second_id, style), 200.0, 100.0);
    close(narrow.layout.component_bounds[0].width, 20.0);
    close(wide.layout.component_bounds[0].width, 80.0);
    close(wide.layout.component_bounds[0].height, 40.0);
}
