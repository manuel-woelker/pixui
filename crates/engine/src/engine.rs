/// Core engine entry point for the pixui project.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Engine;

impl Engine {
    /// Creates a new engine instance.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::Engine;

    #[test]
    fn new_returns_default_engine() {
        assert_eq!(Engine::new(), Engine);
    }
}
