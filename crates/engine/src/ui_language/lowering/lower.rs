//! Converts only checked nodes into the existing live-part runtime.
use crate::events::program::{ActionProgram, RequestArgument};
use crate::expression::program::{PathRoot, Program, ProgramKind};
use crate::{
    expression::expression::{Expression, ExpressionKind},
    live_model::{
        match_part::{MatchCandidate, MatchPart, MatchPattern},
        part::{ComponentPart, CompositePart, ForLoopPart, LivePart},
    },
    ui::definition::UiDefinition,
    ui_language::{
        checking::typed::{self, TypedDefinition, TypedPart},
        resolution::resolved::Root,
        source::{origin::Origin, snapshot::Source},
    },
};
use pixui_base::PixuiResult;
use std::sync::Arc;

pub fn lower(definition: TypedDefinition, source: &Source) -> PixuiResult<UiDefinition> {
    let origin = Origin {
        source: source.clone(),
        span: definition.syntax.span,
    };
    let root = part(definition.root, source)?;
    let mut ui = UiDefinition::new(definition.syntax.name.value, root);
    if let Some(domain) = definition.syntax.domain {
        ui = ui.with_translation_domain(domain.value);
    }
    ui.source_origin = Some(origin);
    Ok(ui)
}
fn part(part: TypedPart, source: &Source) -> PixuiResult<LivePart> {
    let origin = Origin {
        source: source.clone(),
        span: part.span,
    };
    let result = match part.kind {
        typed::PartKind::Composite(children) => LivePart::Composite(CompositePart {
            parts: children
                .into_iter()
                .map(|child| self::part(child, source))
                .collect::<PixuiResult<_>>()?,
        }),
        typed::PartKind::Container {
            mut container,
            children,
        } => {
            container.children = children
                .into_iter()
                .map(|child| self::part(child, source))
                .collect::<PixuiResult<_>>()?;
            LivePart::Container(container)
        }
        typed::PartKind::Component {
            export,
            props,
            types,
            callbacks,
            layout,
            styles,
            focus,
        } => {
            let mut component = ComponentPart::default()
                .with_layout(layout)
                .with_focus(focus);
            component.styles = styles;
            component.expressions = props
                .into_iter()
                .map(|value| expression(value, source))
                .collect::<PixuiResult<_>>()?;
            component.binding = Some(Arc::new(super::binding::LocatedBinding {
                inner: (export.build)(types),
                origin: origin.clone(),
            }));
            for (name, action) in callbacks {
                let mut arguments = vec![];
                for argument in action.arguments {
                    arguments.push(match argument {
                        typed::ActionArgument::Value(value, ty) => RequestArgument::Value {
                            expression: expression(value, source)?,
                            ty,
                        },
                        typed::ActionArgument::Reference(root, export) => {
                            RequestArgument::Reference { root, export }
                        }
                    });
                }
                component
                    .events
                    .push(crate::events::binding::EventCallback {
                        index: Some(
                            export
                                .events
                                .iter()
                                .position(|event| event.name == name)
                                .expect("checked event"),
                        ),
                        name,
                        callback: crate::events::binding::Callback::Program(Arc::new(
                            ActionProgram {
                                handle: action.handle,
                                arguments,
                                origin: Origin {
                                    source: source.clone(),
                                    span: action.span,
                                },
                            },
                        )),
                    });
            }
            LivePart::Component(component)
        }
        typed::PartKind::For { sequence, body } => LivePart::ForLoop(ForLoopPart::new(
            expression(sequence, source)?,
            self::part(*body, source)?,
        )),
        typed::PartKind::Match { value, arms } => {
            let candidates = arms
                .into_iter()
                .map(|(pattern, body)| {
                    Ok(MatchCandidate {
                        pattern: pattern
                            .map(|value| match_pattern(value, source))
                            .transpose()?
                            .unwrap_or(MatchPattern::Wildcard),
                        part: self::part(body, source)?,
                    })
                })
                .collect::<PixuiResult<Vec<_>>>()?;
            LivePart::Match(MatchPart::new(expression(value, source)?, candidates)?)
        }
    };
    Ok(result)
}
pub fn expression(value: typed::Expression, source: &Source) -> PixuiResult<Expression> {
    let origin = Origin {
        source: source.clone(),
        span: value.span,
    };
    let mut result = match value.kind {
        typed::ExpressionKind::Path {
            root: Root::Data(data),
            fields,
            capture_type: None,
            capture_result: None,
        } if fields.is_empty() => data.expression,
        typed::ExpressionKind::Template {
            translated: true,
            source: template,
            arguments,
        } => {
            let args = arguments
                .into_iter()
                .map(|(name, value)| Ok((name, expression(value, source)?)))
                .collect::<PixuiResult<Vec<_>>>()?;
            let mut expression = Expression::i18n(template, args)?;
            if let ExpressionKind::I18n(message) = expression.kind_mut() {
                let (line, column) = source.position(origin.span.start).unwrap();
                message.location = format!("{}:{line}:{column}", source.filename);
            }
            expression
        }
        kind => {
            let mut captured = None;
            let (kind, children) = match kind {
                typed::ExpressionKind::Constant { value, ty } => {
                    (ProgramKind::Constant { value, ty }, vec![])
                }
                typed::ExpressionKind::Path {
                    root,
                    fields,
                    capture_type,
                    capture_result,
                } => {
                    captured = capture_type;
                    match root {
                        Root::Local(depth) => (
                            ProgramKind::Path {
                                root: PathRoot::Local(depth),
                                fields,
                                owned_result: capture_result.clone(),
                            },
                            vec![],
                        ),
                        Root::Data(data) => (
                            ProgramKind::Path {
                                root: PathRoot::Data,
                                fields,
                                owned_result: capture_result.clone(),
                            },
                            vec![data.expression],
                        ),
                        Root::Event(_) => (
                            ProgramKind::Path {
                                root: PathRoot::Event,
                                fields,
                                owned_result: capture_result.clone(),
                            },
                            vec![],
                        ),
                        Root::Layout(_) | Root::Styles(_) => {
                            unreachable!("type checker excludes layout/styles expressions")
                        }
                    }
                }
                typed::ExpressionKind::Not(value) => {
                    (ProgramKind::Not, vec![expression(*value, source)?])
                }
                typed::ExpressionKind::Binary { op, left, right } => (
                    ProgramKind::Binary(op),
                    vec![expression(*left, source)?, expression(*right, source)?],
                ),
                typed::ExpressionKind::Template {
                    source: template,
                    arguments,
                    ..
                } => {
                    let (names, values): (Vec<_>, Vec<_>) = arguments.into_iter().unzip();
                    (
                        ProgramKind::Template(
                            crate::i18n::template::CompiledTemplate::parse(&template)?,
                            names,
                        ),
                        values
                            .into_iter()
                            .map(|value| expression(value, source))
                            .collect::<PixuiResult<_>>()?,
                    )
                }
            };
            Expression::new(ExpressionKind::Program(Arc::new(Program {
                kind,
                children,
                origin: origin.clone(),
                capture_type: captured,
            })))
        }
    };
    result.origin = Some(origin);
    Ok(result)
}
fn match_pattern(value: typed::Expression, source: &Source) -> PixuiResult<MatchPattern> {
    let id = value.descriptor.type_id();
    let owned = match value.kind {
        typed::ExpressionKind::Constant { value, .. } => {
            crate::expression::program::owned_scalar(value.as_ref(), id)?
        }
        typed::ExpressionKind::Template { source, .. } => {
            let template = crate::i18n::template::CompiledTemplate::parse(&source)?;
            pixui_reflect::DynamicObject::from_reflect(template.render(&[])?)
        }
        _ => unreachable!("checked constant pattern"),
    };
    macro_rules! pattern {($($ty:ty),*)=>{$(if let Some(value)=owned.downcast_ref::<$ty>() {return Ok(MatchPattern::value(value.clone()));})*};}
    pattern!(
        String, bool, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64
    );
    let _ = source;
    unreachable!("type checker restricts match patterns to supported scalars")
}
