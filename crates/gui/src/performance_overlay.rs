//! Per-window diagnostics, appended after application commands with no hit regions.
//! The font is prepared once per DPI scale; diagnostic refreshes do not count as
//! application frames. Timings are the latest completed frames, not averages.
use crate::renderer::contract::RendererTimings;
use pixui_base::PixuiResult;
use pixui_engine::ui::{
    display_list::{Color, DisplayList, DrawCommand, RenderRevision},
    geometry::{Point, Rect, Size},
    performance::{FrameMemory, WorkerTimings},
    text::{
        font::{FontConfig, FontFace},
        resource::{Font, FontIndex},
        service::TextService,
    },
};
use std::{
    collections::{BTreeSet, VecDeque},
    time::{Duration, Instant},
};

#[derive(Default)]
pub(crate) struct PerformanceOverlay {
    pub visible: bool,
    pub refresh: Option<Instant>,
    frames: VecDeque<Instant>,
    revision: Option<RenderRevision>,
    font: Option<(f32, Font)>,
    timings: Option<RendererTimings>,
}
impl PerformanceOverlay {
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        self.refresh = None;
    }
    pub fn presented(
        &mut self,
        revision: RenderRevision,
        timings: Option<RendererTimings>,
        now: Instant,
    ) {
        if self.revision != Some(revision) {
            self.frames.push_back(now);
            self.revision = Some(revision);
            self.timings = timings;
        }
        self.prune(now);
        self.refresh = self.visible.then_some(now + Duration::from_millis(250));
    }
    fn prune(&mut self, now: Instant) {
        while self
            .frames
            .front()
            .is_some_and(|time| now.duration_since(*time) >= Duration::from_secs(1))
        {
            self.frames.pop_front();
        }
    }
    pub fn append(
        &mut self,
        source: &DisplayList,
        worker: WorkerTimings,
        scale: f32,
        viewport: Size,
        now: Instant,
    ) -> PixuiResult<DisplayList> {
        self.prune(now);
        if self
            .font
            .as_ref()
            .is_none_or(|(previous, _)| *previous != scale)
        {
            let config = FontConfig::new(FontFace::geist_mono()?, 14.0, scale)?;
            let characters: BTreeSet<_> = (' '..='~').collect();
            let font = TextService::default().prepare(&config, &characters)?;
            self.font = Some((scale, font));
        }
        let memory = FrameMemory::measure(source);
        let mut text = format!(
            "Performance (F11)\n{:<20} {:>9} fps\n",
            "FPS (worker outputs):",
            self.frames.len()
        );
        for (label, duration) in [
            ("Prepare:", worker.preparation),
            ("Paint:", worker.painting),
            ("Text / finalize:", worker.text),
        ] {
            text.push_str(&format!(
                "{label:<20} {:>9.3} ms\n",
                duration.as_secs_f64() * 1000.0
            ));
        }
        if let Some(gui) = self.timings {
            for (label, duration) in [
                ("Acquire / validate:", gui.acquisition),
                ("Resources / upload:", gui.resources),
                ("Draw / rasterize:", gui.drawing),
                ("Submit / present:", gui.submission),
            ] {
                text.push_str(&format!(
                    "{label:<20} {:>9.3} ms\n",
                    duration.as_secs_f64() * 1000.0
                ));
            }
        } else {
            text.push_str("Renderer timings: unavailable\n");
        }
        for (label, bytes) in [
            ("Display list:", memory.display_list),
            ("Images (RGB):", memory.images),
            ("Fonts / atlases:", memory.fonts),
        ] {
            text.push_str(&format!("{label:<20} {:>9.1} KiB\n", bytes as f64 / 1024.0));
        }
        text.push_str("CPU times; frame memory estimates");
        let font = &self.font.as_ref().unwrap().1;
        let height = text.lines().count() as f32 * font.metrics().line_height + 20.0;
        let mut display = source.clone();
        let mut fonts: Vec<_> = display.fonts.iter().cloned().collect();
        let index = FontIndex::from_raw(fonts.len());
        fonts.push(font.clone());
        display.fonts = fonts.into();
        let width = 320.0_f32.min(viewport.width);
        let height = height.min(viewport.height);
        let rect = Rect {
            x: (viewport.width - width - 8.0).max(0.0),
            y: (viewport.height - height - 8.0).max(0.0),
            width,
            height,
        };
        display.commands.extend([
            DrawCommand::FillRect {
                rect,
                color: Color(20, 24, 30),
            },
            DrawCommand::StrokeRect {
                rect,
                color: Color(130, 150, 170),
                width: 1.0,
            },
            DrawCommand::PushClip { rect },
            DrawCommand::DrawText {
                origin: Point {
                    x: rect.x + 10.0,
                    y: rect.y + 10.0 + font.metrics().ascent,
                },
                text,
                font: index,
                color: Color(240, 245, 250),
            },
            DrawCommand::PopClip,
        ]);
        display.validate()?;
        Ok(display)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostic_redraws_do_not_inflate_fps_and_idle_frames_expire() {
        let mut overlay = PerformanceOverlay::default();
        let now = Instant::now();
        overlay.toggle();
        overlay.presented(RenderRevision(1), None, now);
        overlay.presented(RenderRevision(1), None, now);
        assert_eq!(overlay.frames.len(), 1);
        overlay.presented(RenderRevision(2), None, now);
        assert_eq!(overlay.frames.len(), 2);
        overlay.prune(now + Duration::from_secs(1));
        assert!(overlay.frames.is_empty());
        overlay.toggle();
        assert!(!overlay.visible);
        assert!(overlay.refresh.is_none());
    }
    #[test]
    fn overlay_is_valid_and_does_not_modify_application_list() {
        let source = DisplayList::default();
        let mut overlay = PerformanceOverlay::default();
        let display = overlay
            .append(
                &source,
                WorkerTimings::default(),
                1.0,
                Size {
                    width: 640.0,
                    height: 480.0,
                },
                Instant::now(),
            )
            .unwrap();
        assert!(source.commands.is_empty());
        assert_eq!(display.commands.len(), 5);
        display.validate().unwrap();
        let first = display.fonts[0].identity();
        assert!(
            overlay
                .append(
                    &source,
                    WorkerTimings::default(),
                    1.0,
                    Size {
                        width: 640.0,
                        height: 480.0
                    },
                    Instant::now()
                )
                .unwrap()
                .fonts[0]
                .identity()
                == first
        );
        assert!(
            overlay
                .append(
                    &source,
                    WorkerTimings::default(),
                    2.0,
                    Size {
                        width: 640.0,
                        height: 480.0
                    },
                    Instant::now()
                )
                .unwrap()
                .fonts[0]
                .identity()
                != first
        );
    }
}
