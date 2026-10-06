use crossbeam_channel::{Receiver, Sender, bounded};
use pixui_base::{Arena, Key};
use pixui_engine::application::{
    action::action,
    app::Application,
    application_handle::ApplicationHandle,
    application_slice::{ApplicationSlice, SliceId},
    collection::Collection,
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

fn slice(application: &ApplicationHandle) -> SliceId {
    let id = application
        .add_slice(ApplicationSlice::new("numbers"))
        .unwrap();
    application
        .add_collection(id, Collection::new::<i32>("numbers"))
        .unwrap();
    application
        .register_action(id, add_action::descriptor())
        .unwrap();
    id
}

#[test]
fn handles_survive_action_append_and_validate_target_at_dispatch() {
    let application = Application::new();
    let id = slice(&application);
    let action = application.action(id, "add").unwrap();
    let index = action.index();
    let copy = action;
    assert_eq!(action.slice_id(), id);
    assert!(std::ptr::eq(action.descriptor(), add_action::descriptor()));
    application
        .register_action(id, hold_action::descriptor())
        .unwrap();
    assert_eq!(application.action(id, "add").unwrap().index(), index);
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
                    .collection(id, "numbers")?
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
    let id = slice(&application);
    application
        .register_action(id, hold_action::descriptor())
        .unwrap();
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
    let id = slice(&application);
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
