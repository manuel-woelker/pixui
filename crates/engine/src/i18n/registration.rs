//! Collect the abstract tree, including hidden arms and unexpanded loop templates.
use super::registry::{TranslationRegistry, validate_domain};
use crate::{live_model::part::LivePart, ui::definition::UiDefinition};
use pixui_base::PixuiResult;

pub(crate) fn register(
    definition: &mut UiDefinition,
    registry: &mut TranslationRegistry,
) -> PixuiResult<()> {
    let domain = &definition.translation_domain;
    validate_domain(domain)?;
    crate::composition::traversal::prepare(&mut definition.template, |part| {
        match part {
            LivePart::Invoke(part) => {
                for (expression, _) in &mut part.values {
                    registry.resolve(domain, expression, 0)?;
                }
            }
            LivePart::Dynamic(part) => {
                part.domain = Some(domain.clone());
                registry.resolve(domain, &mut part.component, 0)?;
            }
            LivePart::Component(component) => {
                for callback in &mut component.events {
                    if let crate::events::binding::Callback::Program(program) =
                        &mut callback.callback
                    {
                        for argument in &mut std::sync::Arc::make_mut(program).arguments {
                            if let crate::events::program::RequestArgument::Value {
                                expression,
                                ..
                            } = argument
                            {
                                registry.resolve(domain, expression, 0)?;
                            }
                        }
                    }
                }
                for expression in &mut component.expressions {
                    registry.resolve(domain, expression, 0)?;
                }
            }
            LivePart::ForLoop(part) => registry.resolve(domain, &mut part.expression, 0)?,
            LivePart::Match(part) => registry.resolve(domain, &mut part.expression, 0)?,
            _ => {}
        }
        Ok(())
    })?;
    if let Some(binding) = &mut definition.window_expressions {
        for expression in &mut binding.expressions {
            registry.resolve(domain, expression, 0)?;
        }
    }
    Ok(())
}
