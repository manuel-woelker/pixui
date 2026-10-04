use pixui_base::{PixuiResult, pixui_error};
use pixui_reflect::DynamicObject;

use super::{
    context::ExpressionContext,
    expression::{Expression, ExpressionKind},
};

/// Evaluates a collection address against borrowed application state.
///
/// The result borrows the application, independently of the expression's lifetime.
/// Missing slices or out-of-range collection indices return errors without mutation.
/// Registration order determines collection indices; evaluation does not resolve names.
/// Collections must opt into reflected access through `Collection::new_reflected`.
/// Results use shared sequence storage: live items can be inspected but not mutated.
pub fn evaluate<'a>(
    context: &ExpressionContext<'a>,
    expression: &Expression,
) -> PixuiResult<DynamicObject<'a>> {
    match expression.kind() {
        ExpressionKind::Collection(expression) => {
            let slice = context.application().slice(expression.slice_id())?;
            let index = expression.collection_index();
            let collection = slice.collections().get(index).ok_or_else(|| {
                pixui_error!(
                    "invalid collection index {index} in slice `{}`",
                    slice.name()
                )
            })?;
            collection.as_sequence()
        }
    }
}
