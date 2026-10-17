//! Callback programs capture published values and evaluate payload-dependent work on emission.
use super::value::{Snapshot, ValueType};
use crate::expression::program::{PathRoot, ProgramKind};
use crate::{
    expression::{
        context::ExpressionContext,
        evaluator::evaluate,
        expression::{Expression, ExpressionKind},
    },
    ui_language::source::origin::Origin,
};
use pixui_base::{PixuiResult, pixui_error};
use std::sync::Arc;
#[derive(Clone)]
pub struct ActionProgram {
    pub handle: crate::application::action_handle::ActionHandle,
    pub arguments: Vec<RequestArgument>,
    pub origin: Origin,
}
#[derive(Clone)]
pub enum RequestArgument {
    Value {
        expression: Expression,
        ty: ValueType,
    },
    Reference {
        root: ReferenceRoot,
        export: super::value::ReferenceExport,
    },
}
impl ActionProgram {
    /// Captures owned roots now; preserves lazy operations and payload accesses.
    pub fn prepare(
        &self,
        context: &ExpressionContext<'_>,
    ) -> PixuiResult<super::binding::EventBinding> {
        let result = (|| {
            let mut arguments = vec![];
            for argument in &self.arguments {
                arguments.push(match argument {
                    RequestArgument::Value { expression, ty } => {
                        (capture(expression.clone(), context)?, ty.clone())
                    }
                    RequestArgument::Reference { root, export } => {
                        let value = match root {
                            ReferenceRoot::Local(depth) => {
                                let (collection, position) = context.local_address(*depth)?;
                                (export.make)(context.application()?, collection, position)?
                            }
                            ReferenceRoot::Entity(reference) => (export.entity)(reference)?,
                        };
                        (
                            constant(value, export.value.clone(), self.origin.clone()),
                            export.value.clone(),
                        )
                    }
                });
            }
            let handle = self.handle;
            let origin = self.origin.clone();
            let language = context.language();
            let domain = context.domain.map(str::to_owned);
            Ok(Box::new(
                move |app: &crate::application::app::Application,
                      payload: &pixui_reflect::DynamicObject<'_>| {
                    let result = (|| {
                        let context = ExpressionContext::new(app)
                            .with_language(language)
                            .with_domain(domain.as_deref())
                            .with_event(payload);
                        let fields = arguments
                            .iter()
                            .map(|(expression, ty)| {
                                let value = evaluate(&context, expression)?;
                                let snapshot = (ty.snapshot)(&value)?;
                                (ty.send)(snapshot.as_ref())
                            })
                            .collect::<PixuiResult<Vec<_>>>()?;
                        let mut call = handle.call(fields)?;
                        call.callback_origin = Some(Box::new(origin.clone()));
                        Ok(call)
                    })();
                    result.map_err(|error| origin.error(error))
                },
            ) as super::binding::EventBinding)
        })();
        result.map_err(|error| self.origin.error(error))
    }
}
fn constant(value: Snapshot, ty: ValueType, origin: Origin) -> Expression {
    Expression::new(ExpressionKind::Program(Arc::new(
        crate::expression::program::Program {
            kind: ProgramKind::Constant { value, ty },
            children: vec![],
            origin,
            capture_type: None,
        },
    )))
}
/// Capture root values, not field-chain results: a payload-controlled branch may
/// legitimately skip a field access that would fail when evaluated eagerly.
fn capture(mut expression: Expression, context: &ExpressionContext<'_>) -> PixuiResult<Expression> {
    match expression.kind_mut() {
        ExpressionKind::Program(program) => {
            let program = Arc::make_mut(program);
            match &mut program.kind {
                ProgramKind::Path {
                    root: PathRoot::Prop(index),
                    ..
                } => {
                    let ty = program
                        .capture_type
                        .clone()
                        .ok_or_else(|| pixui_error!("callback prop needs ownership adapter"))?;
                    let value = (ty.snapshot)(context.prop(*index)?)?;
                    if let ProgramKind::Path { root, .. } = &mut program.kind {
                        *root = PathRoot::Data;
                    }
                    program.children = vec![constant(value, ty, program.origin.clone())];
                }
                ProgramKind::Path {
                    root: PathRoot::Local(depth),
                    ..
                } => {
                    let ty = program
                        .capture_type
                        .clone()
                        .ok_or_else(|| pixui_error!("callback root needs ownership adapter"))?;
                    let value = (ty.snapshot)(context.local(*depth)?)?;
                    if let ProgramKind::Path { root, .. } = &mut program.kind {
                        *root = PathRoot::Data;
                    }
                    program.children = vec![constant(value, ty, program.origin.clone())];
                }
                ProgramKind::Path {
                    root: PathRoot::Data,
                    ..
                } if program.capture_type.is_some() => {
                    let ty = program.capture_type.clone().expect("checked adapter");
                    let value = (ty.snapshot)(&evaluate(context, &program.children[0])?)?;
                    program.children = vec![constant(value, ty, program.origin.clone())];
                    program.capture_type = None;
                }
                _ => {
                    program.children = std::mem::take(&mut program.children)
                        .into_iter()
                        .map(|child| capture(child, context))
                        .collect::<PixuiResult<_>>()?;
                }
            }
        }
        ExpressionKind::I18n(message) => {
            for (_, argument) in &mut message.arguments {
                *argument = capture(argument.clone(), context)?;
            }
        }
        _ => return Err(pixui_error!("unexpected uncaptured callback expression")),
    }
    Ok(expression)
}

#[derive(Clone)]
pub enum ReferenceRoot {
    Local(usize),
    Entity(crate::application::erased_object_ref::ErasedObjectRef),
}
