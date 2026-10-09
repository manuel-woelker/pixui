//! Ordered, bounded clipboard work off both the application and native event threads.
//! The native backend retains clipboard ownership (needed by X11) for its lifetime.
use crossbeam_channel::{Receiver, Sender, bounded};
use pixui_engine::ui::{
    instance::UiInstanceId,
    text_input::{
        editing::MAX_CONTENT_BYTES,
        protocol::{ClipboardReply, HostEffect},
    },
};

/// Plain UTF-8 clipboard access. Tests and alternate hosts can supply a backend.
pub trait ClipboardBackend: Send + 'static {
    fn read(&mut self) -> Result<Option<String>, String>;
    fn write(&mut self, text: &str) -> Result<(), String>;
}

#[derive(Default)]
pub struct NativeClipboard {
    clipboard: Option<arboard::Clipboard>,
}
impl NativeClipboard {
    fn clipboard(&mut self) -> Result<&mut arboard::Clipboard, String> {
        if self.clipboard.is_none() {
            self.clipboard = Some(arboard::Clipboard::new().map_err(|error| error.to_string())?);
        }
        Ok(self.clipboard.as_mut().expect("initialized clipboard"))
    }
}
impl ClipboardBackend for NativeClipboard {
    fn read(&mut self) -> Result<Option<String>, String> {
        match self.clipboard()?.get_text() {
            Ok(text) => Ok(Some(text)),
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }
    fn write(&mut self, text: &str) -> Result<(), String> {
        self.clipboard()?
            .set_text(text)
            .map_err(|error| error.to_string())
    }
}

/// One executor per host. Queue saturation returns an error reply, never blocks.
/// Dropping it closes the queue; an in-flight OS operation finishes on its thread.
pub struct ClipboardExecutor {
    requests: Sender<(UiInstanceId, HostEffect)>,
    replies: Receiver<ClipboardReply>,
}
impl ClipboardExecutor {
    pub fn new(mut backend: impl ClipboardBackend, wake: impl Fn() + Send + 'static) -> Self {
        let (requests, incoming) = bounded::<(UiInstanceId, HostEffect)>(16);
        let (outgoing, replies) = bounded(16);
        std::thread::spawn(move || {
            while let Ok((instance, effect)) = incoming.recv() {
                let (request, result) = match effect {
                    HostEffect::ReadClipboard { request } => (
                        request,
                        backend.read().and_then(|text| {
                            if text
                                .as_ref()
                                .is_some_and(|text| text.len() > MAX_CONTENT_BYTES)
                            {
                                Err("clipboard text exceeds 1 MiB".into())
                            } else {
                                Ok(text)
                            }
                        }),
                    ),
                    HostEffect::WriteClipboard { request, text } => (
                        request,
                        if text.len() > MAX_CONTENT_BYTES {
                            Err("clipboard text exceeds 1 MiB".into())
                        } else {
                            backend.write(&text).map(|()| None)
                        },
                    ),
                };
                if outgoing
                    .send(ClipboardReply {
                        instance,
                        request,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
                wake();
            }
        });
        Self { requests, replies }
    }
    pub fn submit(&self, instance: UiInstanceId, effect: HostEffect) -> Result<(), ClipboardReply> {
        self.requests.try_send((instance, effect)).map_err(|error| {
            let (instance, effect) = error.into_inner();
            let request = match effect {
                HostEffect::ReadClipboard { request }
                | HostEffect::WriteClipboard { request, .. } => request,
            };
            ClipboardReply {
                instance,
                request,
                result: Err("clipboard executor unavailable or queue full".into()),
            }
        })
    }
    pub fn try_recv(&self) -> Option<ClipboardReply> {
        self.replies.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pixui_engine::{
        application::app::Application,
        live_model::part::{CompositePart, LivePart},
        ui::{definition::UiDefinition, presentation::PresentationSettings},
    };
    struct Memory {
        text: Option<String>,
    }
    impl ClipboardBackend for Memory {
        fn read(&mut self) -> Result<Option<String>, String> {
            Ok(self.text.clone())
        }
        fn write(&mut self, text: &str) -> Result<(), String> {
            if text == "fail" {
                return Err("write failed".into());
            }
            self.text = Some(text.into());
            Ok(())
        }
    }
    #[test]
    fn asynchronous_backend_preserves_order_and_reports_failures() {
        let app = Application::new();
        let definition = app
            .register_ui(UiDefinition::new(
                "empty",
                LivePart::Composite(CompositePart { parts: vec![] }),
            ))
            .unwrap();
        let (instance, _) = app
            .create_ui(definition, PresentationSettings::default())
            .unwrap();
        let (wake, notifications) = bounded(16);
        let clipboard = ClipboardExecutor::new(Memory { text: None }, move || {
            let _ = wake.send(());
        });
        for (request, text) in [(1, "value"), (2, "fail")] {
            clipboard
                .submit(
                    instance,
                    HostEffect::WriteClipboard {
                        request,
                        text: text.into(),
                    },
                )
                .unwrap();
        }
        clipboard
            .submit(instance, HostEffect::ReadClipboard { request: 3 })
            .unwrap();
        let mut replies = Vec::new();
        for _ in 0..3 {
            notifications
                .recv_timeout(std::time::Duration::from_secs(3))
                .unwrap();
            replies.push(clipboard.try_recv().unwrap());
        }
        assert_eq!(
            replies
                .iter()
                .map(|reply| reply.request)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(replies[0].result, Ok(None));
        assert_eq!(replies[1].result, Err("write failed".into()));
        assert_eq!(replies[2].result, Ok(Some("value".into())));
    }
}
