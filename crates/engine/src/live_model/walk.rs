use super::identity::{ComponentPath, ItemKey, PathSegment};
use crate::expression::{context::ExpressionContext, evaluator::evaluate};
use crate::live_model::{
    part::LivePart,
    state::{ComponentState, CompositeState, ForLoopState, MatchState, PartState},
};
use pixui_base::{PixuiResult, pixui_error};
use pixui_reflect::DynamicObject;

pub struct Walk {}

pub struct WalkEntry<'part, 'context> {
    pub part: &'part mut LivePart,
    /// Persistent state for this physical node, initialized before visitation.
    pub state: &'part mut PartState,
    /// Structural occurrence identity in this traversal.
    pub path: ComponentPath,
    /// Expression inputs: optional application and current root or loop element.
    pub context: &'context ExpressionContext<'context>,
}

pub trait Visitor {
    fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()>;
}

/// Walks the template and updates its physical state in depth-first sequence order.
///
/// Unknown and mismatched state variants are initialized when reached. Composite
/// children retain state by position. Keyed loop items retain state by key; other
/// loop items retain state by position. New entries start Unknown; removed entries
/// are dropped. Each loop item has an independent body
/// state, even though all items reuse the same mutable template body.
/// Loop expressions can select application collections or fields of the current
/// value. Nested loops replace that value while retaining application access.
///
/// Visitors see initialized state. Template edits are reconciled again after the
/// visit, before descent. Loop item counts are reconciled after resolving the
/// sequence. Errors stop traversal without rollback; unvisited entries can remain
/// Unknown. Collection loops use generational arena keys automatically. Other
/// loops can provide immutable keys or retain positional identity. Loop nesting
/// remains recursive, while composites and matches use a stack.
/// Match visitors see the previous selection; selection is refreshed once after
/// visiting, before descending with the unchanged context. Only the active arm
/// retains state. Switching or losing selection drops its previous subtree.
pub fn walk<V: Visitor>(
    root: &mut LivePart,
    root_state: &mut PartState,
    context: &ExpressionContext<'_>,
    visitor: &mut V,
) -> PixuiResult<()> {
    walk_at(root, root_state, context, visitor, ComponentPath::default())
}

fn walk_at<V: Visitor>(
    root: &mut LivePart,
    root_state: &mut PartState,
    context: &ExpressionContext<'_>,
    visitor: &mut V,
    root_path: ComponentPath,
) -> PixuiResult<()> {
    let mut stack = vec![(root, root_state, root_path)];
    while let Some((part, state, path)) = stack.pop() {
        reconcile(part, state, context)?;
        visitor.visit(&mut WalkEntry {
            part,
            state,
            context,
            path: path.clone(),
        })?;
        reconcile(part, state, context)?;
        match (part, state) {
            (LivePart::Composite(composite), PartState::Composite(state)) => {
                stack.extend(
                    composite
                        .parts
                        .iter_mut()
                        .zip(state.parts.iter_mut())
                        .enumerate()
                        .rev()
                        .map(|(index, (part, state))| {
                            (part, state, path.child(PathSegment::Child(index)))
                        }),
                );
            }
            (LivePart::Container(container), PartState::Container(state)) => {
                stack.extend(
                    container
                        .children
                        .iter_mut()
                        .zip(state.parts.iter_mut())
                        .enumerate()
                        .rev()
                        .map(|(index, (part, state))| {
                            (part, state, path.child(PathSegment::Child(index)))
                        }),
                );
            }
            (LivePart::Component(_), PartState::Component(_)) => {}
            (LivePart::Match(match_part), PartState::Match(state)) => {
                let selected = match_part.select(&evaluate(context, &match_part.expression)?)?;
                if selected != state.selected {
                    *state.part = PartState::Unknown;
                    state.selected = selected;
                }
                if let Some(index) = selected {
                    stack.push((
                        &mut match_part.candidates[index].part,
                        &mut state.part,
                        path.child(PathSegment::MatchArm(index)),
                    ));
                }
            }

            (LivePart::ForLoop(for_loop), PartState::ForLoop(state)) => {
                let sequence = evaluate(context, &for_loop.expression)?;
                let items = sequence.iter()?.collect::<Vec<_>>();
                let keys = if let Some(key) = for_loop.key {
                    Some(
                        items
                            .iter()
                            .map(|item| key(&context.with_value(item)))
                            .collect::<PixuiResult<Vec<_>>>()?,
                    )
                } else if let crate::expression::expression::ExpressionKind::Collection(index) =
                    for_loop.expression.kind()
                {
                    Some(
                        context
                            .application()?
                            .resolve_collection(*index)?
                            .sequence_keys()?
                            .into_iter()
                            .map(ItemKey::Arena)
                            .collect(),
                    )
                } else {
                    None
                };
                reconcile_items(state, keys, items.len())?;
                for (index, (item, item_state)) in
                    items.iter().zip(state.items.iter_mut()).enumerate()
                {
                    let segment = match &state.keys {
                        Some(keys) => PathSegment::LoopKey(keys[index].clone()),
                        None => PathSegment::LoopItem(index),
                    };
                    walk_at(
                        &mut for_loop.body,
                        item_state,
                        &context.with_value(item),
                        visitor,
                        path.child(segment),
                    )?;
                }
            }
            _ => unreachable!("state reconciled with template"),
        }
    }
    Ok(())
}

/// Validate keys before moving state, so duplicate failures retain the old entries.
fn reconcile_items(
    state: &mut ForLoopState,
    keys: Option<Vec<ItemKey>>,
    len: usize,
) -> PixuiResult<()> {
    if let Some(keys) = &keys
        && (keys.len() != len || keys.iter().collect::<std::collections::HashSet<_>>().len() != len)
    {
        return Err(pixui_error!("duplicate or mismatched loop item keys"));
    }
    match (&state.keys, &keys) {
        (Some(_), Some(_)) => {
            let previous_keys = state.keys.take().expect("keyed state");
            let mut previous: std::collections::HashMap<_, _> = previous_keys
                .into_iter()
                .zip(std::mem::take(&mut state.items))
                .collect();
            state.items = keys
                .as_ref()
                .expect("keyed loop")
                .iter()
                .map(|key| previous.remove(key).unwrap_or_default())
                .collect();
        }
        (None, None) => state.items.resize_with(len, || PartState::Unknown),
        _ => state.items = (0..len).map(|_| PartState::Unknown).collect(),
    }
    state.keys = keys;
    Ok(())
}

/// Initialize before assignment so a failing component factory leaves old state intact.
fn reconcile(
    part: &LivePart,
    state: &mut PartState,
    context: &ExpressionContext<'_>,
) -> PixuiResult<()> {
    match part {
        LivePart::Component(component) => {
            if !matches!(state, PartState::Component(existing) if existing.state.component_address() == component.component_address())
            {
                *state = PartState::Component(ComponentState {
                    state: if let Some(address) = component.component_address() {
                        context.application()?.components().initialize(address)?
                    } else {
                        match context.value() {
                            Ok(value) => (component.create_state)(value)?,
                            Err(_) => (component.create_state)(&DynamicObject::from_reflect(()))?,
                        }
                    },
                });
            }
        }
        LivePart::Composite(composite) => {
            if !matches!(state, PartState::Composite(_)) {
                *state = PartState::Composite(CompositeState::default());
            }
            let PartState::Composite(state) = state else {
                unreachable!()
            };
            state
                .parts
                .resize_with(composite.parts.len(), || PartState::Unknown);
        }
        LivePart::Container(container) => {
            if !matches!(state, PartState::Container(_)) {
                *state = PartState::Container(CompositeState::default());
            }
            let PartState::Container(state) = state else {
                unreachable!()
            };
            state
                .parts
                .resize_with(container.children.len(), || PartState::Unknown);
        }
        LivePart::Match(part) => {
            part.validate()?;
            if !matches!(state, PartState::Match(_)) {
                *state = PartState::Match(MatchState::default());
            }
        }
        LivePart::ForLoop(_) => {
            if !matches!(state, PartState::ForLoop(_)) {
                *state = PartState::ForLoop(ForLoopState::default());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::expression::context::ExpressionContext;
    use expect_test::expect;

    use super::{Visitor, WalkEntry, walk};
    use crate::expression::expression::Expression;
    use crate::live_model::part::{CompositePart, ForLoopPart, LivePart};
    use crate::live_model::state::PartState;
    use pixui_base::PixuiResult;
    use pixui_reflect::{DynamicObject, FieldIndex, Reflect};

    #[derive(Default)]
    struct CollectingVisitor {
        output: String,
    }

    impl Visitor for CollectingVisitor {
        fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()> {
            use std::fmt::Write;

            match &*entry.part {
                LivePart::Composite(part) => {
                    writeln!(self.output, "Composite({} children)", part.parts.len()).unwrap();
                }
                LivePart::Container(_) => self.output.push_str("Container\n"),
                LivePart::Component(_) => self.output.push_str("Component\n"),
                LivePart::ForLoop(_) => self.output.push_str("ForLoop\n"),
                LivePart::Match(_) => self.output.push_str("Match\n"),
            }
            Ok(())
        }
    }

    fn composite(parts: Vec<LivePart>) -> LivePart {
        LivePart::Composite(CompositePart { parts })
    }

    #[test]
    fn visits_tree_depth_first_in_child_order() {
        let mut root = composite(vec![
            LivePart::Component(crate::live_model::part::ComponentPart::default()),
            composite(vec![
                composite(vec![]),
                LivePart::Component(crate::live_model::part::ComponentPart::default()),
            ]),
            composite(vec![LivePart::Component(
                crate::live_model::part::ComponentPart::default(),
            )]),
        ]);
        let mut visitor = CollectingVisitor::default();

        walk(
            &mut root,
            &mut PartState::Unknown,
            &ExpressionContext::from_value(&DynamicObject::from_reflect(model::Root {
                marker: 0,
                groups: vec![],
            })),
            &mut visitor,
        )
        .unwrap();

        expect![[r#"
            Composite(3 children)
            Component
            Composite(2 children)
            Composite(0 children)
            Component
            Composite(1 children)
            Component
        "#]]
        .assert_eq(&visitor.output);
    }

    #[pixui_reflect::reflect]
    mod model {
        pub struct Root {
            pub marker: i32,
            pub groups: Vec<Group>,
        }
        pub struct Group {
            pub id: i32,
            pub items: Vec<Item>,
        }
        pub struct Item {
            pub value: i32,
        }
    }

    fn for_loop(field_index: usize, body: LivePart) -> LivePart {
        LivePart::ForLoop(ForLoopPart {
            key: None,
            expression: Expression::field(FieldIndex(field_index)),
            body: Box::new(body),
        })
    }

    fn component() -> LivePart {
        LivePart::Component(crate::live_model::part::ComponentPart::default())
    }

    fn loop_tree() -> LivePart {
        let groups = model::Root::type_descriptor()
            .field_index("groups")
            .unwrap()
            .0;
        let items = model::Group::type_descriptor()
            .field_index("items")
            .unwrap()
            .0;
        composite(vec![
            for_loop(
                groups,
                composite(vec![component(), for_loop(items, component())]),
            ),
            component(),
        ])
    }

    #[derive(Default)]
    struct ContextVisitor {
        output: String,
        item_values: Vec<i32>,
    }

    impl Visitor for ContextVisitor {
        fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()> {
            use std::fmt::Write;
            let context = if let Some(group) = entry.context.value()?.downcast_ref::<model::Group>()
            {
                format!("group {}", group.id)
            } else if let Some(item) = entry.context.value()?.downcast_ref::<model::Item>() {
                self.item_values.push(item.value);
                format!("item {}", item.value)
            } else {
                "root".into()
            };
            let kind = match entry.part {
                LivePart::Composite(_) => "Composite",
                LivePart::Container(_) => "Container",
                LivePart::Component(_) => "Component",
                LivePart::ForLoop(_) => "ForLoop",
                LivePart::Match(_) => "Match",
            };
            writeln!(self.output, "{kind}: {context}").unwrap();
            Ok(())
        }
    }

    #[test]
    fn nested_loops_visit_each_element_in_order_and_restore_parent_context() {
        let context = DynamicObject::from_reflect(model::Root {
            marker: 0,
            groups: vec![
                model::Group {
                    id: 1,
                    items: vec![model::Item { value: 10 }, model::Item { value: 11 }],
                },
                model::Group {
                    id: 2,
                    items: vec![],
                },
                model::Group {
                    id: 3,
                    items: vec![model::Item { value: 30 }],
                },
            ],
        });
        let mut visitor = ContextVisitor::default();
        walk(
            &mut loop_tree(),
            &mut PartState::Unknown,
            &ExpressionContext::from_value(&context),
            &mut visitor,
        )
        .unwrap();
        expect![[r#"
            Composite: root
            ForLoop: root
            Composite: group 1
            Component: group 1
            ForLoop: group 1
            Component: item 10
            Component: item 11
            Composite: group 2
            Component: group 2
            ForLoop: group 2
            Composite: group 3
            Component: group 3
            ForLoop: group 3
            Component: item 30
            Component: root
        "#]]
        .assert_eq(&visitor.output);
        assert_eq!(visitor.item_values, [10, 11, 30]);
    }

    #[test]
    fn empty_sequence_visits_loop_but_skips_body() {
        let root = model::Root {
            marker: 0,
            groups: vec![],
        };
        let context = DynamicObject::from_ref(&root);
        let mut visitor = ContextVisitor::default();
        walk(
            &mut loop_tree(),
            &mut PartState::Unknown,
            &ExpressionContext::from_value(&context),
            &mut visitor,
        )
        .unwrap();
        expect![[r#"
            Composite: root
            ForLoop: root
            Component: root
        "#]]
        .assert_eq(&visitor.output);
    }

    #[test]
    fn invalid_and_non_sequence_fields_stop_traversal() {
        let context = DynamicObject::from_reflect(model::Root {
            marker: 0,
            groups: vec![],
        });
        for index in [0, 99] {
            let mut visitor = ContextVisitor::default();
            let mut root = composite(vec![for_loop(index, component()), component()]);
            assert!(
                walk(
                    &mut root,
                    &mut PartState::Unknown,
                    &ExpressionContext::from_value(&context),
                    &mut visitor
                )
                .is_err()
            );
            assert_eq!(visitor.output, "Composite: root\nForLoop: root\n");
        }
    }

    #[test]
    fn visitor_error_stops_later_iterations_and_siblings() {
        struct FailingVisitor {
            visits: Vec<i32>,
        }
        impl Visitor for FailingVisitor {
            fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()> {
                if let Some(group) = entry.context.value()?.downcast_ref::<model::Group>() {
                    self.visits.push(group.id);
                    if group.id == 2 {
                        return Err(pixui_base::message("stop"));
                    }
                }
                Ok(())
            }
        }
        let context = DynamicObject::from_reflect(model::Root {
            marker: 0,
            groups: vec![
                model::Group {
                    id: 1,
                    items: vec![],
                },
                model::Group {
                    id: 2,
                    items: vec![],
                },
                model::Group {
                    id: 3,
                    items: vec![],
                },
            ],
        });
        let mut visitor = FailingVisitor { visits: vec![] };
        let index = context.field_index("groups").unwrap().0;
        let error = walk(
            &mut for_loop(index, component()),
            &mut PartState::Unknown,
            &ExpressionContext::from_value(&context),
            &mut visitor,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "stop");
        assert_eq!(visitor.visits, [1, 2]);
    }

    #[test]
    fn loop_body_edits_persist_between_iterations() {
        // Replace only once to avoid repeatedly expanding a newly inserted child.
        struct OnceVisitor {
            body_visits: usize,
        }
        impl Visitor for OnceVisitor {
            fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()> {
                if entry
                    .context
                    .value()?
                    .downcast_ref::<model::Group>()
                    .is_some()
                {
                    self.body_visits += 1;
                    if self.body_visits == 1 {
                        *entry.part = composite(vec![]);
                    } else {
                        assert!(matches!(entry.part, LivePart::Composite(_)));
                    }
                }
                Ok(())
            }
        }
        let context = DynamicObject::from_reflect(model::Root {
            marker: 0,
            groups: vec![
                model::Group {
                    id: 1,
                    items: vec![],
                },
                model::Group {
                    id: 2,
                    items: vec![],
                },
            ],
        });
        let mut root = for_loop(context.field_index("groups").unwrap().0, component());
        let mut visitor = OnceVisitor { body_visits: 0 };
        walk(
            &mut root,
            &mut PartState::Unknown,
            &ExpressionContext::from_value(&context),
            &mut visitor,
        )
        .unwrap();
        assert_eq!(visitor.body_visits, 2);
        let LivePart::ForLoop(for_loop) = root else {
            panic!("expected loop")
        };
        assert!(matches!(*for_loop.body, LivePart::Composite(_)));
    }
}
