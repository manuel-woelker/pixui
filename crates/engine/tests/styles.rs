//! Public contracts for theme recipes, state precedence, and reflected slots.
use pixui_engine::{
    styles::{
        family::{StyleAssignment, StyleFamily},
        index::StyleIndex,
        registry::StyleRegistry,
        slots::{PainterSlots, SlotAssignment},
        style::{PaintStyle, StatePatch, StylePatch, StyleStates},
        theme::{ThemeFont, ThemeTokens},
    },
    ui::{display_list::Color, presentation::Theme},
};
use pixui_reflect::Reflect;
#[pixui_reflect::reflect(send)]
mod fixtures {
    use pixui_engine::styles::index::StyleIndex;
    pub struct Slots {
        pub body: StyleIndex,
        pub text: StyleIndex,
    }
    pub struct Configured {
        pub body: StyleIndex,
        pub count: u32,
    }
}
fn body(tokens: &ThemeTokens) -> PaintStyle {
    let mut style = PaintStyle::text(tokens);
    style.fill = Some(tokens.surface.into());
    style
}
#[test]
fn reflection_binds_constructor_order_and_rejects_invalid_assignments() {
    let slots = PainterSlots::discover::<fixtures::Slots>().unwrap();
    assert_eq!(slots.names().collect::<Vec<_>>(), ["body", "text"]);
    assert!(PainterSlots::discover::<fixtures::Configured>().is_err());
    let mut registry = StyleRegistry::default();
    let id = registry.register("body", StyleFamily::new(body)).unwrap();
    let assignment = || StyleAssignment::new(id);
    let ordered = slots
        .assignments(vec![
            SlotAssignment::new("text", assignment()),
            SlotAssignment::new("body", assignment()),
        ])
        .unwrap();
    assert_eq!(ordered.len(), 2);
    assert!(
        slots
            .assignments(vec![SlotAssignment::new("body", assignment())])
            .is_err()
    );
    assert!(
        slots
            .assignments(vec![
                SlotAssignment::new("body", assignment()),
                SlotAssignment::new("body", assignment())
            ])
            .is_err()
    );
    assert!(
        slots
            .assignments(vec![SlotAssignment::new("unknown", assignment())])
            .is_err()
    );
    let painter = slots
        .construct::<fixtures::Slots>(&[StyleIndex { index: 3 }, StyleIndex { index: 1 }])
        .unwrap();
    assert_eq!(painter.body.index, 3);
    assert_eq!(painter.text.index, 1);
    assert_eq!(fixtures::Slots::type_descriptor().fields().len(), 2);
    assert_eq!(StyleIndex::default().index, usize::MAX);
}
#[test]
fn variants_states_and_local_removal_have_deterministic_precedence() {
    let mut registry = StyleRegistry::default();
    let mut family = StyleFamily::new(body);
    family.variants.insert(
        "primary".into(),
        StylePatch {
            fill: Some(Some(Color(1, 2, 3).into())),
            ..Default::default()
        },
    );
    family.active = StatePatch {
        fill: Some(Color(4, 5, 6).into()),
        ..Default::default()
    };
    family.hover = StatePatch {
        fill: Some(Color(7, 8, 9).into()),
        ..Default::default()
    };
    family.disabled = StatePatch {
        fill: Some(Color(10, 11, 12).into()),
        ..Default::default()
    };
    let id = registry.register("button", family).unwrap();
    let mut assignment = StyleAssignment::new(id).variant("primary");
    let resolve = |assignment: &StyleAssignment, states| {
        registry
            .resolve(assignment, Theme::Light, 1.0, states)
            .unwrap()
    };
    assert_eq!(
        resolve(&assignment, StyleStates::default()).fill,
        Some(Color(1, 2, 3).into())
    );
    let all = StyleStates {
        active: true,
        hovered: true,
        disabled: true,
    };
    assert_eq!(
        resolve(&assignment, all).fill,
        Some(Color(10, 11, 12).into())
    );
    assignment.patch.fill = Some(None);
    assert_eq!(resolve(&assignment, all).fill, None);
    assignment.hover.fill = Some(Color(13, 14, 15).into());
    assert_eq!(
        resolve(&assignment, all).fill,
        Some(Color(13, 14, 15).into())
    );
    assert!(
        registry
            .resolve(
                &StyleAssignment::new(id).variant("missing"),
                Theme::Dark,
                1.0,
                all
            )
            .is_err()
    );
}
#[test]
fn replacements_are_atomic_and_foreign_ids_fail() {
    let mut registry = StyleRegistry::default();
    let id = registry.register("body", StyleFamily::new(body)).unwrap();
    let revision = registry.revision();
    let mut tokens = registry.tokens(Theme::Light).clone();
    tokens.inset = f32::NAN;
    assert!(registry.set_tokens(Theme::Light, tokens).is_err());
    assert_eq!(registry.revision(), revision);
    let invalid = StyleFamily::new(|tokens| {
        let mut style = PaintStyle::text(tokens);
        style.font_size = -1.0;
        style
    });
    assert!(registry.replace(id, invalid).is_err());
    assert_eq!(registry.revision(), revision);
    assert!(
        StyleRegistry::default()
            .resolve(
                &StyleAssignment::new(id),
                Theme::Light,
                1.0,
                Default::default()
            )
            .is_err()
    );
    let mut tokens = registry.tokens(Theme::Dark).clone();
    tokens.font = ThemeFont::Mono;
    tokens.inset = 12.0;
    registry.set_tokens(Theme::Dark, tokens).unwrap();
    assert_eq!(registry.revision(), revision + 1);
    let style = registry
        .resolve(
            &StyleAssignment::new(id),
            Theme::Dark,
            1.5,
            Default::default(),
        )
        .unwrap();
    assert_eq!(style.inset, 12.0);
    assert!(style.font_config(0.0).is_err());
}

use pixui_engine::{
    application::app::Application,
    components::{
        button::{ButtonComponent, ButtonProps},
        text_input::{TextInputComponent, TextInputProps},
    },
    live_model::{
        part::{ComponentPart, CompositePart, LivePart},
        state::LiveState,
    },
    painters::{button::ButtonPainter, text_input::TextInputPainter},
    ui::{
        presentation::PresentationSettings,
        render_input::{RenderInput, ResolvedCommand},
        renderer,
    },
};
fn button_part(
    app: &mut Application,
) -> (
    pixui_engine::component_registry::component_id::ComponentId<ButtonComponent>,
    StyleAssignment,
) {
    let id = app.register_component::<ButtonComponent>("button").unwrap();
    app.register_painter::<ButtonComponent>(ButtonPainter::default())
        .unwrap();
    let family = app
        .register_style("example", StyleFamily::new(body))
        .unwrap();
    (id, StyleAssignment::new(family))
}
fn draw(
    app: &Application,
    part: &LivePart,
    settings: &PresentationSettings,
) -> renderer::RenderedUi {
    renderer::render_measured(part, &mut LiveState::new(), app, settings, 0.0, None, None).unwrap()
}
#[test]
fn occurrences_bind_independent_styles_and_old_outputs_keep_their_fonts() {
    let mut app = Application::default();
    let (id, assignment) = button_part(&mut app);
    let leaf = || {
        ComponentPart::typed(id, |_, _| {
            Ok(ButtonProps {
                label: "Same text".into(),
            })
        })
    };
    let tree = LivePart::Composite(CompositePart {
        parts: vec![
            leaf().with_style("body", assignment.clone()).into(),
            leaf()
                .with_style(
                    "body",
                    assignment.clone().patch(StylePatch {
                        fill: Some(Some(Color(99, 45, 140).into())),
                        font: Some(ThemeFont::Mono),
                        font_size: Some(20.0),
                        minimum_height: Some(64.0),
                        inset: Some(12.0),
                        ..Default::default()
                    }),
                )
                .into(),
        ],
    });
    let first = draw(&app, &tree, &PresentationSettings::default());
    let input = RenderInput {
        display: &first.display_list,
        styles: &first.styles,
    };
    input.validate().unwrap();
    assert!(
        first.display_list.validate().is_err(),
        "a styled list alone must not silently drop its table"
    );
    let colors: Vec<_> = first
        .display_list
        .commands
        .iter()
        .filter_map(|command| match input.resolve(command).unwrap() {
            ResolvedCommand::FillRect { brush, .. } => Some(brush),
            _ => None,
        })
        .collect();
    assert_eq!(
        &colors[1..],
        [
            app.styles().tokens(Theme::Light).surface.into(),
            Color(99, 45, 140).into()
        ]
    );
    assert_eq!(first.display_list.fonts.len(), 2);
    assert_ne!(
        first.layout.component_bounds[0].height,
        first.layout.component_bounds[1].height
    );
    let old_styles = first.styles.clone();
    let old_font = first.display_list.fonts.iter().next().unwrap().identity();
    let mut tokens = app.styles().tokens(Theme::Light).clone();
    tokens.surface = Color(12, 23, 34);
    tokens.font = ThemeFont::Mono;
    app.set_theme_tokens(Theme::Light, tokens).unwrap();
    let changed = draw(&app, &tree, &PresentationSettings::default());
    assert_ne!(changed.styles, first.styles);
    assert_eq!(first.styles, old_styles);
    assert!(first.display_list.fonts.iter().next().unwrap().identity() == old_font);
    input.validate().unwrap();
    let dark = draw(
        &app,
        &tree,
        &PresentationSettings {
            theme: Theme::Dark,
            ..Default::default()
        },
    );
    assert_ne!(dark.styles, changed.styles);
}
#[test]
fn invalid_static_overrides_fail_before_ui_registration() {
    let mut app = Application::default();
    let (id, assignment) = button_part(&mut app);
    let part = || {
        ComponentPart::typed(id, |_, _| {
            Ok(ButtonProps {
                label: "Button".into(),
            })
        })
    };
    for tree in [
        part().with_style("missing", assignment.clone()),
        part().with_style("body", assignment.clone().variant("missing")),
        part()
            .with_style("body", assignment.clone())
            .with_style("body", assignment.clone()),
    ] {
        assert!(
            app.register_ui(pixui_engine::ui::definition::UiDefinition::new(
                "bad",
                tree.into()
            ))
            .is_err()
        );
    }
    let mut foreign = StyleRegistry::default();
    let family = foreign.register("foreign", StyleFamily::new(body)).unwrap();
    assert!(
        app.register_ui(pixui_engine::ui::definition::UiDefinition::new(
            "foreign",
            part()
                .with_style("body", StyleAssignment::new(family))
                .into()
        ))
        .is_err()
    );
}
#[test]
fn styled_input_uses_the_same_font_and_inset_for_layout_text_and_caret() {
    let mut app = Application::default();
    let id = app
        .register_component::<TextInputComponent>("input")
        .unwrap();
    app.register_painter::<TextInputComponent>(TextInputPainter::default())
        .unwrap();
    let family = app
        .register_style(
            "mono-input",
            StyleFamily::new(|tokens| {
                let mut style = PaintStyle::text(tokens);
                style.font = ThemeFont::Mono;
                style.font_size = 22.0;
                style.inset = 14.0;
                style
            }),
        )
        .unwrap();
    let tree = ComponentPart::typed(id, |_, _| {
        Ok(TextInputProps {
            content: "Wi🙂".into(),
        })
    })
    .with_style("body", StyleAssignment::new(family))
    .into();
    for scale in [1.0, 1.5, 2.0] {
        let settings = PresentationSettings {
            scale_factor: scale,
            ..Default::default()
        };
        let output = draw(&app, &tree, &settings);
        let target = &output.layout.text_inputs[0];
        let config = pixui_engine::ui::text::font::FontConfig::new(
            ThemeFont::Mono.face().unwrap(),
            22.0,
            scale,
        )
        .unwrap();
        assert_eq!(target.geometry.x(0), 0.0);
        assert!(
            (output.layout.content_bounds[0].height - (config.metrics().line_height + 28.0)).abs()
                < 0.01
        );
        let input = RenderInput {
            display: &output.display_list,
            styles: &output.styles,
        };
        input.validate().unwrap();
        let origin = output
            .display_list
            .commands
            .iter()
            .find_map(|command| match input.resolve(command).unwrap() {
                ResolvedCommand::DrawText { origin, .. } => Some(origin),
                _ => None,
            })
            .unwrap();
        assert!((origin.x - target.bounds.x - 14.0).abs() < 0.01);
        assert!(
            (target.geometry.x("Wi🙂".len()) - config.measure_text("Wi🙂").unwrap().width).abs()
                < 0.01
        );
    }
}
#[test]
fn output_validation_rejects_unbound_styles_missing_properties_and_bad_fonts() {
    use pixui_engine::ui::{
        display_list::{DisplayList, DrawCommand},
        geometry::Rect,
        render_input::RenderStyle,
        text::resource::FontIndex,
    };
    let styles = vec![RenderStyle {
        fill: None,
        border: None,
        corner_radius: 0.0,
        foreground: Color(0, 0, 0),
        font: None,
    }];
    for command in [
        DrawCommand::StyledFillRect {
            rect: Rect::default(),
            style: StyleIndex::default(),
        },
        DrawCommand::StyledFillRect {
            rect: Rect::default(),
            style: StyleIndex { index: 0 },
        },
        DrawCommand::StyledStrokeRect {
            rect: Rect::default(),
            style: StyleIndex { index: 0 },
        },
        DrawCommand::StyledText {
            origin: Default::default(),
            text: "x".into(),
            style: StyleIndex { index: 0 },
        },
    ] {
        let display = DisplayList {
            commands: vec![command],
            ..Default::default()
        };
        assert!(
            RenderInput {
                display: &display,
                styles: &styles
            }
            .validate()
            .is_err()
        );
    }
    let styles = vec![RenderStyle {
        fill: None,
        border: None,
        corner_radius: 0.0,
        foreground: Color(0, 0, 0),
        font: Some(FontIndex::from_raw(99)),
    }];
    let display = DisplayList {
        commands: vec![DrawCommand::StyledText {
            origin: Default::default(),
            text: "x".into(),
            style: StyleIndex { index: 0 },
        }],
        ..Default::default()
    };
    assert!(
        RenderInput {
            display: &display,
            styles: &styles
        }
        .validate()
        .is_err()
    );
}

#[test]
fn solver_probes_reuse_recipes_and_hover_reuses_font_snapshots_without_relayout() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static RECIPES: AtomicUsize = AtomicUsize::new(0);
    static RESOLVERS: AtomicUsize = AtomicUsize::new(0);
    let mut app = Application::default();
    let (id, _) = button_part(&mut app);
    let mut family = StyleFamily::new(|tokens| {
        RECIPES.fetch_add(1, Ordering::Relaxed);
        body(tokens)
    });
    family.hover = StatePatch {
        fill: Some(Color(20, 40, 80).into()),
        ..Default::default()
    };
    app.register_style("counted", family).unwrap();
    let tree = ComponentPart::typed(id, |_, _| {
        Ok(ButtonProps {
            label: "Counter".into(),
        })
    })
    .with_style_resolver(|context, _| {
        RESOLVERS.fetch_add(1, Ordering::Relaxed);
        Ok(vec![SlotAssignment::new(
            "body",
            StyleAssignment::new(context.application()?.styles().family("counted")?),
        )])
    })
    .into();
    RECIPES.store(0, Ordering::Relaxed);
    let first = draw(&app, &tree, &PresentationSettings::default());
    assert!(first.timings.measurements > 0);
    assert_eq!(RECIPES.load(Ordering::Relaxed), 1);
    assert_eq!(RESOLVERS.load(Ordering::Relaxed), 1);
    let hovered = renderer::render_measured(
        &tree,
        &mut LiveState::new(),
        &app,
        &PresentationSettings::default(),
        0.0,
        None,
        Some(&first.layout.component_paths[0]),
    )
    .unwrap();
    assert_eq!(RECIPES.load(Ordering::Relaxed), 2);
    assert_eq!(RESOLVERS.load(Ordering::Relaxed), 2);
    assert_eq!(
        first.layout.component_bounds,
        hovered.layout.component_bounds
    );
    assert_ne!(first.styles, hovered.styles);
    assert!(
        first.display_list.fonts.iter().next().unwrap().identity()
            == hovered.display_list.fonts.iter().next().unwrap().identity()
    );
}
#[test]
fn runtime_replacement_invalidates_visible_windows_and_defers_hidden_ones_atomically() {
    use pixui_engine::ui::{definition::UiDefinition, input::UiCommand};
    use std::time::Duration;
    let handle = Application::new();
    let id = handle
        .register_component::<ButtonComponent>("button")
        .unwrap();
    handle
        .register_painter::<ButtonComponent>(ButtonPainter::default())
        .unwrap();
    let family = handle
        .register_style("replaceable", StyleFamily::new(body))
        .unwrap();
    let tree = ComponentPart::typed(id, |_, _| {
        Ok(ButtonProps {
            label: "Keep me".into(),
        })
    })
    .with_style("body", StyleAssignment::new(family))
    .into();
    let definition = handle
        .register_ui(UiDefinition::new("styles", tree))
        .unwrap();
    let (_, visible) = handle
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let (hidden, hidden_output) = handle
        .create_ui(definition, PresentationSettings::default())
        .unwrap();
    let first = visible.recv_timeout(Duration::from_secs(2)).unwrap();
    hidden_output.recv_timeout(Duration::from_secs(2)).unwrap();
    handle
        .ui_command(UiCommand::Visibility {
            instance: hidden,
            visible: false,
        })
        .unwrap();
    let revision = handle.inspect(|app| Ok(app.styles().revision())).unwrap();
    assert!(
        handle
            .replace_style(
                family,
                StyleFamily::new(|tokens| {
                    let mut style = body(tokens);
                    style.inset = -1.0;
                    style
                })
            )
            .is_err()
    );
    assert_eq!(
        handle.inspect(|app| Ok(app.styles().revision())).unwrap(),
        revision
    );
    assert!(visible.try_recv().is_err());
    handle
        .replace_style(
            family,
            StyleFamily::new(|tokens| {
                let mut style = body(tokens);
                style.fill = Some(Color(40, 70, 100).into());
                style
            }),
        )
        .unwrap();
    let next = visible.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_ne!(first.styles, next.styles);
    assert!(hidden_output.try_recv().is_err());
    handle
        .ui_command(UiCommand::Visibility {
            instance: hidden,
            visible: true,
        })
        .unwrap();
    let shown = hidden_output.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(next.styles, shown.styles);
    RenderInput::from(&first).validate().unwrap();
    RenderInput::from(&shown).validate().unwrap();
}

#[test]
fn gradient_styles_deduplicate_preserve_old_outputs_and_accept_state_overrides() {
    use pixui_engine::styles::brush::LinearGradient;
    let mut app = Application::default();
    let (id, mut assignment) = button_part(&mut app);
    let gradient = LinearGradient::horizontal(Color(10, 20, 30), Color(200, 150, 100));
    assignment.patch.fill = Some(Some(gradient.into()));
    let tree = LivePart::Composite(CompositePart {
        parts: (0..2)
            .map(|_| {
                ComponentPart::typed(id, |_, _| {
                    Ok(ButtonProps {
                        label: "Gradient".into(),
                    })
                })
                .with_style("body", assignment.clone())
                .into()
            })
            .collect(),
    });
    let first = draw(&app, &tree, &PresentationSettings::default());
    assert_eq!(
        first
            .styles
            .iter()
            .filter(|style| style.fill == Some(gradient.into()))
            .count(),
        1
    );
    let mut hovered = assignment.clone();
    let vertical = LinearGradient::vertical(Color(1, 2, 3), Color(4, 5, 6));
    hovered.hover.fill = Some(vertical.into());
    assert_eq!(
        app.styles()
            .resolve(
                &hovered,
                Theme::Light,
                1.0,
                StyleStates {
                    hovered: true,
                    ..Default::default()
                }
            )
            .unwrap()
            .fill,
        Some(vertical.into())
    );
    let second = draw(
        &app,
        &tree,
        &PresentationSettings {
            theme: Theme::Dark,
            scale_factor: 2.0,
            ..Default::default()
        },
    );
    assert!(
        second
            .styles
            .iter()
            .any(|style| style.fill == Some(gradient.into()))
    );
    assert!(
        first
            .styles
            .iter()
            .any(|style| style.fill == Some(gradient.into()))
    );
}

#[test]
fn malformed_gradients_are_rejected_by_worker_and_render_input_validation() {
    use pixui_engine::{
        styles::brush::{Brush, LinearGradient},
        ui::{geometry::Point, render_input::RenderStyle},
    };
    let valid = LinearGradient::horizontal(Color(0, 0, 0), Color(255, 255, 255));
    for start in [
        Point {
            x: f32::NAN,
            y: 0.0,
        },
        Point {
            x: f32::INFINITY,
            y: 0.0,
        },
        Point { x: -0.1, y: 0.0 },
        Point { x: 1.1, y: 0.0 },
        valid.end,
    ] {
        let brush = Brush::LinearGradient(LinearGradient { start, ..valid });
        let mut style = PaintStyle::text(&ThemeTokens::for_theme(Theme::Light));
        style.fill = Some(brush);
        assert!(style.validate(1.0).is_err());
        let display = Default::default();
        let styles = [RenderStyle {
            fill: Some(brush),
            border: None,
            corner_radius: 0.0,
            foreground: Color(0, 0, 0),
            font: None,
        }];
        assert!(
            RenderInput {
                display: &display,
                styles: &styles
            }
            .validate()
            .is_err()
        );
    }
}

#[test]
fn corner_radius_defaults_patches_validation_and_deduplication() {
    let mut app = Application::default();
    let (id, assignment) = button_part(&mut app);
    assert_eq!(
        body(&ThemeTokens::for_theme(Theme::Light)).corner_radius,
        0.0
    );
    let tree = LivePart::Composite(CompositePart {
        parts: [0.0, 8.0, 8.0]
            .into_iter()
            .map(|radius| {
                ComponentPart::typed(id, |_, _| {
                    Ok(ButtonProps {
                        label: "Radius".into(),
                    })
                })
                .with_style(
                    "body",
                    assignment.clone().patch(StylePatch {
                        corner_radius: Some(radius),
                        ..Default::default()
                    }),
                )
                .into()
            })
            .collect(),
    });
    let first = draw(&app, &tree, &PresentationSettings::default());
    assert_eq!(
        first
            .styles
            .iter()
            .filter(|style| style.corner_radius == 8.0)
            .count(),
        1
    );
    assert!(first.styles.iter().any(|style| style.corner_radius == 0.0));
    let input = RenderInput {
        display: &first.display_list,
        styles: &first.styles,
    };
    let radii: Vec<_> = first
        .display_list
        .commands
        .iter()
        .filter_map(|command| match input.resolve(command).unwrap() {
            ResolvedCommand::FillRect { corner_radius, .. } => Some(corner_radius),
            _ => None,
        })
        .collect();
    assert_eq!(&radii[radii.len() - 3..], [0.0, 8.0, 8.0]);
    for radius in [-1.0, f32::INFINITY, f32::NAN] {
        let assignment = assignment.clone().patch(StylePatch {
            corner_radius: Some(radius),
            ..Default::default()
        });
        assert!(
            app.styles()
                .resolve(&assignment, Theme::Light, 1.0, Default::default())
                .is_err()
        );
        let display = Default::default();
        let styles = [pixui_engine::ui::render_input::RenderStyle {
            fill: None,
            border: None,
            corner_radius: radius,
            foreground: Color(0, 0, 0),
            font: None,
        }];
        assert!(
            RenderInput {
                display: &display,
                styles: &styles
            }
            .validate()
            .is_err()
        );
    }
}
