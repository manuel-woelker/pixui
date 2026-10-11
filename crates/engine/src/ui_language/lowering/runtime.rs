//! Checked expression programs reuse existing application and i18n evaluation.
use crate::{
    expression::{context::ExpressionContext, evaluator::evaluate, expression::Expression},
    ui_language::{
        ast::expression::Operator,
        resolution::environment::{Snapshot, ValueType},
        source::origin::Origin,
    },
};
use pixui_base::{PixuiResult, pixui_error};
use pixui_reflect::{DynamicObject, FieldIndex};
use std::any::{Any, TypeId};
#[derive(Clone)]
pub enum PathRoot {
    Local(usize),
    Data,
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
    },
    Not,
    Binary(Operator),
    Template(crate::i18n::template::CompiledTemplate, Vec<String>),
}
#[derive(Clone)]
pub struct Program {
    pub kind: ProgramKind,
    pub children: Vec<Expression>,
    pub origin: Origin,
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
                owned_scalar(value.as_ref(), ty.descriptor.type_id())
            }
            ProgramKind::Path { root, fields } => {
                let mut value = match root {
                    PathRoot::Local(depth) => context.local(*depth)?.shared_view()?,
                    PathRoot::Data => evaluate(context, &self.children[0])?,
                };
                for field in fields {
                    value = value.into_read_with_descriptor(field.0, field.1)?;
                }
                Ok(value)
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

#[derive(Clone)]
pub struct ActionProgram {
    pub handle: crate::application::action_handle::ActionHandle,
    pub arguments: Vec<RequestArgument>,
    pub origin: Origin,
}
#[derive(Clone)]
pub enum RequestArgument {
    Value {
        expression: usize,
        ty: ValueType,
    },
    Reference {
        root: crate::ui_language::checking::typed::ReferenceRoot,
        export: crate::ui_language::resolution::environment::ReferenceExport,
    },
}
impl ActionProgram {
    pub fn prepare(
        &self,
        context: &ExpressionContext<'_>,
        values: &[DynamicObject<'_>],
    ) -> PixuiResult<crate::ui::activation::ActionBinding> {
        let result = (|| {
            let mut snapshots = vec![];
            for argument in &self.arguments {
                let (snapshot, ty) = match argument {
                    RequestArgument::Value { expression, ty } => {
                        ((ty.snapshot)(&values[*expression])?, ty.clone())
                    }
                    RequestArgument::Reference { root, export } => {
                        let snapshot = match root {
                            crate::ui_language::checking::typed::ReferenceRoot::Local(depth) => {
                                let (collection, position) = context.local_address(*depth)?;
                                (export.make)(context.application()?, collection, position)?
                            }
                            crate::ui_language::checking::typed::ReferenceRoot::Entity(
                                reference,
                            ) => (export.entity)(reference)?,
                        };
                        (snapshot, export.value.clone())
                    }
                };
                snapshots.push((snapshot, ty));
            }
            let handle = self.handle;
            let origin = self.origin.clone();
            Ok(
                Box::new(move |_application: &crate::application::app::Application| {
                    let fields = snapshots
                        .iter()
                        .map(|(value, ty)| (ty.send)(value.as_ref()))
                        .collect::<PixuiResult<Vec<_>>>()?;
                    handle.call(fields).map_err(|error| origin.error(error))
                }) as crate::ui::activation::ActionBinding,
            )
        })();
        result.map_err(|error| self.origin.error(error))
    }
}
