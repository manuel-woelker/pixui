//! Output-local slot indices. Never persist these between render passes.
#[pixui_reflect::reflect(send)]
mod reflected {
    /// An index in one render's style table. Default is deliberately unbound.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct StyleIndex {
        pub index: usize,
    }
    impl Default for StyleIndex {
        fn default() -> Self {
            Self { index: usize::MAX }
        }
    }
}
pub use reflected::StyleIndex;
