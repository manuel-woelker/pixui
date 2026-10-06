use pixui_base::{Arena, Key, PixuiResult, pixui_error};
use pixui_engine::application::{
    action::{ActionDescriptor, slice_actions},
    app::Application,
    application_handle::ApplicationHandle,
    application_slice::{ApplicationSlice, SliceId},
    collection::Collection,
};

#[slice_actions(slice = "notes", facade = NoteActions)]
mod actions {
    use super::*;

    /// Stores a nonempty note in the injected collection.
    #[action]
    pub fn add(notes: &mut Arena<String>, title: String) -> PixuiResult<Key<String>> {
        if title.is_empty() {
            return Err(pixui_error!("empty title"));
        }
        Ok(notes.insert(title))
    }

    #[action]
    pub fn rename(note: &mut String, title: std::string::String) {
        *note = title;
    }

    #[action]
    pub fn sum(application: i32, slice: i32, request: i32) -> i32 {
        application + slice + request
    }

    #[action]
    pub fn answer() -> i32 {
        42
    }

    #[cfg(any())]
    #[action]
    pub fn unavailable(missing: &mut Arena<bool>) {}

    #[cfg_attr(all(), cfg(any()))]
    #[action]
    pub fn also_unavailable() {}
}

use actions::NoteActions;

fn slice(application: &ApplicationHandle, name: &str) -> SliceId {
    let id = application
        .add_slice(ApplicationSlice::new(name.to_owned()))
        .unwrap();
    application
        .add_collection(id, Collection::new::<String>("notes"))
        .unwrap();
    NoteActions::register(application, id).unwrap();
    id
}

#[test]
fn typed_facade_converts_strings_resolves_references_and_propagates_errors() {
    struct Title;
    impl From<Title> for String {
        fn from(_: Title) -> Self {
            "custom title".into()
        }
    }
    let application = Application::new();
    let id = slice(&application, "notes");
    let actions = NoteActions::bind(&application).unwrap();
    assert_eq!(actions.slice_id(), id);
    let first: Key<String> = actions.add("borrowed string").unwrap();
    let second = actions.add(String::from("owned string")).unwrap();
    actions.add(Title).unwrap();
    assert!(actions.add("").is_err());
    let reference = application.object_ref(id, "notes", first).unwrap();
    let result: () = actions.rename(reference, "renamed").unwrap();
    assert_eq!(result, ());
    assert_eq!(actions.sum(1, 2, 3).unwrap(), 6);
    assert_eq!(actions.answer().unwrap(), 42);
    application
        .inspect(move |state| {
            let notes = state.collection(id, "notes")?.arena::<String>().unwrap();
            assert_eq!(notes.len(), 3);
            assert_eq!(notes.get(first).unwrap(), "renamed");
            assert_eq!(notes.get(second).unwrap(), "owned string");
            Ok(())
        })
        .unwrap();
}

#[test]
fn cloned_facades_dispatch_from_multiple_threads() {
    let application = Application::new();
    slice(&application, "notes");
    let actions = NoteActions::bind(&application).unwrap();
    let callers: Vec<_> = (0..8)
        .map(|index| {
            let actions = actions.clone();
            std::thread::spawn(move || actions.add(format!("note {index}")).unwrap())
        })
        .collect();
    let keys: Vec<_> = callers
        .into_iter()
        .map(|caller| caller.join().unwrap())
        .collect();
    let id = actions.slice_id();
    application
        .inspect(move |state| {
            let notes = state.collection(id, "notes")?.arena::<String>().unwrap();
            assert_eq!(notes.len(), keys.len());
            assert!(keys.iter().all(|key| notes.contains(*key)));
            Ok(())
        })
        .unwrap();
}

#[test]
fn binding_validates_missing_slices_actions_and_exact_descriptors() {
    let application = Application::new();
    assert!(NoteActions::bind(&application).is_err());
    let missing = application
        .add_slice(ApplicationSlice::new("notes"))
        .unwrap();
    application
        .add_collection(missing, Collection::new::<String>("notes"))
        .unwrap();
    assert!(NoteActions::bind(&application).is_err());

    let application = Application::new();
    let original = actions::add_action::descriptor();
    static IMPOSTOR: std::sync::OnceLock<ActionDescriptor> = std::sync::OnceLock::new();
    let impostor = IMPOSTOR.get_or_init(|| {
        ActionDescriptor::new::<actions::add_action::request::Request>(
            "add",
            "",
            vec![],
            |_, _, _| Ok(Box::new(())),
        )
    });
    let wrong = application
        .add_slice(ApplicationSlice::new("notes"))
        .unwrap();
    application.register_action(wrong, impostor).unwrap();
    assert!(!std::ptr::eq(original, impostor));
    assert!(NoteActions::bind(&application).is_err());
}

#[test]
fn explicit_binding_supports_other_names_and_rejects_foreign_identities() {
    let application = Application::new();
    let id = slice(&application, "archive");
    assert!(NoteActions::bind(&application).is_err());
    let actions = NoteActions::bind_to(&application, id).unwrap();
    actions.add("archived").unwrap();
    assert_eq!(actions.slice_id(), id);
    let foreign = ApplicationSlice::new("foreign").id();
    assert!(NoteActions::bind_to(&application, foreign).is_err());
}

#[test]
fn registration_and_slice_name_validation_are_atomic() {
    let application = Application::new();
    let missing = application
        .add_slice(ApplicationSlice::new("missing"))
        .unwrap();
    assert!(
        application
            .register_actions(
                missing,
                &[
                    actions::answer_action::descriptor(),
                    actions::add_action::descriptor(),
                ]
            )
            .is_err()
    );
    assert!(
        application
            .register_actions(
                missing,
                &[
                    actions::answer_action::descriptor(),
                    actions::answer_action::descriptor(),
                ]
            )
            .is_err()
    );
    application
        .inspect(move |state| {
            assert!(state.slice(missing)?.actions().is_empty());
            Ok(())
        })
        .unwrap();
    let id = slice(&application, "notes");
    assert!(
        application
            .add_slice(ApplicationSlice::new("notes"))
            .is_err()
    );
    assert!(application.add_slice(ApplicationSlice::new("")).is_err());
    assert!(NoteActions::register(&application, id).is_err());
    application
        .inspect(move |state| {
            assert_eq!(state.slices().len(), 2);
            assert_eq!(state.slice_named("notes")?.name(), "notes");
            assert_eq!(state.slice(id)?.actions().len(), 4);
            Ok(())
        })
        .unwrap();
}

#[test]
fn registration_rejects_mutable_aliases_and_checks_foreign_and_wrong_types_atomically() {
    use pixui_engine::application::action::CollectionBinding;
    let mut app = Application::default();
    let id = app.add_slice(ApplicationSlice::new("aliases")).unwrap();
    let index = app
        .add_collection(id, Collection::new::<String>("notes"))
        .unwrap();
    app.bind_collection(id, "alias", index).unwrap();
    let action = Box::leak(Box::new(ActionDescriptor::new::<
        actions::answer_action::request::Request,
    >(
        "aliases",
        "",
        vec![
            CollectionBinding::new::<String>("notes"),
            CollectionBinding::new::<String>("alias"),
        ],
        |_, _, _| Ok(Box::new(())),
    )));
    assert!(
        app.register_actions(id, &[actions::answer_action::descriptor(), action])
            .is_err()
    );
    assert!(app.slice(id).unwrap().actions().is_empty());
    let other = app.add_slice(ApplicationSlice::new("wrong_type")).unwrap();
    app.add_collection(other, Collection::new::<i32>("notes"))
        .unwrap();
    assert!(actions::NoteActions::register_in(&mut app, other).is_err());
    assert!(app.slice(other).unwrap().actions().is_empty());
    let second = app
        .add_collection(id, Collection::new::<String>("different"))
        .unwrap();
    assert_ne!(index, second);
    let action = Box::leak(Box::new(ActionDescriptor::new::<
        actions::answer_action::request::Request,
    >(
        "distinct",
        "",
        vec![
            CollectionBinding::new::<String>("notes"),
            CollectionBinding::new::<String>("different"),
        ],
        |_, _, _| Ok(Box::new(())),
    )));
    app.register_action(id, action).unwrap();
}

#[test]
fn action_facades_in_different_slices_mutate_the_same_application_collection() {
    let app = Application::new();
    let shared = app
        .register_collection(Collection::new::<String>("shared_notes"))
        .unwrap();
    let mut first = ApplicationSlice::new("notes");
    first.bind_collection("notes", shared).unwrap();
    let first = app.add_slice(first).unwrap();
    let mut second = ApplicationSlice::new("archive");
    second.bind_collection("notes", shared).unwrap();
    let second = app.add_slice(second).unwrap();
    NoteActions::register(&app, first).unwrap();
    NoteActions::register(&app, second).unwrap();
    let first_actions = NoteActions::bind_to(&app, first).unwrap();
    let second_actions = NoteActions::bind_to(&app, second).unwrap();
    let key = first_actions.add("shared item").unwrap();
    let reference = app.object_ref_at(shared, key).unwrap();
    second_actions
        .rename(reference, "updated through archive")
        .unwrap();
    app.inspect(move |state| {
        assert_eq!(
            state
                .collection(first, "notes")?
                .arena::<String>()
                .unwrap()
                .get(key)
                .unwrap(),
            "updated through archive"
        );
        assert!(std::ptr::eq(
            state.collection(first, "notes")?,
            state.collection(second, "notes")?
        ));
        Ok(())
    })
    .unwrap();
}
