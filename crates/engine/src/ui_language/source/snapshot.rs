//! Retained source text; diagnostics never consult a changed file on disk.
use super::span::Span;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
static NEXT_SOURCE: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SourceId(u64);
#[derive(Clone, Debug)]
pub struct Source {
    pub id: SourceId,
    pub filename: Arc<str>,
    pub text: Arc<str>,
    lines: Arc<[usize]>,
}
impl Source {
    pub fn new(filename: impl Into<Arc<str>>, text: impl Into<Arc<str>>) -> Self {
        let text = text.into();
        let mut lines = vec![0];
        lines.extend(text.match_indices('\n').map(|(index, _)| index + 1));
        Self {
            id: SourceId(
                NEXT_SOURCE
                    .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
                    .expect("source IDs exhausted"),
            ),
            filename: filename.into(),
            text,
            lines: lines.into(),
        }
    }
    /// Checked offsets, including zero-width EOF spans.
    pub fn span(&self, start: usize, end: usize) -> Option<Span> {
        (start <= end && self.text.is_char_boundary(start) && self.text.is_char_boundary(end))
            .then_some(Span {
                source: self.id,
                start,
                end,
            })
    }
    pub fn slice(&self, span: Span) -> Option<&str> {
        (span.source == self.id)
            .then(|| self.text.get(span.start..span.end))
            .flatten()
    }
    /// One-based line and Unicode-scalar column. Tabs count as one scalar.
    pub fn position(&self, offset: usize) -> Option<(usize, usize)> {
        if !self.text.is_char_boundary(offset) {
            return None;
        }
        let line = self.lines.partition_point(|start| *start <= offset) - 1;
        Some((
            line + 1,
            self.text[self.lines[line]..offset].chars().count() + 1,
        ))
    }
    pub fn line(&self, line: usize) -> Option<&str> {
        let start = *self.lines.get(line.checked_sub(1)?)?;
        let end = self.lines.get(line).copied().unwrap_or(self.text.len());
        Some(self.text[start..end].trim_end_matches(['\r', '\n']))
    }
}
