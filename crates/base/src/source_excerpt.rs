use crate::file_path::FilePath;
use crate::shared_string::SharedString;
use crate::source_annotation::SourceAnnotation;

/// One rendered source line plus inline annotations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceExcerpt {
    /// Logical path for the excerpted source line.
    pub file_path: FilePath,
    /// One-based line number in the source file.
    pub line_number: usize,
    /// Full source line text.
    pub source_line: SharedString,
    /// Span annotations attached to this line.
    pub annotations: Vec<SourceAnnotation>,
}

impl SourceExcerpt {
    /// Creates a source excerpt from its file path, line number, and source text.
    pub fn new(
        file_path: impl Into<FilePath>,
        line_number: usize,
        source_line: impl Into<SharedString>,
    ) -> Self {
        Self {
            file_path: file_path.into(),
            line_number,
            source_line: source_line.into(),
            annotations: Vec::new(),
        }
    }

    /// Returns a copy of this excerpt with one appended annotation.
    pub fn with_annotation(mut self, annotation: SourceAnnotation) -> Self {
        self.annotations.push(annotation);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::SourceExcerpt;
    use crate::source_annotation::SourceAnnotation;
    use crate::span::Span;

    #[test]
    fn source_excerpt_tracks_annotations() {
        let excerpt = SourceExcerpt::new("examples/test.pixui", 3, "println(value);")
            .with_annotation(SourceAnnotation::new(Span::new(8, 13), "unknown name"));

        assert_eq!(excerpt.file_path.as_str(), "examples/test.pixui");
        assert_eq!(excerpt.line_number, 3);
        assert_eq!(excerpt.source_line, "println(value);");
        assert_eq!(excerpt.annotations.len(), 1);
        assert_eq!(excerpt.annotations[0].span, Span::new(8, 13));
        assert_eq!(excerpt.annotations[0].message, "unknown name");
    }
}
