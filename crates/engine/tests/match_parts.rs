use pixui_base::PixuiResult;
use pixui_engine::{
    expression::{context::ExpressionContext, expression::Expression},
    live_model::{
        match_part::{MatchCandidate, MatchPart, MatchPattern},
        part::{ComponentPart, ForLoopPart, LivePart},
        state::PartState,
        walk::{Visitor, WalkEntry, walk},
    },
};
use pixui_reflect::{DynamicObject, Reflect};

#[pixui_reflect::reflect]
mod model {
    pub struct Item {
        pub selected: bool,
    }
    pub struct Root {
        pub items: Vec<Item>,
    }
}

#[derive(Default)]
struct Visits {
    components: usize,
}
impl Visitor for Visits {
    fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()> {
        if matches!(entry.part, LivePart::Component(_)) {
            assert!(
                entry
                    .context
                    .value()?
                    .downcast_ref::<model::Item>()
                    .is_some()
            );
            self.components += 1;
        }
        Ok(())
    }
}
fn conditional(patterns: Vec<MatchPattern>) -> LivePart {
    LivePart::Match(
        MatchPart::new(
            Expression::field(
                model::Item::type_descriptor()
                    .field_index("selected")
                    .unwrap(),
            ),
            patterns
                .into_iter()
                .map(|pattern| MatchCandidate {
                    pattern,
                    part: LivePart::Component(ComponentPart::default()),
                })
                .collect(),
        )
        .unwrap(),
    )
}
fn run(root: &mut LivePart, state: &mut PartState, item: &model::Item) -> usize {
    let value = DynamicObject::from_ref(item);
    let mut visitor = Visits::default();
    walk(
        root,
        state,
        &ExpressionContext::from_value(&value),
        &mut visitor,
    )
    .unwrap();
    visitor.components
}
#[test]
fn first_arm_wins_and_switching_or_losing_selection_discards_state() {
    let mut root = conditional(vec![
        MatchPattern::value(false),
        MatchPattern::value(false),
        MatchPattern::value(true),
    ]);
    let mut state = PartState::Unknown;
    assert_eq!(
        run(&mut root, &mut state, &model::Item { selected: false }),
        1
    );
    let PartState::Match(selected) = &mut state else {
        panic!()
    };
    assert_eq!(selected.selected, Some(0));
    let PartState::Component(component) = &mut *selected.part else {
        panic!()
    };
    component.state = pixui_engine::live_model::state::GenericComponentState::new(42usize);
    run(&mut root, &mut state, &model::Item { selected: false });
    let PartState::Match(selected) = &state else {
        panic!()
    };
    let PartState::Component(component) = &*selected.part else {
        panic!()
    };
    assert_eq!(component.state.downcast_ref::<usize>(), Some(&42));
    run(&mut root, &mut state, &model::Item { selected: true });
    let PartState::Match(selected) = &state else {
        panic!()
    };
    assert_eq!(selected.selected, Some(2));
    let PartState::Component(component) = &*selected.part else {
        panic!()
    };
    assert!(component.state.downcast_ref::<usize>().is_none());
    let LivePart::Match(part) = &mut root else {
        panic!()
    };
    part.candidates.truncate(2);
    assert_eq!(
        run(&mut root, &mut state, &model::Item { selected: true }),
        0
    );
    let PartState::Match(selected) = &state else {
        panic!()
    };
    assert_eq!(selected.selected, None);
    assert!(matches!(*selected.part, PartState::Unknown));
    assert_eq!(
        run(&mut root, &mut state, &model::Item { selected: false }),
        1
    );
}
#[test]
fn loops_match_independently_and_preserve_item_context() {
    let mut root = LivePart::ForLoop(ForLoopPart {
        expression: Expression::field(model::Root::type_descriptor().field_index("items").unwrap()),
        body: Box::new(conditional(vec![MatchPattern::value(false)])),
    });
    let value = DynamicObject::from_reflect(model::Root {
        items: vec![
            model::Item { selected: false },
            model::Item { selected: true },
            model::Item { selected: false },
        ],
    });
    let mut state = PartState::Unknown;
    let mut visitor = Visits::default();
    walk(
        &mut root,
        &mut state,
        &ExpressionContext::from_value(&value),
        &mut visitor,
    )
    .unwrap();
    assert_eq!(visitor.components, 2);
    let PartState::ForLoop(state) = state else {
        panic!()
    };
    assert_eq!(state.items.len(), 3);
}
#[test]
fn wildcard_empty_candidates_and_wrong_types() {
    let mut root = conditional(vec![MatchPattern::Wildcard]);
    assert_eq!(
        run(
            &mut root,
            &mut PartState::Unknown,
            &model::Item { selected: true }
        ),
        1
    );
    let mut root = conditional(vec![]);
    assert_eq!(
        run(
            &mut root,
            &mut PartState::Unknown,
            &model::Item { selected: true }
        ),
        0
    );
    let mut root = conditional(vec![MatchPattern::value(1i32), MatchPattern::Wildcard]);
    let value = DynamicObject::from_reflect(model::Item { selected: false });
    assert!(
        walk(
            &mut root,
            &mut PartState::Unknown,
            &ExpressionContext::from_value(&value),
            &mut Visits::default()
        )
        .is_err()
    );
}
#[test]
fn invalid_configuration_and_visitor_edits_are_rejected() {
    let expression = Expression::field(pixui_reflect::FieldIndex(0));
    for patterns in [
        vec![MatchPattern::Wildcard, MatchPattern::Wildcard],
        vec![MatchPattern::value(false), MatchPattern::value(0i32)],
    ] {
        assert!(
            MatchPart::new(
                expression.clone(),
                patterns
                    .into_iter()
                    .map(|pattern| MatchCandidate {
                        pattern,
                        part: LivePart::Component(ComponentPart::default())
                    })
                    .collect()
            )
            .is_err()
        );
    }
    struct Edit;
    impl Visitor for Edit {
        fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()> {
            if let LivePart::Match(part) = entry.part {
                part.expression = Expression::field(pixui_reflect::FieldIndex(99));
            }
            Ok(())
        }
    }
    let mut root = conditional(vec![MatchPattern::Wildcard]);
    let value = DynamicObject::from_reflect(model::Item { selected: false });
    assert!(
        walk(
            &mut root,
            &mut PartState::Unknown,
            &ExpressionContext::from_value(&value),
            &mut Edit
        )
        .is_err()
    );
}

#[test]
fn selector_is_read_once_and_branch_payload_is_dropped_on_unmount() {
    use std::sync::{
        Arc, OnceLock,
        atomic::{AtomicUsize, Ordering},
    };
    struct Counted {
        selected: bool,
        reads: Arc<AtomicUsize>,
        drops: Arc<AtomicUsize>,
    }
    impl Reflect for Counted {
        fn type_descriptor() -> &'static pixui_reflect::TypeDescriptor {
            static DESCRIPTOR: OnceLock<pixui_reflect::TypeDescriptor> = OnceLock::new();
            DESCRIPTOR.get_or_init(|| {
                pixui_reflect::TypeDescriptor::new::<Self>(
                    vec![pixui_reflect::Field::reflected(
                        "selected",
                        |value: &Self| {
                            value.reads.fetch_add(1, Ordering::Relaxed);
                            &value.selected
                        },
                    )],
                    vec![],
                )
                .unwrap()
            })
        }
    }
    struct Payload(Arc<AtomicUsize>);
    impl Drop for Payload {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    fn initialize(
        value: &DynamicObject<'_>,
    ) -> PixuiResult<pixui_engine::live_model::state::GenericComponentState> {
        Ok(pixui_engine::live_model::state::GenericComponentState::new(
            Payload(value.downcast_ref::<Counted>().unwrap().drops.clone()),
        ))
    }
    struct Noop;
    impl Visitor for Noop {
        fn visit(&mut self, _: &mut WalkEntry) -> PixuiResult<()> {
            Ok(())
        }
    }
    let reads = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let mut value = Counted {
        selected: false,
        reads: reads.clone(),
        drops: drops.clone(),
    };
    let mut root = LivePart::Match(
        MatchPart::new(
            Expression::field(pixui_reflect::FieldIndex(0)),
            vec![MatchCandidate {
                pattern: MatchPattern::value(false),
                part: LivePart::Component(ComponentPart::new(initialize)),
            }],
        )
        .unwrap(),
    );
    let mut state = PartState::Unknown;
    for selected in [false, false, true, false] {
        value.selected = selected;
        let object = DynamicObject::from_ref(&value);
        walk(
            &mut root,
            &mut state,
            &ExpressionContext::from_value(&object),
            &mut Noop,
        )
        .unwrap();
    }
    assert_eq!(reads.load(Ordering::Relaxed), 4);
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    drop(state);
    assert_eq!(drops.load(Ordering::Relaxed), 2);
}

#[test]
fn nested_matches_keep_context_and_loop_inside_match_is_walked() {
    let mut root = conditional(vec![MatchPattern::Wildcard]);
    let LivePart::Match(part) = &mut root else {
        panic!()
    };
    part.candidates[0].part = conditional(vec![MatchPattern::value(false)]);
    assert_eq!(
        run(
            &mut root,
            &mut PartState::Unknown,
            &model::Item { selected: false }
        ),
        1
    );
    let loop_part = LivePart::ForLoop(ForLoopPart {
        expression: Expression::field(model::Root::type_descriptor().field_index("items").unwrap()),
        body: Box::new(conditional(vec![MatchPattern::Wildcard])),
    });
    let mut root = LivePart::Match(
        MatchPart::new(
            Expression::field(pixui_reflect::FieldIndex(0)),
            vec![MatchCandidate {
                pattern: MatchPattern::Wildcard,
                part: loop_part,
            }],
        )
        .unwrap(),
    );
    let value = DynamicObject::from_reflect(model::Root {
        items: vec![model::Item { selected: false }],
    });
    let mut visitor = Visits::default();
    walk(
        &mut root,
        &mut PartState::Unknown,
        &ExpressionContext::from_value(&value),
        &mut visitor,
    )
    .unwrap();
    assert_eq!(visitor.components, 1);
}
