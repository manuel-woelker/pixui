//! Identity, focus eligibility and publication contracts without native windows.
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::{
        action::action, app::Application, application_slice::ApplicationSlice,
        object_ref::ObjectRef,
    },
    component_registry::component_id::ComponentId,
    expression::{context::ExpressionContext, expression::Expression},
    layout::{container::ContainerPart, style::LayoutStyle},
    live_model::{
        component::Component,
        identity::{ComponentPath, ItemKey, PathSegment},
        match_part::{MatchCandidate, MatchPart, MatchPattern},
        part::{ComponentPart, CompositePart, ForLoopPart, LivePart},
        state::{GenericComponentState, PartState},
        walk::{Visitor, WalkEntry, walk},
    },
    painters::{context::PaintContext, measure::MeasureContext, painter::Painter},
    ui::{
        activation::ActionBinding,
        definition::{UiDefinition, UiDefinitionId},
        display_list::{Color, DrawCommand, RenderOutput},
        focus::{ComponentInstanceId, FocusBehavior},
        geometry::{Point, Size},
        input::{ButtonState, KeyboardEvent, MouseButton, UiCommand, UiInput},
        instance::UiInstanceId,
        mailbox::OutputReceiver,
        presentation::PresentationSettings,
        registry::UiRegistry,
    },
};
use pixui_reflect::DynamicObject;

#[pixui_reflect::reflect]
mod model {
    pub struct Data {
        pub values: Vec<u64>,
        pub show: bool,
        pub eligible: bool,
    }
}
#[action]
fn noop() {}

struct Node;
impl Component for Node {
    type Props = ();
    type State = ();
}
struct NodePainter;
impl Painter<Node> for NodePainter {
    fn measure(&self, context: &MeasureContext<'_, Node>) -> PixuiResult<Size> {
        Ok(context.constrain(Size {
            width: 80.0,
            height: 30.0,
        }))
    }
    fn paint(&self, context: &mut PaintContext<'_, Node>) -> PixuiResult<()> {
        context.fill_rect(
            context.bounds(),
            Color(0, if context.focused { 255 } else { 0 }, 0),
        );
        Ok(())
    }
}
fn data<'a>(context: &ExpressionContext<'a>) -> PixuiResult<&'a model::Data> {
    let app = context.application()?;
    app.entity(app.slice_named("test")?.id(), "data")
}
fn values<'a>(context: &ExpressionContext<'a>) -> PixuiResult<DynamicObject<'a>> {
    Ok(DynamicObject::from_ref(&data(context)?.values))
}
fn show<'a>(context: &ExpressionContext<'a>) -> PixuiResult<DynamicObject<'a>> {
    Ok(DynamicObject::from_reflect(
        data(context)?.show && context.language() == Default::default(),
    ))
}
fn item_key(context: &ExpressionContext<'_>) -> PixuiResult<ItemKey> {
    Ok(ItemKey::Integer(
        *context.value()?.downcast_ref::<u64>().unwrap(),
    ))
}
fn eligibility(
    context: &ExpressionContext<'_>,
    settings: &PresentationSettings,
) -> PixuiResult<FocusBehavior> {
    Ok(
        if data(context)?.eligible && settings.locale != "unfocusable" {
            FocusBehavior::Sequential
        } else {
            FocusBehavior::None
        },
    )
}
fn props(_: &ExpressionContext<'_>, settings: &PresentationSettings) -> PixuiResult<()> {
    if settings.locale == "fail" {
        return Err(pixui_error!("deliberate preparation failure"));
    }
    Ok(())
}
fn activate(
    context: &ExpressionContext<'_>,
    _: &PresentationSettings,
) -> PixuiResult<ActionBinding> {
    let handle = context
        .application()?
        .slice_named("test")?
        .action_handle_named("noop")?;
    Ok(Box::new(move |_| handle.call(vec![])))
}
fn leaf(id: ComponentId<Node>, focus: FocusBehavior) -> LivePart {
    ComponentPart::typed(id, props).with_focus(focus).into()
}
fn branch(id: ComponentId<Node>) -> LivePart {
    LivePart::Match(
        MatchPart::new(
            Expression::computed(show),
            vec![MatchCandidate {
                pattern: MatchPattern::value(true),
                part: leaf(id, FocusBehavior::Sequential),
            }],
        )
        .unwrap(),
    )
}
struct Fixture {
    app: Application,
    uis: UiRegistry,
    node: ComponentId<Node>,
    data: ObjectRef<model::Data>,
}
impl Fixture {
    fn new() -> Self {
        let mut app = Application::default();
        let mut slice = ApplicationSlice::new("test");
        slice
            .bind(
                "data",
                model::Data {
                    values: vec![10, 20],
                    show: true,
                    eligible: true,
                },
            )
            .unwrap();
        let slice = app.add_slice(slice).unwrap();
        app.register_action(slice, noop_action::descriptor())
            .unwrap();
        let data = app.entity_ref(slice, "data").unwrap();
        let node = app.register_component::<Node>("node").unwrap();
        app.register_painter::<Node>(NodePainter).unwrap();
        Self {
            app,
            uis: UiRegistry::default(),
            node,
            data,
        }
    }
    fn register(&mut self, name: &str, parts: Vec<LivePart>) -> UiDefinitionId {
        self.uis
            .register(UiDefinition::new(
                name,
                LivePart::Composite(CompositePart { parts }),
            ))
            .unwrap()
    }
    fn create(
        &mut self,
        definition: UiDefinitionId,
        settings: PresentationSettings,
    ) -> (UiInstanceId, OutputReceiver) {
        let result = self.uis.create(definition, settings).unwrap();
        self.uis.render_dirty(&self.app);
        result
    }
    fn command(&mut self, command: UiCommand) -> PixuiResult<()> {
        if let Some(call) = self.uis.command(command, &self.app)? {
            self.app.dispatch(call)?;
            self.uis.invalidate_all();
        }
        self.uis.render_dirty(&self.app);
        Ok(())
    }
    fn input(&mut self, instance: UiInstanceId, input: UiInput) {
        let revision = self.uis.instance(instance).unwrap().revision();
        self.command(UiCommand::Input {
            instance,
            revision,
            input,
        })
        .unwrap();
    }
    fn focus(&mut self, instance: UiInstanceId, index: usize) -> ComponentInstanceId {
        let target = self
            .uis
            .instance(instance)
            .unwrap()
            .component_id(index)
            .unwrap();
        self.command(UiCommand::Focus {
            instance,
            target: Some(target.clone()),
        })
        .unwrap();
        target
    }
    fn state(&self, definition: UiDefinitionId) -> Option<ComponentInstanceId> {
        self.uis
            .definition(definition)
            .unwrap()
            .state()
            .focus
            .clone()
    }
    fn update(&mut self, change: impl FnOnce(&mut model::Data)) {
        change(self.app.resolve_mut(self.data).unwrap());
        self.uis.invalidate_all();
        self.uis.render_dirty(&self.app);
    }
}
fn green(output: &RenderOutput) -> usize {
    output
        .display_list
        .commands
        .iter()
        .filter(|command| {
            matches!(
                command,
                DrawCommand::FillRect {
                    color: Color(0, 255, 0),
                    ..
                }
            )
        })
        .count()
}
fn drain(receiver: &OutputReceiver) -> RenderOutput {
    receiver.try_recv().unwrap()
}

#[test]
fn keyed_rows_and_static_siblings_keep_focus_through_reordering_and_actions() {
    let mut f = Fixture::new();
    let rows = LivePart::ForLoop(
        ForLoopPart::new(
            Expression::computed(values),
            leaf(f.node, FocusBehavior::Sequential),
        )
        .with_key(item_key),
    );
    let definition = f.register(
        "rows",
        vec![
            rows,
            branch(f.node),
            ComponentPart::typed(f.node, props)
                .with_activation(activate)
                .into(),
        ],
    );
    let (instance, outputs) = f.create(definition, Default::default());
    drain(&outputs);
    let sibling = f.uis.instance(instance).unwrap().component_id(3).unwrap();
    let focused = f.focus(instance, 1);
    drain(&outputs);
    assert!(matches!(
        focused.path().segments(),
        [
            PathSegment::Child(0),
            PathSegment::LoopKey(ItemKey::Integer(20))
        ]
    ));
    f.update(|data| data.values = vec![20, 30, 10]);
    assert_eq!(f.state(definition), Some(focused.clone()));
    assert_eq!(
        f.uis.instance(instance).unwrap().component_id(0).unwrap(),
        focused
    );
    assert_eq!(
        f.uis.instance(instance).unwrap().component_id(4).unwrap(),
        sibling
    );
    assert_eq!(green(&drain(&outputs)), 1);
    f.focus(instance, 4);
    drain(&outputs);
    f.input(instance, UiInput::Keyboard(KeyboardEvent::named("Enter")));
    assert_eq!(f.state(definition), Some(sibling));
    drain(&outputs);
    f.focus(instance, 0);
    drain(&outputs);
    f.update(|data| data.values.retain(|value| *value != 20));
    assert_eq!(f.state(definition), None);
    assert_eq!(green(&drain(&outputs)), 0);
}

#[test]
fn match_removal_eligibility_and_failure_reconcile_only_successful_frames() {
    let mut f = Fixture::new();
    let definition = f.register(
        "conditional",
        vec![
            branch(f.node),
            ComponentPart::typed(f.node, props)
                .with_focus_resolver(eligibility)
                .into(),
        ],
    );
    let (instance, outputs) = f.create(definition, Default::default());
    drain(&outputs);
    let focus = f.focus(instance, 0);
    drain(&outputs);
    assert_eq!(
        focus.path().segments(),
        &[PathSegment::Child(0), PathSegment::MatchArm(0)]
    );
    f.command(UiCommand::Present {
        instance,
        settings: PresentationSettings {
            locale: "fail".into(),
            ..Default::default()
        },
    })
    .unwrap();
    assert_eq!(f.state(definition), Some(focus.clone()));
    assert!(f.uis.instance(instance).unwrap().last_error().is_some());
    assert!(outputs.try_recv().is_err());
    f.app.resolve_mut(f.data).unwrap().show = false;
    f.command(UiCommand::Present {
        instance,
        settings: Default::default(),
    })
    .unwrap();
    assert_eq!(f.state(definition), None);
    drain(&outputs);
    f.update(|data| data.show = true);
    assert_eq!(
        f.state(definition),
        None,
        "returning to an arm does not restore focus"
    );
    drain(&outputs);
    let target = f.focus(instance, 1);
    drain(&outputs);
    let old_revision = f.uis.instance(instance).unwrap().revision();
    f.update(|data| data.eligible = false);
    assert_eq!(f.state(definition), None);
    drain(&outputs);
    assert!(
        f.command(UiCommand::Focus {
            instance,
            target: Some(target)
        })
        .is_err()
    );
    assert!(
        f.command(UiCommand::Input {
            instance,
            revision: old_revision,
            input: UiInput::Keyboard(KeyboardEvent::named("Tab"))
        })
        .is_err()
    );
}

#[test]
fn focus_navigation_is_independent_of_activation_and_scoped_to_a_definition() {
    let mut f = Fixture::new();
    let definition = f.register(
        "navigation",
        vec![
            leaf(f.node, FocusBehavior::Sequential),
            leaf(f.node, FocusBehavior::Direct),
            ComponentPart::typed(f.node, props)
                .with_activation(activate)
                .with_focus(FocusBehavior::None)
                .into(),
            leaf(f.node, FocusBehavior::Sequential),
        ],
    );
    let other_definition = f.register("other", vec![leaf(f.node, FocusBehavior::Sequential)]);
    let (instance, outputs) = f.create(definition, Default::default());
    drain(&outputs);
    let (other, other_outputs) = f.create(other_definition, Default::default());
    drain(&other_outputs);
    let last = f.uis.instance(instance).unwrap().component_id(3).unwrap();
    let mut backwards = KeyboardEvent::named("Tab");
    backwards.modifiers.shift = true;
    f.input(instance, UiInput::Keyboard(backwards));
    assert_eq!(f.state(definition), Some(last));
    drain(&outputs);
    f.input(instance, UiInput::Keyboard(KeyboardEvent::named("Tab")));
    let first = f.uis.instance(instance).unwrap().component_id(0).unwrap();
    assert_eq!(f.state(definition), Some(first.clone()));
    drain(&outputs);
    f.input(instance, UiInput::Keyboard(KeyboardEvent::named("Space")));
    assert!(
        outputs.try_recv().is_err(),
        "nonactivating target ignores activation keys"
    );
    assert!(
        f.command(UiCommand::Focus {
            instance: other,
            target: Some(first)
        })
        .is_err()
    );
    let direct = f.focus(instance, 1);
    drain(&outputs);
    f.input(instance, UiInput::Focused(false));
    assert_eq!(f.state(definition), Some(direct));
    let rect = f.uis.instance(instance).unwrap().layout().component_bounds[0];
    f.input(
        instance,
        UiInput::MouseButton {
            button: MouseButton::Left,
            state: ButtonState::Pressed,
            position: Point {
                x: rect.x + 1.0,
                y: rect.y + 1.0,
            },
            modifiers: Default::default(),
        },
    );
    assert_eq!(
        f.state(definition),
        Some(f.uis.instance(instance).unwrap().component_id(0).unwrap())
    );
    drain(&outputs);
    f.input(
        instance,
        UiInput::MouseButton {
            button: MouseButton::Left,
            state: ButtonState::Pressed,
            position: Point { x: 1.0, y: 1.0 },
            modifiers: Default::default(),
        },
    );
    assert_eq!(f.state(definition), None);
    drain(&outputs);
    // An explicitly nonfocusable action still activates with the pointer.
    let rect = f.uis.instance(instance).unwrap().layout().component_bounds[2];
    f.input(
        instance,
        UiInput::MouseButton {
            button: MouseButton::Left,
            state: ButtonState::Released,
            position: Point {
                x: rect.x + 1.0,
                y: rect.y + 1.0,
            },
            modifiers: Default::default(),
        },
    );
    assert_eq!(f.state(definition), None);
    drain(&outputs);
    f.command(UiCommand::Focus {
        instance,
        target: None,
    })
    .unwrap();
}

#[test]
fn shared_focus_uses_the_latest_source_and_survives_hidden_or_failed_peers() {
    let mut f = Fixture::new();
    let definition = f.register("shared", vec![branch(f.node)]);
    let language = f.app.register_language("fr").unwrap();
    let (one, a) = f.create(definition, Default::default());
    drain(&a);
    let (two, b) = f.create(
        definition,
        PresentationSettings {
            language,
            ..Default::default()
        },
    );
    drain(&b); // This instance has no active match arm.
    let focused = f.focus(one, 0);
    assert_eq!(green(&drain(&a)), 1);
    assert_eq!(green(&drain(&b)), 0);
    f.uis.invalidate_all();
    f.uis.render_dirty(&f.app);
    assert_eq!(f.state(definition), Some(focused.clone()));
    drain(&a);
    drain(&b);
    f.input(two, UiInput::Keyboard(KeyboardEvent::named("Enter")));
    assert_eq!(
        f.state(definition),
        None,
        "keyboard input transfers source authority"
    );
    drain(&a);
    drain(&b);
    f.focus(one, 0);
    drain(&a);
    drain(&b);
    f.command(UiCommand::Visibility {
        instance: one,
        visible: false,
    })
    .unwrap();
    f.update(|data| data.show = false);
    assert_eq!(
        f.state(definition),
        Some(focused),
        "hidden source defers validation"
    );
    drain(&b);
    f.command(UiCommand::Visibility {
        instance: one,
        visible: true,
    })
    .unwrap();
    assert_eq!(f.state(definition), None);
    drain(&a);
    f.update(|data| data.show = true);
    drain(&a);
    drain(&b);
    f.focus(one, 0);
    drain(&a);
    drain(&b);
    f.command(UiCommand::Close { instance: one }).unwrap();
    assert_eq!(f.state(definition), None, "remaining source has no target");
    drain(&b);
    f.command(UiCommand::Close { instance: two }).unwrap();
    assert_eq!(f.state(definition), None);
}

#[test]
fn tab_scrolls_offscreen_targets_into_view_and_skips_permanently_clipped_nodes() {
    let mut f = Fixture::new();
    let clipped = ContainerPart::column()
        .with_layout(LayoutStyle::fixed(100.0, 1.0))
        .with_children(vec![
            leaf(f.node, FocusBehavior::Sequential),
            leaf(f.node, FocusBehavior::Sequential),
        ])
        .into();
    let definition = f.register(
        "scroll",
        vec![
            clipped,
            leaf(f.node, FocusBehavior::Sequential),
            leaf(f.node, FocusBehavior::Sequential),
            leaf(f.node, FocusBehavior::Sequential),
        ],
    );
    let (instance, outputs) = f.create(
        definition,
        PresentationSettings {
            viewport: Size {
                width: 200.0,
                height: 60.0,
            },
            ..Default::default()
        },
    );
    drain(&outputs);
    assert_eq!(
        f.uis
            .instance(instance)
            .unwrap()
            .layout()
            .focus_targets
            .len(),
        4,
        "partially clipped first child is reachable; fully clipped second is skipped"
    );
    for _ in 0..4 {
        f.input(instance, UiInput::Keyboard(KeyboardEvent::named("Tab")));
        drain(&outputs);
    }
    assert!(f.uis.definition(definition).unwrap().state().scroll > 0.0);
    let layout = f.uis.instance(instance).unwrap().layout();
    let target = layout.focus_targets.last().unwrap();
    assert!(target.bounds.y >= 0.0 && target.bounds.y + target.bounds.height <= 60.0);
}

#[derive(Default)]
struct Paths {
    entries: Vec<(ComponentPath, u64)>,
}
impl Visitor for Paths {
    fn visit(&mut self, entry: &mut WalkEntry) -> PixuiResult<()> {
        if let PartState::Component(state) = entry.state {
            let count = state.state.downcast_mut::<u64>().unwrap();
            *count += 1;
            self.entries.push((entry.path.clone(), *count));
        }
        Ok(())
    }
}
fn tracked() -> LivePart {
    ComponentPart::new(|value| {
        Ok(GenericComponentState::new(
            *value.downcast_ref::<u64>().unwrap_or(&0) * 10,
        ))
    })
    .into()
}
fn walk_values(tree: &mut LivePart, state: &mut PartState, values: Vec<u64>) -> PixuiResult<Paths> {
    let root = DynamicObject::from_reflect(values);
    let mut visitor = Paths::default();
    walk(
        tree,
        state,
        &ExpressionContext::from_value(&root),
        &mut visitor,
    )?;
    Ok(visitor)
}
fn current<'a>(context: &ExpressionContext<'a>) -> PixuiResult<DynamicObject<'a>> {
    Ok(DynamicObject::from_ref(
        context.value()?.downcast_ref::<Vec<u64>>().unwrap(),
    ))
}

#[test]
fn walker_paths_and_keyed_payloads_are_stable_but_unkeyed_items_are_positional() {
    let mut tree = LivePart::Composite(CompositePart {
        parts: vec![
            LivePart::ForLoop(
                ForLoopPart::new(Expression::computed(current), tracked()).with_key(item_key),
            ),
            tracked(),
        ],
    });
    let mut state = PartState::Unknown;
    let first = walk_values(&mut tree, &mut state, vec![1, 2]).unwrap();
    let second = walk_values(&mut tree, &mut state, vec![2, 3, 1]).unwrap();
    assert_eq!(first.entries[1].0, second.entries[0].0);
    assert_eq!(second.entries[0].1, 22);
    assert_eq!(second.entries[2].1, 12);
    assert_eq!(
        first.entries[2].0, second.entries[3].0,
        "static sibling survives changed loop length"
    );
    assert!(walk_values(&mut tree, &mut state, vec![2, 2]).is_err());
    let third = walk_values(&mut tree, &mut state, vec![2]).unwrap();
    assert_eq!(
        third.entries[0].1, 23,
        "duplicate failure does not move payloads"
    );
    walk_values(&mut tree, &mut state, vec![]).unwrap();
    let recreated = walk_values(&mut tree, &mut state, vec![2]).unwrap();
    assert_eq!(recreated.entries[0].1, 21, "removed keys drop payloads");

    let mut positional =
        LivePart::ForLoop(ForLoopPart::new(Expression::computed(current), tracked()));
    let mut state = PartState::Unknown;
    let first = walk_values(&mut positional, &mut state, vec![1, 2]).unwrap();
    let second = walk_values(&mut positional, &mut state, vec![2, 1]).unwrap();
    assert_eq!(first.entries[0].0, second.entries[0].0);
    assert_eq!(
        second.entries[0].1, 12,
        "unkeyed payload stays at its position"
    );
    assert_eq!(second.entries[0].0.segments(), &[PathSegment::LoopItem(0)]);
}

#[test]
fn nested_loop_keys_have_separate_namespaces_and_typed_paths() {
    fn groups<'a>(context: &ExpressionContext<'a>) -> PixuiResult<DynamicObject<'a>> {
        Ok(DynamicObject::from_ref(
            context.value()?.downcast_ref::<Vec<Vec<u64>>>().unwrap(),
        ))
    }
    fn group_key(context: &ExpressionContext<'_>) -> PixuiResult<ItemKey> {
        let group = context.value()?.downcast_ref::<Vec<u64>>().unwrap();
        Ok(ItemKey::Text(group[0].to_string()))
    }
    let inner = LivePart::ForLoop(
        ForLoopPart::new(Expression::computed(current), tracked()).with_key(item_key),
    );
    let mut tree = LivePart::ForLoop(
        ForLoopPart::new(Expression::computed(groups), inner).with_key(group_key),
    );
    let mut state = PartState::Unknown;
    let mut results = Vec::new();
    for groups in [
        vec![vec![1_u64, 9], vec![2, 9]],
        vec![vec![2, 9], vec![1, 9]],
    ] {
        let root = DynamicObject::from_reflect(groups);
        let mut visitor = Paths::default();
        walk(
            &mut tree,
            &mut state,
            &ExpressionContext::from_value(&root),
            &mut visitor,
        )
        .unwrap();
        results.push(visitor.entries);
    }
    assert_ne!(
        results[0][1].0, results[0][3].0,
        "equal inner keys belong to different outer items"
    );
    assert_eq!(results[0][1].0, results[1][3].0);
    assert_eq!(results[1][3].1, 92);
}

#[test]
fn collection_slot_reuse_does_not_restore_removed_entity_focus() {
    use pixui_engine::application::collection::Collection;
    let mut f = Fixture::new();
    let collection = f
        .app
        .register_collection(Collection::new_reflected::<u64>("numbers"))
        .unwrap();
    let old = f
        .app
        .resolve_collection_mut::<u64>(collection)
        .unwrap()
        .insert(1);
    let definition = f.register(
        "arena",
        vec![LivePart::ForLoop(ForLoopPart::new(
            Expression::collection(collection),
            leaf(f.node, FocusBehavior::Sequential),
        ))],
    );
    let (instance, outputs) = f.create(definition, Default::default());
    drain(&outputs);
    let target = f.focus(instance, 0);
    drain(&outputs);
    let arena = f.app.resolve_collection_mut::<u64>(collection).unwrap();
    arena.remove(old).unwrap();
    let new = arena.insert(2);
    assert_eq!(old.index(), new.index());
    assert_ne!(old.generation(), new.generation());
    f.uis.invalidate_all();
    f.uis.render_dirty(&f.app);
    drain(&outputs);
    assert_eq!(f.state(definition), None);
    assert_ne!(
        f.uis.instance(instance).unwrap().component_id(0).unwrap(),
        target
    );
    assert!(
        f.command(UiCommand::Focus {
            instance,
            target: Some(target)
        })
        .is_err()
    );
}

#[test]
fn closing_a_focus_source_preserves_a_surviving_target_and_requests_reject_dirty_geometry() {
    let mut f = Fixture::new();
    let definition = f.register("close", vec![leaf(f.node, FocusBehavior::Sequential)]);
    let (one, a) = f.create(definition, Default::default());
    drain(&a);
    let (two, b) = f.create(definition, Default::default());
    drain(&b);
    let focused = f.focus(one, 0);
    drain(&a);
    drain(&b);
    f.command(UiCommand::Close { instance: one }).unwrap();
    assert_eq!(f.state(definition), Some(focused.clone()));
    assert_eq!(green(&drain(&b)), 1);
    let revision = f.uis.instance(two).unwrap().revision();
    f.uis.invalidate_all();
    assert!(
        f.uis
            .command(
                UiCommand::Focus {
                    instance: two,
                    target: Some(focused)
                },
                &f.app
            )
            .is_err()
    );
    assert!(
        f.uis
            .command(
                UiCommand::Input {
                    instance: two,
                    revision,
                    input: UiInput::Keyboard(KeyboardEvent::named("Enter"))
                },
                &f.app
            )
            .is_err()
    );
    f.uis.render_dirty(&f.app);
    drain(&b);
    f.command(UiCommand::Focus {
        instance: two,
        target: None,
    })
    .unwrap();
    assert_eq!(f.state(definition), None);
    drain(&b);
}
