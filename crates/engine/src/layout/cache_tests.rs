//! Compare retained geometry with a fresh solve, including custom measurements.
use super::{
    adapter::{self, LayoutResult},
    cache::LayoutCache,
    container::ContainerPart,
    grid::Track,
    style::LayoutStyle,
};
use crate::{
    application::app::Application,
    expression::expression::Expression,
    live_model::{
        component::Component,
        match_part::{MatchCandidate, MatchPart, MatchPattern},
        part::{ComponentPart, ForLoopPart, LivePart},
        state::{
            ComponentState, CompositeState, ForLoopState, GenericComponentState, MatchState,
            PartState,
        },
    },
    painters::measure::{AvailableSpace, MeasureConstraints},
    ui::geometry::Size,
};
use pixui_base::{PixuiResult, pixui_error};

struct Block;
impl Component for Block {
    type Events = crate::events::interface::NoEvents;
    type Props = ();
    type State = ();
}

fn leaf() -> LivePart {
    let mut app = Application::default();
    let id = app.register_component::<Block>("block").unwrap();
    ComponentPart::typed(id, |_, _| Ok(())).into()
}

// These tests exercise the adapter after walking. Only the physical state shape
// matters here; runtime integration tests cover expression and key resolution.
fn prepared(part: &LivePart) -> PartState {
    match part {
        LivePart::Component(_) => PartState::Component(ComponentState {
            state: GenericComponentState::new(()),
        }),
        LivePart::Container(part) => PartState::Container(CompositeState {
            parts: part.children.iter().map(prepared).collect(),
        }),
        LivePart::ForLoop(part) => PartState::ForLoop(ForLoopState {
            keys: None,
            items: (0..2).map(|_| prepared(&part.body)).collect(),
        }),
        LivePart::Match(part) => PartState::Match(MatchState {
            selected: Some(0),
            part: Box::new(prepared(&part.candidates[0].part)),
        }),
        _ => panic!("unsupported test template"),
    }
}

fn measure(index: usize, constraints: MeasureConstraints, factor: f32) -> PixuiResult<Size> {
    // A constraint-sensitive custom painter, rather than a text-only intrinsic
    // size. Verifying only a single preferred size would miss these answers.
    let natural_width = match constraints.available_width {
        AvailableSpace::MinContent => 20.0,
        AvailableSpace::MaxContent => 70.0,
        AvailableSpace::Definite(width) => width.min(70.0),
    };
    Ok(Size {
        width: constraints.width.unwrap_or(natural_width * factor),
        height: constraints.height.unwrap_or((10.0 + index as f32) * factor),
    })
}

fn solve(
    cache: &mut LayoutCache,
    part: &LivePart,
    state: &PartState,
    width: f32,
    scroll: f32,
    factor: f32,
) -> LayoutResult {
    adapter::compute(
        cache,
        part,
        state,
        Size {
            width,
            height: 100.0,
        },
        scroll,
        |index, constraints| measure(index, constraints, factor),
    )
    .unwrap()
}

fn same_geometry(actual: &LayoutResult, expected: &LayoutResult) {
    let boxes = |result: &LayoutResult| {
        result
            .leaves
            .iter()
            .map(|leaf| (leaf.border, leaf.content, leaf.clip, leaf.navigation_clip))
            .collect::<Vec<_>>()
    };
    assert_eq!(boxes(actual), boxes(expected));
    assert_eq!(actual.containers, expected.containers);
    assert_eq!(actual.scroll, expected.scroll);
    assert_eq!(actual.content_height, expected.content_height);
}

fn grid() -> LivePart {
    ContainerPart::grid()
        .with_columns(vec![Track::fraction(1.0); 2])
        .with_gap(8.0)
        .with_children(
            (0..4)
                .map(|_| {
                    ContainerPart::column()
                        .with_padding(8.0)
                        .with_children(vec![leaf(), leaf()])
                        .into()
                })
                .collect(),
        )
        .into()
}

#[test]
fn unchanged_frames_validate_probes_and_reuse_solver_nodes() {
    let part = grid();
    let state = prepared(&part);
    let mut cache = LayoutCache::default();
    let first = solve(&mut cache, &part, &state, 250.0, 0.0, 1.0);
    let ids = cache.ids.clone();
    let probes: usize = cache.probes.iter().map(Vec::len).sum();
    assert!(probes > 0);
    let second = solve(&mut cache, &part, &state, 250.0, 0.0, 1.0);
    assert_eq!(cache.ids, ids);
    assert_eq!(
        second.measurements, probes,
        "only validation calls; no solver callbacks"
    );
    assert!(second.measurements <= first.measurements);
    same_geometry(&second, &first);
}

#[test]
fn changed_constraint_sensitive_measurements_invalidate_old_answers() {
    let part = grid();
    let state = prepared(&part);
    let mut cache = LayoutCache::default();
    let before = solve(&mut cache, &part, &state, 250.0, 0.0, 1.0);
    let changed = solve(&mut cache, &part, &state, 250.0, 0.0, 2.0);
    let fresh = solve(&mut LayoutCache::default(), &part, &state, 250.0, 0.0, 2.0);
    assert_ne!(changed.content_height, before.content_height);
    same_geometry(&changed, &fresh);
    let warm = solve(&mut cache, &part, &state, 250.0, 0.0, 2.0);
    assert_eq!(
        warm.measurements,
        cache.probes.iter().map(Vec::len).sum::<usize>()
    );
    same_geometry(&warm, &fresh);
}

#[test]
fn styles_and_viewport_changes_match_fresh_layout_and_scroll_reextracts_clips() {
    let mut part = grid();
    let state = prepared(&part);
    let mut cache = LayoutCache::default();
    solve(&mut cache, &part, &state, 250.0, 0.0, 2.0);
    if let LivePart::Container(container) = &mut part {
        container.gap = 20.0;
        container.style = LayoutStyle::auto().with_padding(12.0);
    }
    let changed = solve(&mut cache, &part, &state, 400.0, 0.0, 2.0);
    let fresh = solve(&mut LayoutCache::default(), &part, &state, 400.0, 0.0, 2.0);
    same_geometry(&changed, &fresh);
    let scrolled = solve(&mut cache, &part, &state, 400.0, 20.0, 2.0);
    let fresh = solve(&mut LayoutCache::default(), &part, &state, 400.0, 20.0, 2.0);
    same_geometry(&scrolled, &fresh);
    assert!(scrolled.scroll > 0.0);
}

#[test]
fn loop_growth_and_removal_rebuild_the_physical_hierarchy() {
    let part = LivePart::ForLoop(ForLoopPart {
        key: None,
        expression: Expression::text("unused after preparation").unwrap(),
        body: Box::new(leaf()),
    });
    let mut state = prepared(&part);
    let mut cache = LayoutCache::default();
    for count in [2, 5, 1, 0, 3] {
        if let (LivePart::ForLoop(part), PartState::ForLoop(state)) = (&part, &mut state) {
            state.items = (0..count).map(|_| prepared(&part.body)).collect();
        }
        let actual = solve(&mut cache, &part, &state, 250.0, 0.0, 1.0);
        let fresh = solve(&mut LayoutCache::default(), &part, &state, 250.0, 0.0, 1.0);
        assert_eq!(actual.leaves.len(), count);
        same_geometry(&actual, &fresh);
    }
}

#[test]
fn validation_failure_drops_cached_solver_state_and_recovery_matches_fresh() {
    let part = grid();
    let state = prepared(&part);
    let mut cache = LayoutCache::default();
    let before = solve(&mut cache, &part, &state, 250.0, 0.0, 1.0);
    let failure = adapter::compute(
        &mut cache,
        &part,
        &state,
        Size {
            width: 250.0,
            height: 100.0,
        },
        0.0,
        |_, _| Err(pixui_error!("measurement failed")),
    );
    assert!(failure.is_err());
    assert!(cache.ids.is_empty());
    assert!(cache.probes.is_empty());
    let recovered = solve(&mut cache, &part, &state, 250.0, 0.0, 1.0);
    same_geometry(&recovered, &before);
}

#[test]
fn match_switches_and_missing_arms_match_fresh_geometry() {
    let part = LivePart::Match(
        MatchPart::new(
            Expression::text("already resolved").unwrap(),
            vec![
                MatchCandidate {
                    pattern: MatchPattern::value(false),
                    part: leaf(),
                },
                MatchCandidate {
                    pattern: MatchPattern::value(true),
                    part: ContainerPart::column()
                        .with_padding(10.0)
                        .with_children(vec![leaf(), leaf()])
                        .into(),
                },
            ],
        )
        .unwrap(),
    );
    let mut state = prepared(&part);
    let mut cache = LayoutCache::default();
    for selected in [Some(0), Some(1), None, Some(0)] {
        if let (LivePart::Match(part), PartState::Match(state)) = (&part, &mut state) {
            state.selected = selected;
            *state.part = selected.map_or(PartState::Unknown, |index| {
                prepared(&part.candidates[index].part)
            });
        }
        let actual = solve(&mut cache, &part, &state, 250.0, 0.0, 1.0);
        let fresh = solve(&mut LayoutCache::default(), &part, &state, 250.0, 0.0, 1.0);
        same_geometry(&actual, &fresh);
    }
}

#[test]
fn validation_checks_constraints_beyond_the_first_probe() {
    let part = grid();
    let state = prepared(&part);
    let mut cache = LayoutCache::default();
    solve(&mut cache, &part, &state, 250.0, 0.0, 1.0);
    let first = cache.probes[0][0].constraints;
    let changed_constraint = cache.probes[0]
        .iter()
        .find(|probe| probe.constraints != first && probe.constraints.height.is_none())
        .unwrap()
        .constraints;
    let changed_measure = |index, constraints| {
        let mut size = measure(index, constraints, 1.0)?;
        if index == 0 && constraints == changed_constraint {
            size.height += 5.0;
        }
        Ok(size)
    };
    let viewport = Size {
        width: 250.0,
        height: 100.0,
    };
    let actual =
        adapter::compute(&mut cache, &part, &state, viewport, 0.0, changed_measure).unwrap();
    let fresh = adapter::compute(
        &mut LayoutCache::default(),
        &part,
        &state,
        viewport,
        0.0,
        changed_measure,
    )
    .unwrap();
    assert!(
        actual.measurements > fresh.measurements,
        "validation finds the changed constraint and performs a full solve"
    );
    same_geometry(&actual, &fresh);
}
