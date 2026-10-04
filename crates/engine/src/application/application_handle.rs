//! Cheaply clonable access to an application's internally owned worker thread.

use std::any::Any;

use crossbeam_channel::{TrySendError, bounded};
use pixui_base::erased_value::SendValues;
use pixui_base::{Key, PixuiResult, pixui_error};

use super::{
    action::ActionCall,
    action_handle::ActionHandle,
    app::Application,
    application_slice::{ApplicationSlice, SliceId},
    collection_key::CollectionKey,
    dispatch::{ApplicationCommand, ApplicationReply, CommandSender, PendingAction},
    object_ref::ObjectRef,
};

/// Nonblocking dispatch preserves the call on full or disconnected queues.
pub type TryDispatchResult = Result<PendingAction, TrySendError<ActionCall>>;

/// One cheaply clonable MPSC sender; application state stays on its owner thread.
///
/// Slice registration and inspection wait for a reply. Dispatch waits for queue
/// capacity, then returns a pending result. Drop every clone to close the queue;
/// accepted commands are drained before the worker exits. No join handle or
/// shared application state is retained here. Never call blocking handle methods
/// from this same application's worker, because it cannot process its own queue.
#[derive(Clone)]
pub struct ApplicationHandle {
    sender: CommandSender,
}

impl ApplicationHandle {
    pub(super) fn start(capacity: usize) -> Self {
        let (sender, receiver) = bounded(capacity);
        std::thread::Builder::new()
            .name("pixui-application".into())
            .spawn(move || super::dispatch::run(receiver))
            .expect("failed to start application worker");
        Self { sender }
    }

    /// Transfers a configured slice to the worker and waits for registration.
    /// The returned identity is stable and can be retained by every caller.
    pub fn add_slice(&self, slice: ApplicationSlice) -> PixuiResult<SliceId> {
        self.request(move |application| application.add_slice(slice))?
            .wait()
    }

    /// Resolves an action through one worker round trip. Cache the returned handle
    /// to construct later calls locally, including from other threads.
    pub fn action(&self, slice: SliceId, name: &str) -> PixuiResult<ActionHandle> {
        let name = name.to_owned();
        self.request(move |application| application.slice(slice)?.action_handle_named(&name))?
            .wait()
    }

    /// Resolves slice and collection names on the worker once. Cache the returned
    /// key for expression construction; removal of its slice invalidates the key.
    pub fn collection_key(&self, slice: &str, collection: &str) -> PixuiResult<CollectionKey> {
        let slice = slice.to_owned();
        let collection = collection.to_owned();
        self.request(move |application| application.collection_key(&slice, &collection))?
            .wait()
    }

    /// Resolves an action on the worker, then constructs its request locally.
    /// Reuse `action(...)?` and `ActionHandle::call` to avoid repeated lookups.
    pub fn action_call(
        &self,
        slice: SliceId,
        name: &str,
        fields: SendValues,
    ) -> PixuiResult<ActionCall> {
        self.action(slice, name)?.call(fields)
    }

    /// Validates an item address on the worker without retaining any borrow.
    pub fn object_ref<T: Any>(
        &self,
        slice: SliceId,
        collection: &str,
        key: Key<T>,
    ) -> PixuiResult<ObjectRef<T>> {
        let collection = collection.to_owned();
        self.request(move |application| application.object_ref(slice, &collection, key))?
            .wait()
    }

    /// Enqueues an action, blocking when the bounded queue is full.
    /// Failed sends drop the unsent call; `try_dispatch` preserves it for retry.
    pub fn dispatch(&self, call: ActionCall) -> PixuiResult<PendingAction> {
        let (reply, receiver) = bounded(1);
        self.sender
            .send(ApplicationCommand::Dispatch { call, reply })
            .map_err(|_| pixui_error!("application worker disconnected"))?;
        Ok(ApplicationReply::new(receiver))
    }

    /// Enqueues without waiting. Errors retain the call for retry via `into_inner`.
    pub fn try_dispatch(&self, call: ActionCall) -> TryDispatchResult {
        let (reply, receiver) = bounded(1);
        self.sender
            .try_send(ApplicationCommand::Dispatch { call, reply })
            .map_err(|error| match error {
                TrySendError::Full(ApplicationCommand::Dispatch { call, .. }) => {
                    TrySendError::Full(call)
                }
                TrySendError::Disconnected(ApplicationCommand::Dispatch { call, .. }) => {
                    TrySendError::Disconnected(call)
                }
                _ => unreachable!("only dispatch commands are sent here"),
            })?;
        Ok(ApplicationReply::new(receiver))
    }

    /// Inspects state on the worker and returns an owned, sendable snapshot.
    /// Borrowed application data cannot escape. Inspection shares the action queue
    /// and observes commands processed before it, rather than running concurrently.
    pub fn inspect<T: Send + 'static>(
        &self,
        read: impl FnOnce(&Application) -> PixuiResult<T> + Send + 'static,
    ) -> PixuiResult<T> {
        self.request(move |application| read(application))?.wait()
    }

    fn request<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut Application) -> PixuiResult<T> + Send + 'static,
    ) -> PixuiResult<ApplicationReply<T>> {
        let (reply, receiver) = bounded(1);
        let command = ApplicationCommand::Task(Box::new(move |application| {
            let _ = reply.send(operation(application));
        }));
        self.sender
            .send(command)
            .map_err(|_| pixui_error!("application worker disconnected"))?;
        Ok(ApplicationReply::new(receiver))
    }
}
