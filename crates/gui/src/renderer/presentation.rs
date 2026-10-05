//! Presentation state independent of a native window, allowing lifecycle tests.
use super::contract::{RenderOutcome, Renderer};
use pixui_base::PixuiResult;
use pixui_engine::ui::display_list::{DisplayList, RenderRevision};
use std::time::{Duration, Instant};

pub(crate) struct Presentation {
    pub renderer: Box<dyn Renderer>,
    pub active: bool,
    pub retry: Option<Instant>,
    pub revision: Option<RenderRevision>,
}
impl Presentation {
    pub fn new(renderer: Box<dyn Renderer>) -> Self {
        Self {
            renderer,
            active: true,
            retry: None,
            revision: None,
        }
    }
    pub fn draw(
        &mut self,
        display: &DisplayList,
        revision: RenderRevision,
        width: u32,
        height: u32,
        scale: f32,
    ) -> PixuiResult<()> {
        if !self.active || width == 0 || height == 0 {
            return Ok(());
        }
        self.renderer.resize(width, height, scale)?;
        if self.renderer.render(display)? == RenderOutcome::Presented {
            self.revision = Some(revision);
            self.retry = None;
        } else {
            self.retry = Some(Instant::now() + Duration::from_millis(33));
        }
        Ok(())
    }
    pub fn suspend(&mut self) {
        self.renderer.suspend();
        self.active = false;
        self.retry = None;
        self.revision = None;
    }
    pub fn resume(&mut self) -> PixuiResult<()> {
        if !self.active {
            self.renderer.resume()?;
            self.active = true;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, collections::VecDeque, rc::Rc};
    struct Fake {
        events: Rc<RefCell<Vec<String>>>,
        outcomes: VecDeque<PixuiResult<RenderOutcome>>,
    }
    impl Renderer for Fake {
        fn resize(&mut self, w: u32, h: u32, s: f32) -> PixuiResult<()> {
            self.events.borrow_mut().push(format!("resize {w} {h} {s}"));
            Ok(())
        }
        fn render(&mut self, _: &DisplayList) -> PixuiResult<RenderOutcome> {
            self.events.borrow_mut().push("render".into());
            self.outcomes.pop_front().unwrap()
        }
        fn suspend(&mut self) {
            self.events.borrow_mut().push("suspend".into());
        }
        fn resume(&mut self) -> PixuiResult<()> {
            self.events.borrow_mut().push("resume".into());
            Ok(())
        }
    }
    #[test]
    fn only_successful_presentation_advances_revision_and_lifecycle_is_independent() {
        let events = Rc::new(RefCell::new(vec![]));
        let fake = Fake {
            events: events.clone(),
            outcomes: VecDeque::from([
                Ok(RenderOutcome::Presented),
                Ok(RenderOutcome::Skipped),
                Err(pixui_base::pixui_error!("failed")),
                Ok(RenderOutcome::Presented),
            ]),
        };
        let mut presentation = Presentation::new(Box::new(fake));
        let display = DisplayList::default();
        presentation
            .draw(&display, RenderRevision(1), 100, 50, 1.5)
            .unwrap();
        assert_eq!(presentation.revision, Some(RenderRevision(1)));
        presentation
            .draw(&display, RenderRevision(2), 100, 50, 1.5)
            .unwrap();
        assert_eq!(presentation.revision, Some(RenderRevision(1)));
        assert!(presentation.retry.is_some());
        assert!(
            presentation
                .draw(&display, RenderRevision(3), 100, 50, 1.5)
                .is_err()
        );
        assert_eq!(presentation.revision, Some(RenderRevision(1)));
        let count = events.borrow().len();
        presentation
            .draw(&display, RenderRevision(4), 0, 50, 1.0)
            .unwrap();
        assert_eq!(events.borrow().len(), count);
        presentation.suspend();
        assert_eq!(presentation.revision, None);
        assert!(presentation.retry.is_none());
        presentation
            .draw(&display, RenderRevision(4), 100, 50, 1.0)
            .unwrap();
        presentation.resume().unwrap();
        presentation.resume().unwrap();
        presentation
            .draw(&display, RenderRevision(4), 100, 50, 2.0)
            .unwrap();
        assert_eq!(presentation.revision, Some(RenderRevision(4)));
        assert_eq!(&events.borrow()[..2], ["resize 100 50 1.5", "render"]);
        assert_eq!(
            events
                .borrow()
                .iter()
                .filter(|event| *event == "resume")
                .count(),
            1
        );
        let other = Presentation::new(Box::new(Fake {
            events: Rc::new(RefCell::new(vec![])),
            outcomes: VecDeque::new(),
        }));
        assert_eq!(other.revision, None);
    }
}
