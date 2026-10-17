//! Checked expression programs reuse existing application and i18n evaluation.
use crate::events::value::{Snapshot, ValueType};
use crate::{
    expression::{context::ExpressionContext, evaluator::evaluate, expression::Expression},
    ui_language::{ast::expression::Operator, source::origin::Origin},
};
use pixui_base::{PixuiResult, pixui_error};
use pixui_reflect::{DynamicObject, FieldIndex};
use std::any::{Any, TypeId};
#[derive(Clone)]
pub enum PathRoot {
    Local(usize),
    Data,
    Event,
}
#[derive(Clone)]
pub enum ProgramKind {
    Constant {
        value: Snapshot,
        ty: ValueType,
    },
    Path {
        root: PathRoot,
        fields: Vec<(FieldIndex, &'static pixui_reflect::TypeDescriptor)>,
        owned_result: Option<ValueType>,
    },
    ResourcePath,
    LocalizedText,
    Not,
    Binary(Operator),
    Template(crate::i18n::template::CompiledTemplate, Vec<String>),
}
#[derive(Clone)]
pub struct Program {
    pub kind: ProgramKind,
    pub children: Vec<Expression>,
    pub origin: Origin,
    pub capture_type: Option<ValueType>,
}
impl Program {
    pub fn evaluate<'a>(&self, context: &ExpressionContext<'a>) -> PixuiResult<DynamicObject<'a>> {
        self.evaluate_inner(context)
            .map_err(|error| self.origin.error(error))
    }
    fn evaluate_inner<'a>(
        &self,
        context: &ExpressionContext<'a>,
    ) -> PixuiResult<DynamicObject<'a>> {
        match &self.kind {
            ProgramKind::Constant { value, ty } => {
                DynamicObject::from_send((ty.send)(value.as_ref())?, ty.descriptor)
            }
            ProgramKind::Path {
                root,
                fields,
                owned_result,
            } => {
                let mut value = match root {
                    PathRoot::Local(depth) => context.local(*depth)?.shared_view()?,
                    PathRoot::Data => evaluate(context, &self.children[0])?,
                    PathRoot::Event => context.event()?.shared_view()?,
                };
                if value.is_owned() && !fields.is_empty() {
                    let mut borrowed = value.shared_view()?;
                    for field in fields {
                        borrowed = borrowed.into_read_with_descriptor(field.0, field.1)?;
                    }
                    let ty = owned_result.as_ref().ok_or_else(|| {
                        pixui_error!("owned field root requires a result adapter")
                    })?;
                    let snapshot = (ty.snapshot)(&borrowed)?;
                    return DynamicObject::from_send((ty.send)(snapshot.as_ref())?, ty.descriptor);
                }
                for field in fields {
                    value = value.into_read_with_descriptor(field.0, field.1)?;
                }
                Ok(value)
            }
            ProgramKind::LocalizedText => {
                let value = evaluate(context, &self.children[0])?;
                let text = value
                    .downcast_ref::<crate::composition::localized_text::LocalizedText>()
                    .ok_or_else(|| pixui_error!("expected LocalizedText"))?;
                if context.domain.is_some_and(|domain| domain != text.domain) {
                    return Err(pixui_error!(
                        "localized text has incompatible translation domain"
                    ));
                }
                evaluate(context, &text.expression)
            }
            ProgramKind::ResourcePath => {
                let value = evaluate(context, &self.children[0])?;
                let text = value
                    .downcast_ref::<String>()
                    .ok_or_else(|| pixui_error!("resource path conversion requires a string"))?;
                Ok(DynamicObject::from_reflect(
                    crate::resources::path::ResourcePath::new(text.clone())?,
                ))
            }
            ProgramKind::Not => Ok(DynamicObject::from_reflect(!boolean(&evaluate(
                context,
                &self.children[0],
            )?)?)),
            ProgramKind::Binary(op) => {
                let left = evaluate(context, &self.children[0])?;
                let result = match op {
                    Operator::And => {
                        boolean(&left)? && boolean(&evaluate(context, &self.children[1])?)?
                    }
                    Operator::Or => {
                        boolean(&left)? || boolean(&evaluate(context, &self.children[1])?)?
                    }
                    Operator::Equal | Operator::NotEqual => {
                        let right = evaluate(context, &self.children[1])?;
                        let equal = equal(&left, &right)?;
                        if *op == Operator::Equal {
                            equal
                        } else {
                            !equal
                        }
                    }
                };
                Ok(DynamicObject::from_reflect(result))
            }
            ProgramKind::Template(template, names) => {
                let arguments = self
                    .children
                    .iter()
                    .zip(names)
                    .map(|(expression, name)| {
                        Ok((
                            name.clone(),
                            crate::expression::evaluator::scalar_text(&evaluate(
                                context, expression,
                            )?)?,
                        ))
                    })
                    .collect::<PixuiResult<Vec<_>>>()?;
                Ok(DynamicObject::from_reflect(template.render(&arguments)?))
            }
        }
    }
}
fn boolean(value: &DynamicObject<'_>) -> PixuiResult<bool> {
    value
        .downcast_ref::<bool>()
        .copied()
        .ok_or_else(|| pixui_error!("expected bool"))
}
pub(crate) fn is_scalar(id: TypeId) -> bool {
    macro_rules! scalar {($($ty:ty),*)=>{false $(||id==TypeId::of::<$ty>())*};}
    scalar!(
        String, bool, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64
    )
}
fn equal(left: &DynamicObject<'_>, right: &DynamicObject<'_>) -> PixuiResult<bool> {
    macro_rules! scalar {($($ty:ty),*)=>{$(if let Some(left)=left.downcast_ref::<$ty>() {return right.downcast_ref::<$ty>().map(|right|left==right).ok_or_else(||pixui_error!("comparison type mismatch"));})*};}
    scalar!(
        String, bool, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64
    );
    Err(pixui_error!("unsupported comparison type"))
}
pub(crate) fn owned_scalar(value: &dyn Any, id: TypeId) -> PixuiResult<DynamicObject<'static>> {
    macro_rules! scalar {($($ty:ty),*)=>{$(if id==TypeId::of::<$ty>() {return value.downcast_ref::<$ty>().cloned().map(DynamicObject::from_reflect).ok_or_else(||pixui_error!("incorrect constant type"));})*};}
    scalar!(
        String, bool, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64
    );
    Err(pixui_error!("unsupported constant type"))
}
pub(crate) fn scalar_snapshot(value: &dyn Any, id: TypeId) -> PixuiResult<String> {
    crate::expression::evaluator::scalar_text(&owned_scalar(value, id)?)
}
