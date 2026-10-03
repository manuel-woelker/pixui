use crate::live_model::part::LivePart;
use pixui_base::PixuiResult;
use pixui_reflect::{DynamicObject, FieldIndex};

pub struct Walk {}

pub struct WalkEntry<'part, 'context> {
    pub part: &'part mut LivePart,
    /// Root context, or the innermost loop's current element.
    pub context: &'context DynamicObject<'context>,
}

pub trait Visitor {
    fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()>;
}

/// Visits parts in depth-first order, preserving composite and sequence order.
///
/// A loop is visited once in its enclosing context. Its body is then visited
/// for each element using that element as context; an empty sequence skips the
/// body. Nested loop indices belong to the enclosing element's descriptor.
/// The same mutable body is reused each iteration, so visitor edits persist.
/// Reflection and visitor errors stop traversal without rolling back edits.
pub fn walk<V: Visitor>(
    root: &mut LivePart,
    context: &DynamicObject<'_>,
    visitor: &mut V,
) -> PixuiResult<()> {
    let mut stack = vec![root];
    while let Some(part) = stack.pop() {
        visitor.visit(&mut WalkEntry { part, context })?;
        match part {
            LivePart::Composite(composite) => stack.extend(composite.parts.iter_mut().rev()),
            LivePart::Component(_) => {}
            LivePart::ForLoop(for_loop) => {
                let sequence = context.read_object(FieldIndex(for_loop.field_index))?;
                for index in 0..sequence.len()? {
                    let item = sequence.get(index)?;
                    walk(&mut for_loop.body, &item, visitor)?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use expect_test::expect;

    use super::{Visitor, WalkEntry, walk};
    use crate::live_model::part::{CompositePart, ForLoopPart, LivePart};
    use pixui_base::PixuiResult;
    use pixui_reflect::{DynamicObject, Reflect};

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
                LivePart::Component(_) => self.output.push_str("Component\n"),
                LivePart::ForLoop(_) => self.output.push_str("ForLoop\n"),
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
            LivePart::Component(crate::live_model::part::ComponentPart {}),
            composite(vec![
                composite(vec![]),
                LivePart::Component(crate::live_model::part::ComponentPart {}),
            ]),
            composite(vec![LivePart::Component(
                crate::live_model::part::ComponentPart {},
            )]),
        ]);
        let mut visitor = CollectingVisitor::default();

        walk(
            &mut root,
            &DynamicObject::from_reflect(model::Root {
                marker: 0,
                groups: vec![],
            }),
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
            field_index,
            body: Box::new(body),
        })
    }

    fn component() -> LivePart {
        LivePart::Component(crate::live_model::part::ComponentPart {})
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
            let context = if let Some(group) = entry.context.downcast_ref::<model::Group>() {
                format!("group {}", group.id)
            } else if let Some(item) = entry.context.downcast_ref::<model::Item>() {
                self.item_values.push(item.value);
                format!("item {}", item.value)
            } else {
                "root".into()
            };
            let kind = match entry.part {
                LivePart::Composite(_) => "Composite",
                LivePart::Component(_) => "Component",
                LivePart::ForLoop(_) => "ForLoop",
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
        walk(&mut loop_tree(), &context, &mut visitor).unwrap();
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
        walk(&mut loop_tree(), &context, &mut visitor).unwrap();
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
            assert!(walk(&mut root, &context, &mut visitor).is_err());
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
                if let Some(group) = entry.context.downcast_ref::<model::Group>() {
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
        let error = walk(&mut for_loop(index, component()), &context, &mut visitor).unwrap_err();
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
                if entry.context.downcast_ref::<model::Group>().is_some() {
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
        walk(&mut root, &context, &mut visitor).unwrap();
        assert_eq!(visitor.body_visits, 2);
        let LivePart::ForLoop(for_loop) = root else {
            panic!("expected loop")
        };
        assert!(matches!(*for_loop.body, LivePart::Composite(_)));
    }
}
