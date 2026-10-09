//! Structural occurrence identity, independent of flattened drawing order.

/// Identity within one loop occurrence. Keys must be immutable and unique among
/// its live items. Reusing a key transfers identity to the new item.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ItemKey {
    Integer(u64),
    Text(String),
    /// Complete generational arena key, including arena identity.
    Arena(u64),
}

/// A typed descent step prevents child, arm and item indices from colliding.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PathSegment {
    Child(usize),
    LoopItem(usize),
    LoopKey(ItemKey),
    MatchArm(usize),
}

/// A path in an immutable live definition. Unkeyed loop steps identify positions,
/// not entities. Definition replacement requires a new definition identity.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ComponentPath(Vec<PathSegment>);

impl ComponentPath {
    pub fn segments(&self) -> &[PathSegment] {
        &self.0
    }

    pub(crate) fn child(&self, segment: PathSegment) -> Self {
        let mut path = self.clone();
        path.0.push(segment);
        path
    }
}

/// Evaluates an immutable key in the current loop item's expression context.
pub type ItemKeyResolver = for<'a> fn(
    &crate::expression::context::ExpressionContext<'a>,
) -> pixui_base::PixuiResult<ItemKey>;
