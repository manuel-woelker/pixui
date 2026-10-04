use pixui_base::PixuiResult;
use pixui_reflect::DynamicObject;

use super::{
    context::ExpressionContext,
    expression::{Expression, ExpressionKind},
};

/// Evaluates a reflected field or collection address against borrowed inputs.
///
/// The result borrows application storage or the current value, independently of
/// the expression's lifetime. Field expressions require a reflected object getter.
/// Missing slices or out-of-range collection indices return errors without mutation.
/// Registration order determines collection indices; evaluation does not resolve names.
/// Collections must opt into reflected access through `Collection::new_reflected`.
/// Collection results use shared sequence storage: live items can be inspected
/// but not mutated. The caller must check that field results are sequences.
pub fn evaluate<'a>(
    context: &ExpressionContext<'a>,
    expression: &Expression,
) -> PixuiResult<DynamicObject<'a>> {
    match expression.kind() {
        ExpressionKind::Field(index) => context.value()?.read_object(*index),
        ExpressionKind::Collection(key) => context
            .application()?
            .resolve_collection(*key)?
            .as_sequence(),
    }
}
