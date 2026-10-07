use pixui_base::PixuiResult;
use pixui_reflect::DynamicObject;

use super::{
    context::ExpressionContext,
    expression::{Expression, ExpressionKind},
};

/// Evaluates indexed storage access, computed values, or registered i18n messages.
/// I18n returns an owned reflected String using the context language and source
/// fallback; arguments evaluate once. Computed callbacks may return borrowed or
/// owned values, and must not hide translatable declarations.
///
/// The result borrows application storage or the current value, independently of
/// the expression's lifetime. Field expressions require a reflected object getter.
/// Foreign collection indices return errors without mutation.
/// Registration order determines collection indices; evaluation does not resolve names.
/// Collections must opt into reflected access through `Collection::new_reflected`.
/// Collection results use shared sequence storage: live items can be inspected
/// but not mutated. The caller must check that field results are sequences.
pub fn evaluate<'a>(
    context: &ExpressionContext<'a>,
    expression: &Expression,
) -> PixuiResult<DynamicObject<'a>> {
    evaluate_depth(context, expression, 0)
}

fn evaluate_depth<'a>(
    context: &ExpressionContext<'a>,
    expression: &Expression,
    depth: usize,
) -> PixuiResult<DynamicObject<'a>> {
    if depth >= crate::i18n::template::MAX_NESTING {
        return Err(pixui_base::pixui_error!(
            "i18n evaluation nesting exceeds limit"
        ));
    }
    match expression.kind() {
        ExpressionKind::Computed(resolve) => resolve(context),
        ExpressionKind::I18n(message) => {
            let template = context
                .application()?
                .translations()
                .template(message.index, context.language())?;
            let mut arguments = Vec::with_capacity(message.arguments.len());
            let mut argument_bytes = 0;
            for (name, expression) in &message.arguments {
                let value = evaluate_depth(context, expression, depth + 1)?;
                let text = scalar_text(&value)?;
                if text.len() > crate::i18n::template::MAX_OUTPUT_BYTES - argument_bytes {
                    return Err(pixui_base::pixui_error!("i18n arguments exceed byte limit"));
                }
                argument_bytes += text.len();
                arguments.push((name.clone(), text));
            }
            Ok(DynamicObject::from_reflect(template.render(&arguments)?))
        }
        ExpressionKind::Entity(reference) => context.application()?.read_entity(reference),
        ExpressionKind::Field(index) => context.value()?.read_object(*index),
        ExpressionKind::Collection(key) => context
            .application()?
            .resolve_collection(*key)?
            .as_sequence(),
    }
}

/// Formatting is deliberately locale independent; arbitrary objects are not debug-formatted.
fn scalar_text(value: &DynamicObject<'_>) -> PixuiResult<String> {
    if let Some(text) = value.downcast_ref::<String>() {
        check_text_size(text.len())?;
        return Ok(text.clone());
    }
    if let Some(text) = value.downcast_ref::<pixui_base::PixuiString>() {
        check_text_size(text.len())?;
        return Ok(text.to_string());
    }
    macro_rules! scalar {
        ($($ty:ty),*) => { $(if let Some(value) = value.downcast_ref::<$ty>() { return Ok(value.to_string()); })* };
    }
    scalar!(
        bool, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64
    );
    Err(pixui_base::pixui_error!(
        "unsupported i18n argument type; expected text, boolean, or number"
    ))
}

fn check_text_size(bytes: usize) -> PixuiResult<()> {
    if bytes > crate::i18n::template::MAX_OUTPUT_BYTES {
        return Err(pixui_base::pixui_error!("i18n argument exceeds byte limit"));
    }
    Ok(())
}
