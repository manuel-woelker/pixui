use std::cell::Cell;

use crossbeam_channel::{Receiver, Sender, bounded};
use pixui_base::{Arena, PixuiResult, pixui_error};
use pixui_engine::application::{
    action::{ActionCall, ActionIndex, action},
    app::Application,
    application_handle::ApplicationHandle,
    application_slice::{ApplicationSlice, SliceId},
    collection::Collection,
};

/// Application state can be Send without being Sync.
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
fn hold(started: Sender<()>, release: Receiver<()>) {
    started.send(()).unwrap();
    release.recv().unwrap();
}

#[action]
fn observe(counters: &mut Arena<Cell<i32>>, observed: Sender<i32>) {
    observed
        .send(counters.iter().next().unwrap().1.get())
        .unwrap();
}

#[action]
fn fail() -> i32 {
    panic!("intentional dispatch test panic")
}

fn setup(capacity: usize) -> (ApplicationHandle, SliceId, ActionIndex) {
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
    slice.register_action(hold_action::descriptor()).unwrap();
    slice.register_action(observe_action::descriptor()).unwrap();
    slice.register_action(fail_action::descriptor()).unwrap();
    let application = Application::with_capacity(capacity);
    let id = application.add_slice(slice).unwrap();
    (application, id, index)
}

fn call(slice: SliceId, action: ActionIndex, amount: i32) -> ActionCall {
    ActionCall {
        slice,
        action,
        request: Box::new(increment_action::request::Request { amount }),
    }
}

fn count(application: &ApplicationHandle, slice: SliceId) -> i32 {
    application
        .inspect(move |state| {
            Ok(state
                .slice(slice)?
                .collection("counters")?
                .arena::<Cell<i32>>()
                .unwrap()
                .iter()
                .next()
                .unwrap()
                .1
                .get())
        })
        .unwrap()
}

fn pause(
    application: &ApplicationHandle,
    slice: SliceId,
) -> (
    Sender<()>,
    pixui_engine::application::dispatch::PendingAction,
) {
    let (started, ready) = bounded::<()>(1);
    let (release, gate) = bounded(1);
    let call = application
        .action_call(slice, "hold", vec![Box::new(started), Box::new(gate)])
        .unwrap();
    let pending = application.dispatch(call).unwrap();
    ready.recv().unwrap();
    (release, pending)
}

#[test]
fn construction_starts_worker_and_cloned_handles_register_slices() {
    let application = Application::new();
    let clone = application.clone();
    let id = std::thread::spawn(move || clone.add_slice(ApplicationSlice::new("new")))
        .join()
        .unwrap()
        .unwrap();
    assert_eq!(
        application
            .inspect(move |state| Ok(state.slice(id)?.name.to_string()))
            .unwrap(),
        "new"
    );
}

#[test]
fn last_handle_drop_stops_worker_and_drops_application_state() {
    struct DropSignal(Sender<()>);
    impl Drop for DropSignal {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }
    let (dropped, notification) = bounded::<()>(1);
    let application = Application::new();
    let clone = application.clone();
    let mut slice = ApplicationSlice::new("lifecycle");
    slice
        .add_collection(Collection::new::<DropSignal>("signals"))
        .unwrap();
    slice
        .collection_mut::<DropSignal>("signals")
        .unwrap()
        .insert(DropSignal(dropped));
    application.add_slice(slice).unwrap();
    drop(application);
    clone.inspect(|state| Ok(state.slices.len())).unwrap();
    assert!(notification.try_recv().is_err());
    drop(clone);
    notification
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
}

#[test]
fn dispatches_from_multiple_threads_and_returns_results_to_each_caller() {
    let (application, slice, action) = setup(2);
    let callers: Vec<_> = (0..4)
        .map(|_| {
            let application = application.clone();
            std::thread::spawn(move || {
                (0..25)
                    .map(|_| {
                        *application
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
    assert_eq!(count(&application, slice), 100);
}

#[test]
fn bounded_queue_reports_full_and_preserves_call_for_retry() {
    let (application, slice, action) = setup(1);
    let (release, hold) = pause(&application, slice);
    let first = application.dispatch(call(slice, action, 1)).unwrap();
    let error = application
        .try_dispatch(call(slice, action, 2))
        .err()
        .unwrap();
    assert!(error.is_full());
    let retry = error.into_inner();
    release.send(()).unwrap();
    hold.wait().unwrap();
    assert_eq!(*first.wait().unwrap().downcast::<i32>().unwrap(), 1);
    let second = application
        .try_dispatch(retry)
        .unwrap_or_else(|_| panic!("queue drained"));
    assert_eq!(*second.wait().unwrap().downcast::<i32>().unwrap(), 3);
    assert_eq!(count(&application, slice), 3);
}

#[test]
fn dropping_handles_drains_accepted_calls_and_abandoned_replies_do_not_block() {
    let (application, slice, action) = setup(2);
    let (observed, observation) = bounded::<i32>(1);
    // Prepare the observer before pausing the worker, since preparation waits.
    let observe = application
        .action_call(slice, "observe", vec![Box::new(observed)])
        .unwrap();
    let (release, hold) = pause(&application, slice);
    drop(application.dispatch(call(slice, action, 1)).unwrap());
    let pending = application.dispatch(observe).unwrap();
    drop(application);
    release.send(()).unwrap();
    hold.wait().unwrap();
    assert_eq!(observation.recv().unwrap(), 1);
    pending.wait().unwrap();
}

#[test]
fn handler_and_validation_errors_do_not_stop_worker() {
    let (application, slice, action) = setup(1);
    let result = application
        .dispatch(call(slice, action, -1))
        .unwrap()
        .wait();
    assert!(result.err().unwrap().to_string().contains("nonnegative"));
    let wrong = ActionCall {
        slice,
        action,
        request: Box::new(String::new()),
    };
    assert!(application.dispatch(wrong).unwrap().wait().is_err());
    assert_eq!(
        *application
            .dispatch(call(slice, action, 1))
            .unwrap()
            .wait()
            .unwrap()
            .downcast::<i32>()
            .unwrap(),
        1
    );
    assert_eq!(count(&application, slice), 1);
}

#[test]
fn zero_capacity_rendezvous_is_supported() {
    let (application, slice, action) = setup(0);
    assert_eq!(
        *application
            .dispatch(call(slice, action, 1))
            .unwrap()
            .wait()
            .unwrap()
            .downcast::<i32>()
            .unwrap(),
        1
    );
    assert_eq!(count(&application, slice), 1);
}

#[test]
fn panic_invalidates_state_and_rejects_queued_and_future_work_without_hanging() {
    let (application, slice, action) = setup(2);
    let failing = application.action_call(slice, "fail", vec![]).unwrap();
    let (release, hold) = pause(&application, slice);
    let failed = application.dispatch(failing).unwrap();
    let queued = application.dispatch(call(slice, action, 1)).unwrap();
    release.send(()).unwrap();
    hold.wait().unwrap();
    assert!(failed.wait().is_err());
    assert!(queued.wait().is_err());
    assert!(
        application
            .dispatch(call(slice, action, 1))
            .unwrap()
            .wait()
            .is_err()
    );
    assert!(
        application
            .add_slice(ApplicationSlice::new("late"))
            .is_err()
    );
    assert!(application.inspect(|_| Ok(())).is_err());
}

#[test]
fn handle_constructs_requests_and_resolves_refs_on_worker() {
    let (application, slice, action) = setup(1);
    let descriptor = application
        .inspect(move |state| state.slice(slice)?.action(action))
        .unwrap();
    assert_eq!(descriptor.name(), "increment");
    let key = application
        .inspect(move |state| {
            Ok(state
                .slice(slice)?
                .collection("counters")?
                .arena::<Cell<i32>>()
                .unwrap()
                .iter()
                .next()
                .unwrap()
                .0)
        })
        .unwrap();
    let reference = application.object_ref(slice, "counters", key).unwrap();
    let wrong = application.object_ref(slice, "missing", key);
    assert!(wrong.is_err());
    let call = application
        .action_call(slice, "increment", vec![Box::new(3i32)])
        .unwrap();
    application.dispatch(call).unwrap().wait().unwrap();
    assert_eq!(count(&application, slice), 3);
    // A reflected reference is still an owned, sendable request value.
    let _: pixui_base::erased_value::SendValue = Box::new(reference);
}
