//! Bounded dispatch from multiple callers to one application owner.

use crossbeam_channel::{Receiver, Sender, TrySendError, bounded, select_biased};
use pixui_base::{PixuiResult, pixui_error};

use super::{
    action::{ActionCall, ActionResult},
    app::Application,
};

type RequestSender = Sender<DispatchRequest>;
type RequestReceiver = Receiver<DispatchRequest>;
type ReplySender = Sender<ActionResult>;
type ReplyReceiver = Receiver<ActionResult>;
type OwnerLifetimeSender = Sender<()>;
type OwnerLifetimeReceiver = Receiver<()>;

/// A nonblocking enqueue outcome. On failure, recover the call with `into_inner`.
pub type TryDispatchResult = Result<PendingAction, TrySendError<ActionCall>>;

struct DispatchRequest {
    call: ActionCall,
    reply: ReplySender,
}

/// Clonable, thread-safe access to a bounded application action queue.
///
/// `dispatch` blocks until queue space is available, then returns a reply handle.
/// Calls execute sequentially on the thread running `DispatchLoop::run`.
/// Drop every dispatcher clone to close the queue; the loop drains accepted
/// calls and returns the application. Dropping the loop rejects pending and
/// future calls. There is no forced cancellation or implicit worker thread.
#[derive(Clone)]
pub struct Dispatch {
    sender: RequestSender,
    owner_closed: OwnerLifetimeReceiver,
}

impl Dispatch {
    /// Creates a bounded queue and its single-owner loop.
    /// A capacity of zero is a rendezvous: enqueue waits for the owner to receive.
    /// Panics if the channel capacity is too large to allocate.
    pub fn new(capacity: usize) -> (Self, DispatchLoop) {
        let (sender, receiver) = bounded(capacity);
        // No values are sent here. Disconnecting this channel notifies waiters
        // even if queued reply senders are retained by the request channel.
        let (owner_alive, owner_closed) = bounded(0);
        (
            Self {
                sender,
                owner_closed,
            },
            DispatchLoop {
                receiver,
                _owner_alive: owner_alive,
            },
        )
    }

    /// Enqueues a call, blocking when the queue is full.
    /// An error means the owner disconnected; the unsent call is dropped.
    /// Never enqueue synchronously from inside an action on this same loop:
    /// a full queue, or waiting for its reply, would deadlock the owner.
    pub fn dispatch(&self, call: ActionCall) -> PixuiResult<PendingAction> {
        let (reply, receiver) = bounded(1);
        self.sender
            .send(DispatchRequest { call, reply })
            .map_err(|_| pixui_error!("dispatch owner disconnected"))?;
        Ok(PendingAction {
            receiver,
            owner_closed: self.owner_closed.clone(),
        })
    }

    /// Attempts to enqueue without waiting for capacity. Full and disconnected
    /// errors preserve the call so the caller can retry or choose to discard it.
    pub fn try_dispatch(&self, call: ActionCall) -> TryDispatchResult {
        let (reply, receiver) = bounded(1);
        self.sender
            .try_send(DispatchRequest { call, reply })
            .map_err(|error| match error {
                TrySendError::Full(request) => TrySendError::Full(request.call),
                TrySendError::Disconnected(request) => TrySendError::Disconnected(request.call),
            })?;
        Ok(PendingAction {
            receiver,
            owner_closed: self.owner_closed.clone(),
        })
    }
}

/// The receiving side of a dispatch queue. Intentionally not clonable.
pub struct DispatchLoop {
    receiver: RequestReceiver,
    _owner_alive: OwnerLifetimeSender,
}

impl DispatchLoop {
    /// Owns and processes the application until every dispatcher is dropped.
    /// Accepted calls are drained before returning the final application.
    /// Handler errors are returned to callers and do not stop the loop.
    /// Handler panics propagate and disconnect the queue; they are not caught.
    pub fn run(self, mut application: Application) -> Application {
        for request in &self.receiver {
            let result = application.dispatch(request.call);
            // Each reply queue has room for its only result, even if no caller
            // is waiting yet. Abandoned replies must not stop the owner loop.
            let _ = request.reply.send(result);
        }
        application
    }
}

/// One action's eventual result. Waiting consumes the handle.
/// Dropping it discards the result, but does not cancel the queued action.
pub struct PendingAction {
    receiver: ReplyReceiver,
    owner_closed: OwnerLifetimeReceiver,
}

impl PendingAction {
    /// Blocks until dispatch completes or the owner disconnects.
    pub fn wait(self) -> ActionResult {
        select_biased! {
            recv(self.receiver) -> result => result.map_err(|_| pixui_error!("dispatch reply disconnected"))?,
            recv(self.owner_closed) -> _ => {
                // A completed reply wins over shutdown, including a reply that
                // became ready while select observed the owner's disconnection.
                self.receiver.try_recv().map_err(|_| pixui_error!("dispatch owner disconnected before replying"))?
            }
        }
    }
}
