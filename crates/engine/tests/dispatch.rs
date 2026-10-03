use std::cell::Cell;

use pixui_base::{Arena, PixuiResult, pixui_error};
use pixui_engine::application::{
    action::{ActionCall, ActionIndex, action},
    app::Application,
    application_slice::{ApplicationSlice, SliceId},
    collection::Collection,
    dispatch::Dispatch,
};

/// Demonstrates application state that is Send without being Sync.
#[action]
fn increment(counters: &mut Arena<Cell<i32>>, amount: i32) -> PixuiResult<i32> {
    if amount < 0 {
        return Err(pixui_error!("amount must be nonnegative"));
    }
    let (_, counter) = counters.iter_mut().next().expect("one counter");
    counter.set(counter.get() + amount);
    Ok(counter.get())
}

#[action]
fn fail() -> i32 {
    panic!("intentional dispatch test panic")
}

fn setup() -> (Application, SliceId, ActionIndex) {
    let mut slice = ApplicationSlice::new("counter");
    slice
        .add_collection(Collection::new::<Cell<i32>>("counters"))
        .unwrap();
    slice
        .collection_mut::<Cell<i32>>("counters")
        .unwrap()
        .insert(Cell::new(0));
    let index = slice
        .register_action(increment_action::descriptor())
        .unwrap();
    slice.register_action(fail_action::descriptor()).unwrap();
    let id = slice.id();
    let mut application = Application::new();
    application.slices.push(slice);
    (application, id, index)
}

fn call(slice: SliceId, action: ActionIndex, amount: i32) -> ActionCall {
    ActionCall {
        slice,
        action,
        request: Box::new(increment_action::request::Request { amount }),
    }
}

fn count(application: &Application, slice: SliceId) -> i32 {
    application
        .slice(slice)
        .unwrap()
        .collection("counters")
        .unwrap()
        .arena::<Cell<i32>>()
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .1
        .get()
}

#[test]
fn dispatches_from_multiple_threads_and_returns_results_to_each_caller() {
    let (application, slice, action) = setup();
    let (dispatch, owner) = Dispatch::new(2);
    let worker = std::thread::spawn(move || owner.run(application));
    let callers: Vec<_> = (0..4)
        .map(|_| {
            let dispatch = dispatch.clone();
            std::thread::spawn(move || {
                (0..25)
                    .map(|_| {
                        *dispatch
                            .dispatch(call(slice, action, 1))
                            .unwrap()
                            .wait()
                            .unwrap()
                            .downcast::<i32>()
                            .unwrap()
                    })
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let mut outputs: Vec<_> = callers
        .into_iter()
        .flat_map(|caller| caller.join().unwrap())
        .collect();
    outputs.sort();
    assert_eq!(outputs, (1..=100).collect::<Vec<_>>());
    drop(dispatch);
    let application = worker.join().unwrap();
    assert_eq!(count(&application, slice), 100);
}

#[test]
fn bounded_queue_reports_full_and_preserves_call_for_retry() {
    let (application, slice, action) = setup();
    let (dispatch, owner) = Dispatch::new(1);
    let first = dispatch.dispatch(call(slice, action, 1)).unwrap();
    let error = dispatch.try_dispatch(call(slice, action, 2)).err().unwrap();
    assert!(error.is_full());
    let retry = error.into_inner();
    let worker = std::thread::spawn(move || owner.run(application));
    assert_eq!(*first.wait().unwrap().downcast::<i32>().unwrap(), 1);
    let second = dispatch
        .try_dispatch(retry)
        .unwrap_or_else(|_| panic!("queue has been drained"));
    assert_eq!(*second.wait().unwrap().downcast::<i32>().unwrap(), 3);
    drop(dispatch);
    assert_eq!(count(&worker.join().unwrap(), slice), 3);
}

#[test]
fn dropping_producers_drains_accepted_calls_and_abandoned_replies_do_not_block() {
    let (application, slice, action) = setup();
    let (dispatch, owner) = Dispatch::new(2);
    drop(dispatch.dispatch(call(slice, action, 1)).unwrap());
    let pending = dispatch.dispatch(call(slice, action, 2)).unwrap();
    drop(dispatch);
    // No caller waits while the owner runs; the one-result reply queue has room.
    let application = owner.run(application);
    assert_eq!(count(&application, slice), 3);
    assert_eq!(*pending.wait().unwrap().downcast::<i32>().unwrap(), 3);
}

#[test]
fn disconnection_fails_pending_and_future_calls() {
    let (_, slice, action) = setup();
    let (dispatch, owner) = Dispatch::new(1);
    let pending = dispatch.dispatch(call(slice, action, 1)).unwrap();
    drop(owner);
    assert!(pending.wait().is_err());
    assert!(dispatch.dispatch(call(slice, action, 1)).is_err());
    let error = dispatch.try_dispatch(call(slice, action, 1)).err().unwrap();
    assert!(error.is_disconnected());
    assert_eq!(error.into_inner().slice, slice);
}

#[test]
fn handler_and_validation_errors_are_returned_without_stopping_owner() {
    let (application, slice, action) = setup();
    let (dispatch, owner) = Dispatch::new(1);
    let worker = std::thread::spawn(move || owner.run(application));
    let result = dispatch.dispatch(call(slice, action, -1)).unwrap().wait();
    assert!(result.err().unwrap().to_string().contains("nonnegative"));
    let wrong = ActionCall {
        slice,
        action,
        request: Box::new(String::new()),
    };
    assert!(dispatch.dispatch(wrong).unwrap().wait().is_err());
    assert_eq!(
        *dispatch
            .dispatch(call(slice, action, 1))
            .unwrap()
            .wait()
            .unwrap()
            .downcast::<i32>()
            .unwrap(),
        1
    );
    drop(dispatch);
    assert_eq!(count(&worker.join().unwrap(), slice), 1);
}

#[test]
fn zero_capacity_rendezvous_is_supported() {
    let (application, slice, action) = setup();
    let (dispatch, owner) = Dispatch::new(0);
    let worker = std::thread::spawn(move || owner.run(application));
    assert_eq!(
        *dispatch
            .dispatch(call(slice, action, 1))
            .unwrap()
            .wait()
            .unwrap()
            .downcast::<i32>()
            .unwrap(),
        1
    );
    drop(dispatch);
    assert_eq!(count(&worker.join().unwrap(), slice), 1);
}

#[test]
fn owner_panic_disconnects_queued_replies() {
    let (application, slice, action) = setup();
    let fail = application
        .slice(slice)
        .unwrap()
        .action_index("fail")
        .unwrap();
    let (dispatch, owner) = Dispatch::new(2);
    let panic_call = ActionCall {
        slice,
        action: fail,
        request: Box::new(fail_action::request::Request {}),
    };
    let failing = dispatch.dispatch(panic_call).unwrap();
    let queued = dispatch.dispatch(call(slice, action, 1)).unwrap();
    let worker = std::thread::spawn(move || owner.run(application));
    assert!(worker.join().is_err());
    assert!(failing.wait().is_err());
    assert!(queued.wait().is_err());
    assert!(dispatch.dispatch(call(slice, action, 1)).is_err());
}
