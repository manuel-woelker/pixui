//! Internal bounded MPSC worker and typed action replies.

use crossbeam_channel::{Receiver, Sender};
use pixui_base::{PixuiResult, pixui_error};

use super::{
    action::{ActionCall, ActionOutput},
    app::Application,
};

pub(super) type CommandSender = Sender<ApplicationCommand>;
pub(super) type CommandReceiver = Receiver<ApplicationCommand>;
pub(super) type ReplySender<T> = Sender<PixuiResult<T>>;
type ReplyReceiver<T> = Receiver<PixuiResult<T>>;
type ApplicationTask = Box<dyn FnOnce(&mut Application) + Send>;

pub(super) enum ApplicationCommand {
    Dispatch {
        call: ActionCall,
        reply: ReplySender<ActionOutput>,
    },
    Task(ApplicationTask),
}

impl ApplicationCommand {
    fn run(self, application: &mut Application) {
        match self {
            Self::Dispatch { call, reply } => {
                let _ = reply.send(application.dispatch(call));
            }
            Self::Task(task) => task(application),
        }
    }
}

/// An action's eventual result. Dropping the reply does not cancel its action.
pub type PendingAction = ApplicationReply<ActionOutput>;

/// A single owned result from the application worker.
pub struct ApplicationReply<T> {
    receiver: ReplyReceiver<T>,
}

impl<T> ApplicationReply<T> {
    pub(super) fn new(receiver: ReplyReceiver<T>) -> Self {
        Self { receiver }
    }

    /// Waits for completion. Worker failure returns an error instead of hanging.
    pub fn wait(self) -> PixuiResult<T> {
        self.receiver
            .recv()
            .map_err(|_| pixui_error!("application worker stopped before replying"))?
    }
}

/// Executes commands on one owner thread until every sender is dropped.
/// A panicking command invalidates the state. The failed worker drains and drops
/// remaining commands, disconnecting their replies, until all handles are gone.
/// This prevents replies retained by the bounded queue from leaving callers stuck.
pub(super) fn run(receiver: CommandReceiver) {
    let mut application = Some(Application::default());
    for command in receiver {
        if let Some(state) = application.as_mut() {
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| command.run(state)));
            if result.is_err() {
                application = None;
            }
        }
        // In the failed state, dropping each command disconnects its reply.
    }
}
