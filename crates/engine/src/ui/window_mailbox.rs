//! Bounded-by-property mailbox: each persistent property has one pending slot.
use super::window_properties::WindowCommand;
use crossbeam_channel::TryRecvError;
use std::sync::{Arc, Mutex};

type Waker = Arc<dyn Fn() + Send + Sync>;
#[derive(Default)]
struct Pending {
    text_input: Option<WindowCommand>,
    title: Option<WindowCommand>,
    icon: Option<WindowCommand>,
    closed: bool,
    waker: Option<Waker>,
}
pub(crate) struct WindowCommandSender(Arc<Mutex<Pending>>);
/// Single nonblocking consumer attached to its UI's output subscription.
/// Commands have no cross-property ordering guarantee. Dropping the instance
/// closes delivery; pending commands can still be drained before disconnection.
pub struct WindowCommandReceiver(Arc<Mutex<Pending>>);

pub(crate) fn mailbox() -> (WindowCommandSender, WindowCommandReceiver) {
    let pending = Arc::new(Mutex::new(Pending::default()));
    (
        WindowCommandSender(pending.clone()),
        WindowCommandReceiver(pending),
    )
}
impl WindowCommandSender {
    pub fn publish(&self, command: WindowCommand) {
        let waker = {
            let mut pending = self.0.lock().expect("window command mailbox lock");
            match command {
                WindowCommand::SetTextInput(_) => pending.text_input = Some(command),
                WindowCommand::SetTitle(_) => pending.title = Some(command),
                WindowCommand::SetIcon(_) => pending.icon = Some(command),
            }
            pending.waker.clone()
        };
        if let Some(waker) = waker {
            waker();
        }
    }
}
impl Drop for WindowCommandSender {
    fn drop(&mut self) {
        let waker = {
            let mut pending = self.0.lock().expect("window command mailbox lock");
            pending.closed = true;
            pending.waker.clone()
        };
        if let Some(waker) = waker {
            waker();
        }
    }
}
impl WindowCommandReceiver {
    /// Runs after publication/disconnection, outside the lock. Installing wakes
    /// once to cover earlier publication. Keep callbacks nonblocking and brief.
    pub fn set_waker(&self, callback: impl Fn() + Send + Sync + 'static) {
        let callback: Waker = Arc::new(callback);
        self.0.lock().expect("window command mailbox lock").waker = Some(callback.clone());
        callback();
    }
    pub fn try_recv(&self) -> Result<WindowCommand, TryRecvError> {
        let mut pending = self.0.lock().expect("window command mailbox lock");
        if let Some(command) = pending
            .title
            .take()
            .or_else(|| pending.icon.take())
            .or_else(|| pending.text_input.take())
        {
            Ok(command)
        } else if pending.closed {
            Err(TryRecvError::Disconnected)
        } else {
            Err(TryRecvError::Empty)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{display_list::Color, image::Image};
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[test]
    fn coalesces_per_property_and_releases_replaced_icons() {
        let (sender, receiver) = mailbox();
        let image = Image::new(1, 1, vec![Color(1, 2, 3)], None).unwrap();
        let weak = image.downgrade();
        sender.publish(WindowCommand::SetTitle("old".into()));
        sender.publish(WindowCommand::SetIcon(Some(image)));
        sender.publish(WindowCommand::SetTitle("latest".into()));
        assert_eq!(
            receiver.try_recv().unwrap(),
            WindowCommand::SetTitle("latest".into())
        );
        assert!(matches!(
            receiver.try_recv(),
            Ok(WindowCommand::SetIcon(Some(_)))
        ));
        assert!(weak.upgrade().is_none());
        let image = Image::new(1, 1, vec![Color(1, 2, 3)], None).unwrap();
        let weak = image.downgrade();
        sender.publish(WindowCommand::SetIcon(Some(image)));
        sender.publish(WindowCommand::SetIcon(None));
        assert!(weak.upgrade().is_none());
        drop(sender);
        assert_eq!(receiver.try_recv().unwrap(), WindowCommand::SetIcon(None));
        assert_eq!(receiver.try_recv(), Err(TryRecvError::Disconnected));
    }
    #[test]
    fn wake_attachment_is_reentrant_and_disconnect_is_observable() {
        let (sender, receiver) = mailbox();
        let receiver = Arc::new(receiver);
        sender.publish(WindowCommand::SetTitle("early".into()));
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let weak = Arc::downgrade(&receiver);
        receiver.set_waker(move || {
            count.fetch_add(1, Ordering::SeqCst);
            if let Some(receiver) = weak.upgrade() {
                let _ = receiver.try_recv();
            }
        });
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        sender.publish(WindowCommand::SetIcon(None));
        drop(sender);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }
}
