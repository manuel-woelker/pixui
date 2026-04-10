use std::ops::Range;

/// Byte span within a source buffer.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Span(Range<usize>);

impl Span {
    /// Creates a new span from a start and end offset.
    pub const fn new(start: usize, end: usize) -> Self {
        Self(start..end)
    }

    /// Returns the start offset.
    pub const fn start(&self) -> usize {
        self.0.start
    }

    /// Returns the end offset.
    pub const fn end(&self) -> usize {
        self.0.end
    }

    /// Returns the underlying range.
    pub fn as_range(&self) -> Range<usize> {
        self.0.clone()
    }

    /// Returns the span length.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the span is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<Range<usize>> for Span {
    fn from(value: Range<usize>) -> Self {
        Self(value)
    }
}

impl From<Span> for Range<usize> {
    fn from(value: Span) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::Span;

    #[test]
    fn span_exposes_offsets_and_length() {
        let span = Span::new(2, 5);

        assert_eq!(span.start(), 2);
        assert_eq!(span.end(), 5);
        assert_eq!(span.len(), 3);
        assert!(!span.is_empty());
        assert_eq!(span.as_range(), 2..5);
    }

    #[test]
    fn empty_span_reports_empty() {
        let span = Span::new(3, 3);

        assert!(span.is_empty());
        assert_eq!(span.len(), 0);
    }
}
