use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::expression::{context::ExpressionContext, expression::Expression};
use pixui_engine::live_model::{
    part::{ComponentPart, CompositePart, ForLoopPart, LivePart},
    state::{GenericComponentState, LiveState, PartState},
    walk::{Visitor, WalkEntry, walk},
};
use pixui_reflect::{DynamicObject, FieldIndex, Reflect};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[pixui_reflect::reflect]
mod model {
    pub struct Root {
        pub groups: Vec<Group>,
    }
    pub struct Group {
        pub items: Vec<i32>,
    }
    pub struct Tracker {
        pub drops: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }
}

struct CountVisits;
impl Visitor for CountVisits {
    fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()> {
        assert!(!matches!(entry.state, PartState::Unknown));
        if let PartState::Component(state) = entry.state
            && let Some(count) = state.state.downcast_mut::<usize>()
        {
            *count += 1;
        }
        Ok(())
    }
}

fn component() -> LivePart {
    LivePart::Component(ComponentPart::new(|_| {
        Ok(GenericComponentState::new(0usize))
    }))
}

fn composite(parts: Vec<LivePart>) -> LivePart {
    LivePart::Composite(CompositePart { parts })
}

fn for_loop(index: usize, body: LivePart) -> LivePart {
    LivePart::ForLoop(ForLoopPart {
        expression: Expression::field(FieldIndex(index)),
        body: Box::new(body),
    })
}

fn nested_template() -> LivePart {
    for_loop(
        model::Root::type_descriptor()
            .field_index("groups")
            .unwrap()
            .0,
        for_loop(
            model::Group::type_descriptor()
                .field_index("items")
                .unwrap()
                .0,
            component(),
        ),
    )
}

fn context(groups: &[&[i32]]) -> DynamicObject<'static> {
    DynamicObject::from_reflect(model::Root {
        groups: groups
            .iter()
            .map(|items| model::Group {
                items: items.to_vec(),
            })
            .collect(),
    })
}

fn counts(state: &PartState) -> Vec<Vec<usize>> {
    let PartState::ForLoop(outer) = state else {
        panic!("outer loop")
    };
    outer
        .items
        .iter()
        .map(|group| {
            let PartState::ForLoop(inner) = group else {
                panic!("inner loop")
            };
            inner
                .items
                .iter()
                .map(|item| {
                    let PartState::Component(component) = item else {
                        panic!("component")
                    };
                    *component.state.downcast_ref::<usize>().unwrap()
                })
                .collect()
        })
        .collect()
}

#[test]
fn initializes_nested_loop_bodies_independently_and_retains_state_by_position() {
    let mut tree = nested_template();
    let mut state = LiveState::new();
    assert!(matches!(state.root_state(), PartState::Unknown));
    walk(
        &mut tree,
        state.root_state_mut(),
        &ExpressionContext::from_value(&context(&[&[1, 2], &[], &[3]])),
        &mut CountVisits,
    )
    .unwrap();
    assert_eq!(counts(state.root_state()), [vec![1, 1], vec![], vec![1]]);
    walk(
        &mut tree,
        state.root_state_mut(),
        &ExpressionContext::from_value(&context(&[&[1, 2], &[], &[3]])),
        &mut CountVisits,
    )
    .unwrap();
    assert_eq!(counts(state.root_state()), [vec![2, 2], vec![], vec![2]]);
    walk(
        &mut tree,
        state.root_state_mut(),
        &ExpressionContext::from_value(&context(&[&[1, 2, 4], &[5]])),
        &mut CountVisits,
    )
    .unwrap();
    assert_eq!(counts(state.root_state()), [vec![3, 3, 1], vec![1]]);
    walk(
        &mut tree,
        state.root_state_mut(),
        &ExpressionContext::from_value(&context(&[])),
        &mut CountVisits,
    )
    .unwrap();
    assert!(counts(state.root_state()).is_empty());
}

#[test]
fn component_factories_receive_each_loop_elements_context() {
    let mut tree = for_loop(
        model::Group::type_descriptor()
            .field_index("items")
            .unwrap()
            .0,
        LivePart::Component(ComponentPart::new(|context| {
            Ok(GenericComponentState::new(
                *context.downcast_ref::<i32>().unwrap(),
            ))
        })),
    );
    let mut state = PartState::Unknown;
    let context = DynamicObject::from_reflect(model::Group {
        items: vec![10, 20],
    });
    walk(
        &mut tree,
        &mut state,
        &ExpressionContext::from_value(&context),
        &mut CountVisits,
    )
    .unwrap();
    let PartState::ForLoop(state) = state else {
        panic!("loop")
    };
    let values: Vec<_> = state
        .items
        .iter()
        .map(|state| {
            let PartState::Component(component) = state else {
                panic!("component")
            };
            *component.state.downcast_ref::<i32>().unwrap()
        })
        .collect();
    assert_eq!(values, [10, 20]);
}

struct DropState(Arc<AtomicUsize>);
impl Drop for DropState {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

fn tracked_component() -> LivePart {
    LivePart::Component(ComponentPart::new(|context| {
        let tracker = context.downcast_ref::<model::Tracker>().unwrap();
        Ok(GenericComponentState::new(DropState(tracker.drops.clone())))
    }))
}

#[test]
fn resizes_composites_and_drops_removed_or_replaced_state() {
    let drops = Arc::new(AtomicUsize::new(0));
    let context = DynamicObject::from_reflect(model::Tracker {
        drops: drops.clone(),
    });
    let mut tree = composite(vec![tracked_component(), tracked_component()]);
    let mut state = PartState::Unknown;
    walk(
        &mut tree,
        &mut state,
        &ExpressionContext::from_value(&context),
        &mut CountVisits,
    )
    .unwrap();
    let LivePart::Composite(children) = &mut tree else {
        panic!("composite")
    };
    children.parts.pop();
    walk(
        &mut tree,
        &mut state,
        &ExpressionContext::from_value(&context),
        &mut CountVisits,
    )
    .unwrap();
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    tree = component();
    walk(
        &mut tree,
        &mut state,
        &ExpressionContext::from_value(&context),
        &mut CountVisits,
    )
    .unwrap();
    assert_eq!(drops.load(Ordering::Relaxed), 2);
    let PartState::Component(component) = state else {
        panic!("component")
    };
    assert_eq!(*component.state.downcast_ref::<usize>().unwrap(), 1);
}

#[test]
fn visitor_failure_leaves_unvisited_children_unknown_and_resume_initializes_them() {
    struct FailOnce;
    impl Visitor for FailOnce {
        fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()> {
            if matches!(entry.part, LivePart::Component(_)) {
                return Err(pixui_error!("stop"));
            }
            Ok(())
        }
    }
    let mut tree = composite(vec![component(), component()]);
    let mut state = PartState::Unknown;
    let context = DynamicObject::from_reflect(());
    assert!(
        walk(
            &mut tree,
            &mut state,
            &ExpressionContext::from_value(&context),
            &mut FailOnce
        )
        .is_err()
    );
    let PartState::Composite(children) = &state else {
        panic!("composite")
    };
    assert!(matches!(children.parts[0], PartState::Component(_)));
    assert!(matches!(children.parts[1], PartState::Unknown));
    walk(
        &mut tree,
        &mut state,
        &ExpressionContext::from_value(&context),
        &mut CountVisits,
    )
    .unwrap();
    let PartState::Composite(children) = state else {
        panic!("composite")
    };
    assert!(
        children
            .parts
            .iter()
            .all(|state| matches!(state, PartState::Component(_)))
    );
}

#[test]
fn failing_factory_preserves_old_state_and_reset_reinitializes_payload() {
    let context = DynamicObject::from_reflect(());
    let mut tree = component();
    let mut state = PartState::Unknown;
    walk(
        &mut tree,
        &mut state,
        &ExpressionContext::from_value(&context),
        &mut CountVisits,
    )
    .unwrap();
    walk(
        &mut tree,
        &mut state,
        &ExpressionContext::from_value(&context),
        &mut CountVisits,
    )
    .unwrap();
    state = PartState::Unknown;
    walk(
        &mut tree,
        &mut state,
        &ExpressionContext::from_value(&context),
        &mut CountVisits,
    )
    .unwrap();
    let PartState::Component(payload) = &state else {
        panic!("component")
    };
    assert_eq!(*payload.state.downcast_ref::<usize>().unwrap(), 1);
    state = PartState::Unknown;
    tree = LivePart::Component(ComponentPart::new(|_| Err(pixui_error!("factory failed"))));
    assert!(
        walk(
            &mut tree,
            &mut state,
            &ExpressionContext::from_value(&context),
            &mut CountVisits
        )
        .is_err()
    );
    assert!(matches!(state, PartState::Unknown));
}

#[test]
fn reconciles_template_changes_made_by_the_visitor_before_descending() {
    struct Replace;
    impl Visitor for Replace {
        fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()> {
            if matches!(entry.part, LivePart::Component(_)) {
                *entry.part = composite(vec![]);
            }
            Ok(())
        }
    }
    let mut tree = component();
    let mut state = PartState::Unknown;
    walk(
        &mut tree,
        &mut state,
        &ExpressionContext::from_value(&DynamicObject::from_reflect(())),
        &mut Replace,
    )
    .unwrap();
    assert!(matches!(state, PartState::Composite(_)));
}

#[test]
fn shrinking_loop_drops_removed_body_state_and_factory_failure_preserves_existing_tree() {
    #[pixui_reflect::reflect]
    mod trackers {
        pub struct Root {
            pub items: Vec<super::model::Tracker>,
        }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let make_context = |count| {
        DynamicObject::from_reflect(trackers::Root {
            items: (0..count)
                .map(|_| model::Tracker {
                    drops: drops.clone(),
                })
                .collect(),
        })
    };
    let mut tree = for_loop(
        trackers::Root::type_descriptor()
            .field_index("items")
            .unwrap()
            .0,
        tracked_component(),
    );
    let mut state = PartState::Unknown;
    walk(
        &mut tree,
        &mut state,
        &ExpressionContext::from_value(&make_context(3)),
        &mut CountVisits,
    )
    .unwrap();
    walk(
        &mut tree,
        &mut state,
        &ExpressionContext::from_value(&make_context(1)),
        &mut CountVisits,
    )
    .unwrap();
    assert_eq!(drops.load(Ordering::Relaxed), 2);
    tree = LivePart::Component(ComponentPart::new(|_| Err(pixui_error!("factory failed"))));
    assert!(
        walk(
            &mut tree,
            &mut state,
            &ExpressionContext::from_value(&make_context(1)),
            &mut CountVisits
        )
        .is_err()
    );
    let PartState::ForLoop(items) = &state else {
        panic!("old state retained")
    };
    assert_eq!(items.items.len(), 1);
    assert_eq!(drops.load(Ordering::Relaxed), 2);
    drop(state);
    assert_eq!(drops.load(Ordering::Relaxed), 3);
}

#[test]
fn collection_loop_expressions_preserve_application_access_inside_nested_loops() {
    use pixui_engine::application::{
        app::Application, application_slice::ApplicationSlice, collection::Collection,
    };
    let mut application = Application::default();
    let mut slice = ApplicationSlice::new("data");
    slice
        .add_collection(Collection::new_reflected::<model::Group>("groups"))
        .unwrap();
    slice
        .add_collection(Collection::new_reflected::<i32>("extras"))
        .unwrap();
    let groups = slice.collection_mut::<model::Group>("groups").unwrap();
    groups.insert(model::Group { items: vec![1, 2] });
    groups.insert(model::Group { items: vec![3] });
    slice.collection_mut::<i32>("extras").unwrap().insert(99);
    let id = application.add_slice(slice).unwrap();
    let nested_field = for_loop(
        model::Group::type_descriptor()
            .field_index("items")
            .unwrap()
            .0,
        component(),
    );
    let nested_collection = LivePart::ForLoop(ForLoopPart {
        expression: Expression::collection(id, 1),
        body: Box::new(component()),
    });
    let mut tree = LivePart::ForLoop(ForLoopPart {
        expression: Expression::collection(id, 0),
        body: Box::new(composite(vec![nested_field, nested_collection])),
    });
    struct Collect(Vec<i32>);
    impl Visitor for Collect {
        fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()> {
            if matches!(entry.part, LivePart::Component(_)) {
                self.0
                    .push(*entry.context.value()?.downcast_ref::<i32>().unwrap());
            }
            Ok(())
        }
    }
    let mut visitor = Collect(vec![]);
    let mut state = PartState::Unknown;
    walk(
        &mut tree,
        &mut state,
        &ExpressionContext::new(&application),
        &mut visitor,
    )
    .unwrap();
    assert_eq!(visitor.0, [1, 2, 99, 3, 99]);
    let PartState::ForLoop(outer) = state else {
        panic!("outer loop")
    };
    assert_eq!(outer.items.len(), 2);
}

#[test]
fn loop_reports_missing_expression_inputs_without_creating_body_state() {
    use pixui_engine::application::application_slice::ApplicationSlice;
    let mut tree = LivePart::ForLoop(ForLoopPart {
        expression: Expression::collection(ApplicationSlice::new("absent").id(), 0),
        body: Box::new(component()),
    });
    let mut state = PartState::Unknown;
    let value = DynamicObject::from_reflect(());
    assert!(
        walk(
            &mut tree,
            &mut state,
            &ExpressionContext::from_value(&value),
            &mut CountVisits
        )
        .is_err()
    );
    let PartState::ForLoop(loop_state) = state else {
        panic!("loop")
    };
    assert!(loop_state.items.is_empty());
}
