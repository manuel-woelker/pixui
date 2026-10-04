use crate::application::application_slice::SliceId;

/// An expression evaluated against application state.
pub struct Expression {
    kind: ExpressionKind,
}

impl Expression {
    pub fn new(kind: ExpressionKind) -> Self {
        Self { kind }
    }

    /// References a collection by its slice identity and registration index.
    pub fn collection(slice_id: SliceId, collection_index: usize) -> Self {
        Self::new(ExpressionKind::Collection(CollectionExpression::new(
            slice_id,
            collection_index,
        )))
    }

    pub fn kind(&self) -> &ExpressionKind {
        &self.kind
    }
}

/// Operations supported by the expression evaluator.
pub enum ExpressionKind {
    Collection(CollectionExpression),
}

/// A collection address within an application.
///
/// Collection indices follow insertion order and remain stable because collections
/// cannot be removed or reordered. Slice identities survive slice reordering.
/// These addresses are process-local; invalid addresses fail during evaluation.
pub struct CollectionExpression {
    slice_id: SliceId,
    collection_index: usize,
}

impl CollectionExpression {
    pub fn new(slice_id: SliceId, collection_index: usize) -> Self {
        Self {
            slice_id,
            collection_index,
        }
    }

    pub fn slice_id(&self) -> SliceId {
        self.slice_id
    }

    pub fn collection_index(&self) -> usize {
        self.collection_index
    }
}
