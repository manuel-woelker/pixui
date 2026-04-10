use crate::file_path::FilePath;
use crate::shared_string::SharedString;

/// Source file contents together with their logical path.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SourceFile {
    pub path: FilePath,
    pub source: SharedString,
}

impl SourceFile {
    /// Creates a source file from its path and source contents.
    pub fn new(path: impl Into<FilePath>, source: impl Into<SharedString>) -> Self {
        Self {
            path: path.into(),
            source: source.into(),
        }
    }

    /// Returns the source contents as a string slice.
    pub fn source(&self) -> &str {
        self.source.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::SourceFile;

    #[test]
    fn source_file_stores_path_and_source() {
        let source_file = SourceFile::new("examples/hello.pixui", "println(\"hello\");");

        assert_eq!(source_file.path.as_str(), "examples/hello.pixui");
        assert_eq!(source_file.source(), "println(\"hello\");");
    }
}
