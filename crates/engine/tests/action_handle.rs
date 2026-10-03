use crossbeam_channel::{Receiver, Sender, bounded};
use pixui_base::{Arena, Key};
use pixui_engine::application::{
    action::action, app::Application, application_slice::ApplicationSlice, collection::Collection,
};

#[action]
fn add(numbers: &mut Arena<i32>, value: i32) -> Key<i32> {
    numbers.insert(value)
}

#[action]
fn hold(started: Sender<()>, release: Receiver<()>) {
    started.send(()).unwrap();
    release.recv().unwrap();
}

fn slice() -> ApplicationSlice {
    let mut slice = ApplicationSlice::new("numbers");
    slice
        .add_collection(Collection::new::<i32>("numbers"))
        .unwrap();
    slice.register_action(add_action::descriptor()).unwrap();
    slice
}

#[test]
fn handles_created_before_add_slice_survive_append_and_validate_target_at_dispatch() {
    let application = Application::new();
    let mut slice = slice();
    let index = slice.action_index("add").unwrap();
    let action = slice.action_handle(index).unwrap();
    let copy = action;
    assert_eq!(action.slice_id(), slice.id());
    assert_eq!(action.index(), index);
    assert!(std::ptr::eq(action.descriptor(), add_action::descriptor()));
    let call = action.call(vec![Box::new(7i32)]).unwrap();
    assert!(application.dispatch(call).unwrap().wait().is_err());
    slice.register_action(hold_action::descriptor()).unwrap();
    let id = application.add_slice(slice).unwrap();
    let call = copy.call(vec![Box::new(7i32)]).unwrap();
    let key = *application
        .dispatch(call)
        .unwrap()
        .wait()
        .unwrap()
        .downcast::<Key<i32>>()
        .unwrap();
    assert_eq!(
        application
            .inspect(move |state| {
                Ok(*state
                    .slice(id)?
                    .collection("numbers")?
                    .arena::<i32>()
                    .unwrap()
                    .get(key)
                    .unwrap())
            })
            .unwrap(),
        7
    );
}

#[test]
fn requests_are_constructed_locally_from_another_thread_while_worker_is_busy() {
    let application = Application::with_capacity(1);
    let mut slice = slice();
    slice.register_action(hold_action::descriptor()).unwrap();
    let id = application.add_slice(slice).unwrap();
    let add = application.action(id, "add").unwrap();
    let hold = application.action(id, "hold").unwrap();
    let (started, ready) = bounded::<()>(1);
    let (release, gate) = bounded::<()>(1);
    let pending_hold = application
        .dispatch(hold.call(vec![Box::new(started), Box::new(gate)]).unwrap())
        .unwrap();
    ready.recv().unwrap();
    assert!(add.call(vec![]).is_err());
    assert!(add.call(vec![Box::new(String::new())]).is_err());
    let call = std::thread::spawn(move || add.call(vec![Box::new(9i32)]).unwrap())
        .join()
        .unwrap();
    let pending_add = application.dispatch(call).unwrap();
    release.send(()).unwrap();
    pending_hold.wait().unwrap();
    assert!(pending_add.wait().unwrap().is::<Key<i32>>());
}

#[test]
fn lookup_reports_missing_targets_and_cached_metadata_outlives_application() {
    let application = Application::new();
    let slice = slice();
    assert!(slice.action_handle_named("missing").is_err());
    let id = application.add_slice(slice).unwrap();
    assert!(application.action(id, "missing").is_err());
    assert!(
        application
            .action(ApplicationSlice::new("absent").id(), "add")
            .is_err()
    );
    let action = application.action(id, "add").unwrap();
    drop(application);
    let call = action.call(vec![Box::new(1i32)]).unwrap();
    assert_eq!(call.slice, id);
    assert_eq!(call.action, action.index());
    assert_eq!(
        call.request
            .downcast::<add_action::request::Request>()
            .unwrap()
            .value,
        1
    );
}
