//! Named values reuse collection arenas, references, reflection and the worker.
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::{
        action::{ActionDescriptor, CollectionBinding, EntityBinding, slice_actions},
        app::Application,
        application_slice::ApplicationSlice,
        collection::Collection,
        entity_mut::EntityMut,
    },
    expression::{context::ExpressionContext, evaluator::evaluate, expression::Expression},
};

#[slice_actions(slice = "flags", facade = FlagActions)]
mod actions {
    use super::*;
    #[action]
    pub fn toggle(mut hide_done: EntityMut<bool>) {
        *hide_done = !*hide_done;
    }
    #[action]
    pub fn rename(
        mut title: pixui_engine::application::entity_mut::EntityMut<String>,
        value: String,
    ) {
        *title = value;
    }
    #[action]
    pub fn fail(mut hide_done: EntityMut<bool>) -> PixuiResult<()> {
        *hide_done = true;
        Err(pixui_error!("deliberate failure"))
    }
}

#[test]
fn staged_values_use_one_unnamed_collection_per_type_and_survive_slice_removal() {
    let mut app = Application::default();
    let mut slice = ApplicationSlice::new("flags");
    slice.bind("hide_done", false).unwrap();
    slice.bind("show_details", true).unwrap();
    slice.bind("title", String::from("first")).unwrap();
    assert!(slice.entity("hide_done").is_err());
    assert!(app.collections().is_empty());
    let slice = app.add_slice(slice).unwrap();
    let hide = app.entity_ref::<bool>(slice, "hide_done").unwrap();
    let details = app.entity_ref::<bool>(slice, "show_details").unwrap();
    assert_eq!(hide.collection_index(), details.collection_index());
    assert_ne!(hide.key(), details.key());
    assert_eq!(app.collections().len(), 2);
    assert!(
        app.collections()
            .iter()
            .all(|collection| collection.name().is_empty())
    );
    let explicit = app
        .add_collection(slice, Collection::new::<bool>("explicit"))
        .unwrap();
    assert_ne!(explicit, hide.collection_index());
    app.bind(slice, "another", false).unwrap();
    assert_eq!(app.collections().len(), 3);
    let second = app.add_slice(ApplicationSlice::new("second")).unwrap();
    app.bind_entity(second, "shared", hide).unwrap();
    *app.entity_mut::<bool>(second, "shared").unwrap() = true;
    assert!(*app.entity::<bool>(slice, "hide_done").unwrap());
    assert!(app.entity_ref::<String>(slice, "hide_done").is_err());
    app.remove_slice(slice).unwrap();
    assert!(*app.resolve(hide).unwrap());
    assert!(*app.entity::<bool>(second, "shared").unwrap());
}

#[test]
fn rejected_names_and_foreign_bindings_do_not_insert_pending_or_runtime_values() {
    let mut app = Application::default();
    let mut slice = ApplicationSlice::new("flags");
    slice.bind("value", false).unwrap();
    assert!(slice.bind("value", true).is_err());
    assert!(slice.bind("", String::from("orphan")).is_err());
    let slice = app.add_slice(slice).unwrap();
    let index = app
        .entity_ref::<bool>(slice, "value")
        .unwrap()
        .collection_index();
    assert!(app.bind(slice, "value", String::from("orphan")).is_err());
    assert!(app.bind(slice, "", 42_i32).is_err());
    assert_eq!(app.collections().len(), 1);
    assert_eq!(
        app.resolve_collection(index)
            .unwrap()
            .arena::<bool>()
            .unwrap()
            .len(),
        1
    );
    let foreign = Application::default().create_entity(false).unwrap();
    assert!(app.bind_entity(slice, "foreign", foreign).is_err());
    let mut invalid = ApplicationSlice::new("invalid");
    invalid.bind("pending", 1_i32).unwrap();
    invalid.bind_entity("foreign", foreign).unwrap();
    assert!(app.add_slice(invalid).is_err());
    assert_eq!(app.collections().len(), 1);
    assert_eq!(app.slices().len(), 1);
    let mut duplicate = ApplicationSlice::new("flags");
    duplicate.bind("pending", 1_i32).unwrap();
    assert!(app.add_slice(duplicate).is_err());
    assert_eq!(app.collections().len(), 1);
    let shared = app.entity_ref::<bool>(slice, "value").unwrap();
    let mut pending = ApplicationSlice::new("pending");
    pending.bind("flag", true).unwrap();
    assert!(pending.bind_entity("flag", shared).is_err());
    let mut resolved = ApplicationSlice::new("resolved");
    resolved.bind_entity("flag", shared).unwrap();
    assert!(resolved.bind("flag", true).is_err());
}

#[test]
fn entity_expressions_borrow_live_values_and_reject_stale_or_foreign_refs() {
    let mut app = Application::default();
    let reference = app.create_entity(false).unwrap();
    let expression = Expression::entity(reference);
    let value = evaluate(&ExpressionContext::new(&app), &expression).unwrap();
    assert!(!*value.downcast_ref::<bool>().unwrap());
    assert!(!value.is_mutable());
    assert!(std::ptr::eq(
        value.downcast_ref::<bool>().unwrap(),
        app.resolve(reference).unwrap()
    ));
    drop(value);
    *app.resolve_mut(reference).unwrap() = true;
    assert!(
        *evaluate(&ExpressionContext::new(&app), &expression)
            .unwrap()
            .downcast_ref::<bool>()
            .unwrap()
    );
    assert!(
        evaluate(
            &ExpressionContext::new(&Application::default()),
            &expression
        )
        .is_err()
    );
    app.resolve_collection_mut::<bool>(reference.collection_index())
        .unwrap()
        .remove(reference.key());
    let replacement = app.create_entity(false).unwrap();
    assert_eq!(replacement.key().index(), reference.key().index());
    assert!(evaluate(&ExpressionContext::new(&app), &expression).is_err());
    assert!(app.resolve(reference).is_err());
    let mut slice = ApplicationSlice::new("stale");
    slice.bind("pending", String::new()).unwrap();
    slice.bind_entity("old", reference).unwrap();
    assert!(app.add_slice(slice).is_err());
    assert_eq!(app.collections().len(), 1);
}

#[test]
fn worker_facades_inject_named_entities_and_keep_them_out_of_requests() {
    let app = Application::new();
    let mut slice = ApplicationSlice::new("flags");
    slice.bind("hide_done", false).unwrap();
    slice.bind("title", String::from("old")).unwrap();
    let slice = app.add_slice(slice).unwrap();
    actions::FlagActions::register(&app, slice).unwrap();
    let facade = actions::FlagActions::bind(&app).unwrap();
    let descriptor = actions::toggle_action::descriptor();
    assert!(descriptor.arguments().fields().is_empty());
    assert!(descriptor.collections().is_empty());
    assert_eq!(descriptor.entities()[0].name, "hide_done");
    facade.toggle().unwrap();
    facade.rename("new").unwrap();
    assert!(facade.fail().is_err());
    app.inspect(move |app| {
        assert!(*app.entity::<bool>(slice, "hide_done")?);
        assert_eq!(app.entity::<String>(slice, "title")?, "new");
        Ok(())
    })
    .unwrap();
    let reference = app.entity_ref::<bool>(slice, "hide_done").unwrap();
    let peer = app.add_slice(ApplicationSlice::new("peer")).unwrap();
    app.bind_entity(peer, "same", reference).unwrap();
    app.bind(peer, "extra", true).unwrap();
    app.inspect(move |app| {
        assert!(*app.entity::<bool>(peer, "same")?);
        assert_eq!(
            app.entity_ref::<bool>(peer, "extra")?.collection_index(),
            reference.collection_index()
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn registration_checks_type_liveness_and_conflicting_mutable_aliases_atomically() {
    let mut app = Application::default();
    let id = app.add_slice(ApplicationSlice::new("flags")).unwrap();
    assert!(
        app.register_action(id, actions::toggle_action::descriptor())
            .is_err()
    );
    app.bind(id, "hide_done", String::new()).unwrap();
    assert!(
        app.register_action(id, actions::toggle_action::descriptor())
            .is_err()
    );
    assert!(app.slice(id).unwrap().actions().is_empty());
    let reference = app.create_entity(false).unwrap();
    let id = app.add_slice(ApplicationSlice::new("valid")).unwrap();
    app.bind_entity(id, "hide_done", reference).unwrap();
    app.bind_entity(id, "alias", reference).unwrap();
    let aliases = Box::leak(Box::new(
        ActionDescriptor::new::<actions::toggle_action::request::Request>(
            "aliases",
            "",
            vec![],
            |_, _, _| Ok(Box::new(())),
        )
        .with_entities(vec![
            EntityBinding::new::<bool>("hide_done"),
            EntityBinding::new::<bool>("alias"),
        ]),
    ));
    assert!(
        app.register_actions(id, &[actions::toggle_action::descriptor(), aliases])
            .is_err()
    );
    assert!(app.slice(id).unwrap().actions().is_empty());
    app.bind_collection(id, "flags", reference.collection_index())
        .unwrap();
    let overlap = Box::leak(Box::new(
        ActionDescriptor::new::<actions::toggle_action::request::Request>(
            "overlap",
            "",
            vec![CollectionBinding::new::<bool>("flags")],
            |_, _, _| Ok(Box::new(())),
        )
        .with_entities(vec![EntityBinding::new::<bool>("hide_done")]),
    ));
    assert!(app.register_action(id, overlap).is_err());
    app.register_action(id, actions::toggle_action::descriptor())
        .unwrap();
    let call = app.action_call(id, "toggle", vec![]).unwrap();
    app.resolve_collection_mut::<bool>(reference.collection_index())
        .unwrap()
        .remove(reference.key());
    assert!(app.dispatch(call).is_err());
    assert!(app.entity_ref::<bool>(id, "hide_done").is_err());
}

#[test]
fn staged_values_are_dropped_on_failed_attachment_and_non_sync_values_are_supported() {
    use pixui_reflect::{Reflect, TypeDescriptor};
    use std::{
        cell::Cell,
        sync::{
            Arc, OnceLock,
            atomic::{AtomicUsize, Ordering},
        },
    };
    struct Value {
        count: Cell<i32>,
        drops: Arc<AtomicUsize>,
    }
    impl Drop for Value {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }
    impl Reflect for Value {
        fn type_descriptor() -> &'static TypeDescriptor {
            static TYPE: OnceLock<TypeDescriptor> = OnceLock::new();
            TYPE.get_or_init(|| TypeDescriptor::new::<Self>(vec![], vec![]).unwrap())
        }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let app = Application::new();
    let mut invalid = ApplicationSlice::new("");
    invalid
        .bind(
            "value",
            Value {
                count: Cell::new(3),
                drops: drops.clone(),
            },
        )
        .unwrap();
    assert!(app.add_slice(invalid).is_err());
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    let mut valid = ApplicationSlice::new("valid");
    valid
        .bind(
            "value",
            Value {
                count: Cell::new(7),
                drops: drops.clone(),
            },
        )
        .unwrap();
    let id = app.add_slice(valid).unwrap();
    app.inspect(move |app| {
        assert_eq!(app.entity::<Value>(id, "value")?.count.get(), 7);
        Ok(())
    })
    .unwrap();
}
