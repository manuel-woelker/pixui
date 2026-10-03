use pixui_base::{Arena, Key, PixuiResult, pixui_error};
use pixui_engine::application::{
    action::{ActionDescriptor, slice_actions},
    app::Application,
    application_slice::ApplicationSlice,
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

fn slice(name: &str) -> ApplicationSlice {
    let mut slice = ApplicationSlice::new(name.to_owned());
    slice
        .add_collection(Collection::new::<String>("notes"))
        .unwrap();
    NoteActions::register(&mut slice).unwrap();
    slice
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
    let id = application.add_slice(slice("notes")).unwrap();
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
            let notes = state
                .slice(id)?
                .collection("notes")?
                .arena::<String>()
                .unwrap();
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
    application.add_slice(slice("notes")).unwrap();
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
            let notes = state
                .slice(id)?
                .collection("notes")?
                .arena::<String>()
                .unwrap();
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
    let mut missing = ApplicationSlice::new("notes");
    missing
        .add_collection(Collection::new::<String>("notes"))
        .unwrap();
    application.add_slice(missing).unwrap();
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
    let mut wrong = ApplicationSlice::new("notes");
    wrong.register_action(impostor).unwrap();
    application.add_slice(wrong).unwrap();
    assert!(!std::ptr::eq(original, impostor));
    assert!(NoteActions::bind(&application).is_err());
}

#[test]
fn explicit_binding_supports_other_names_and_rejects_foreign_identities() {
    let application = Application::new();
    let id = application.add_slice(slice("archive")).unwrap();
    assert!(NoteActions::bind(&application).is_err());
    let actions = NoteActions::bind_to(&application, id).unwrap();
    actions.add("archived").unwrap();
    assert_eq!(actions.slice_id(), id);
    let foreign = ApplicationSlice::new("foreign").id();
    assert!(NoteActions::bind_to(&application, foreign).is_err());
}

#[test]
fn registration_and_slice_name_validation_are_atomic() {
    let mut missing = ApplicationSlice::new("notes");
    // Put a valid action before one whose collection binding is missing.
    assert!(
        missing
            .register_actions(&[
                actions::answer_action::descriptor(),
                actions::add_action::descriptor(),
            ])
            .is_err()
    );
    assert!(missing.actions().is_empty());
    assert!(
        missing
            .register_actions(&[
                actions::answer_action::descriptor(),
                actions::answer_action::descriptor(),
            ])
            .is_err()
    );
    assert!(missing.actions().is_empty());

    let application = Application::new();
    application.add_slice(slice("notes")).unwrap();
    assert!(application.add_slice(slice("notes")).is_err());
    assert!(application.add_slice(slice("")).is_err());
    application
        .inspect(|state| {
            assert_eq!(state.slices().len(), 1);
            assert_eq!(state.slice_named("notes")?.name(), "notes");
            Ok(())
        })
        .unwrap();
    let mut configured = slice("another");
    let count = configured.actions().len();
    assert!(NoteActions::register(&mut configured).is_err());
    assert_eq!(configured.actions().len(), count);
}
