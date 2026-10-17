//! Relocate detached bundle symbols without evaluating application expressions.
use crate::{
    composition::component_ref::ComponentRef,
    expression::{
        expression::{Expression, ExpressionKind},
        program::ProgramKind,
    },
    live_model::part::LivePart,
};
use pixui_base::{PixuiResult, pixui_error};
use std::sync::Arc;
fn reference(value: &mut ComponentRef, links: &[ComponentRef]) -> PixuiResult<()> {
    if value.owner == 0 {
        *value = *links
            .get(value.index)
            .ok_or_else(|| pixui_error!("invalid UI bundle symbol"))?;
    }
    Ok(())
}
fn expression(value: &mut Expression, links: &[ComponentRef]) -> PixuiResult<()> {
    match value.kind_mut() {
        ExpressionKind::ComponentRef(value) => reference(value, links)?,
        ExpressionKind::Program(program) => {
            let program = Arc::make_mut(program);
            if let ProgramKind::Constant { value, .. } = &mut program.kind
                && let Some(found) = value.downcast_ref::<ComponentRef>()
            {
                let mut found = *found;
                reference(&mut found, links)?;
                *value = Arc::new(found);
            }
            for child in &mut program.children {
                expression(child, links)?;
            }
        }
        ExpressionKind::I18n(message) => {
            for (_, child) in &mut message.arguments {
                expression(child, links)?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub(crate) fn link(root: &mut LivePart, links: &[ComponentRef]) -> PixuiResult<()> {
    crate::composition::traversal::prepare(root, |part| {
        match part {
            LivePart::Invoke(part) => {
                for (value, _) in &mut part.values {
                    expression(value, links)?;
                }
            }
            LivePart::Dynamic(part) => expression(&mut part.component, links)?,
            LivePart::Component(part) => {
                for value in &mut part.expressions {
                    expression(value, links)?;
                }
                for callback in &mut part.events {
                    if let crate::events::binding::Callback::Program(program) =
                        &mut callback.callback
                    {
                        for argument in &mut Arc::make_mut(program).arguments {
                            if let crate::events::program::RequestArgument::Value {
                                expression: value,
                                ..
                            } = argument
                            {
                                expression(value, links)?;
                            }
                        }
                    }
                }
            }
            LivePart::ForLoop(part) => expression(&mut part.expression, links)?,
            LivePart::Match(part) => expression(&mut part.expression, links)?,
            _ => {}
        }
        Ok(())
    })
}
