//! Numeric literals are converted only with a concrete expected primitive type.
use super::typed::{Expression, ExpressionKind};
use crate::ui_language::{
    diagnostic::report::{Diagnostic, Stage},
    resolution::environment::{Environment, ValueType},
    source::span::Span,
};
use pixui_reflect::Reflect;
use std::{any::TypeId, sync::Arc};
pub fn number(text: &str, expected: Option<TypeId>, span: Span) -> Result<Expression, Diagnostic> {
    let expected = expected.unwrap_or(if text.contains('.') {
        TypeId::of::<f64>()
    } else {
        TypeId::of::<i64>()
    });
    macro_rules! convert {($($ty:ty),*)=>{$(if expected==TypeId::of::<$ty>() {let value=text.parse::<$ty>().map_err(|_|Diagnostic::new(Stage::Checking,"T001",span,format!("number is out of range or invalid for {}",std::any::type_name::<$ty>())))?;return Ok(Expression {kind:ExpressionKind::Constant {value:Arc::new(value),ty:ValueType::of::<$ty>()},descriptor:<$ty>::type_descriptor(),span});})*};}
    convert!(
        i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
    );
    macro_rules! float {($($ty:ty),*)=>{$(if expected==TypeId::of::<$ty>() {let value=text.parse::<$ty>().map_err(|_|Diagnostic::new(Stage::Checking,"T001",span,"invalid floating-point literal"))?;if !value.is_finite() {return Err(Diagnostic::new(Stage::Checking,"T001",span,"floating-point literal overflows"));}return Ok(Expression {kind:ExpressionKind::Constant {value:Arc::new(value),ty:ValueType::of::<$ty>()},descriptor:<$ty>::type_descriptor(),span});})*};}
    float!(f32, f64);
    Err(Diagnostic::new(
        Stage::Checking,
        "T002",
        span,
        "numeric literal requires a numeric type",
    ))
}
pub fn value_type(
    environment: &Environment,
    id: TypeId,
    span: Span,
) -> Result<ValueType, Diagnostic> {
    environment.types.get(&id).cloned().ok_or_else(|| {
        Diagnostic::new(
            Stage::Checking,
            "T003",
            span,
            "type needs an exported reflection/ownership adapter",
        )
    })
}
