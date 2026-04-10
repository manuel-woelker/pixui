/// Compilation stage that can report expected source-level failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompilationStage {
    Lexer,
    Parser,
    Resolver,
}

impl std::fmt::Display for CompilationStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lexer => f.write_str("lexer"),
            Self::Parser => f.write_str("parser"),
            Self::Resolver => f.write_str("resolver"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CompilationStage;

    #[test]
    fn compilation_stage_formats_as_lowercase_name() {
        assert_eq!(CompilationStage::Lexer.to_string(), "lexer");
        assert_eq!(CompilationStage::Parser.to_string(), "parser");
        assert_eq!(CompilationStage::Resolver.to_string(), "resolver");
    }
}
