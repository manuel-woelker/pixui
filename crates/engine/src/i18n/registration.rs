//! Collect the abstract tree, including hidden arms and unexpanded loop templates.
use super::registry::{TranslationRegistry, validate_domain};
use crate::{live_model::part::LivePart, ui::definition::UiDefinition};
use pixui_base::{PixuiResult, pixui_error};

pub(crate) fn register(
    definition: &mut UiDefinition,
    registry: &mut TranslationRegistry,
) -> PixuiResult<()> {
    let domain = &definition.translation_domain;
    validate_domain(domain)?;
    let mut pending = vec![(&mut definition.template, 0)];
    while let Some((part, depth)) = pending.pop() {
        // Bound template depth too: it is cloned before rendering.
        if depth > 1024 {
            return Err(pixui_error!(
                "UI definition nesting exceeds registration limit"
            ));
        }
        match part {
            LivePart::Component(component) => {
                for expression in &mut component.expressions {
                    registry.resolve(domain, expression, 0)?;
                }
            }
            LivePart::Composite(composite) => {
                pending.extend(
                    composite
                        .parts
                        .iter_mut()
                        .rev()
                        .map(|part| (part, depth + 1)),
                );
            }
            LivePart::Container(container) => {
                pending.extend(
                    container
                        .children
                        .iter_mut()
                        .rev()
                        .map(|part| (part, depth + 1)),
                );
            }
            LivePart::ForLoop(part) => {
                registry.resolve(domain, &mut part.expression, 0)?;
                pending.push((&mut part.body, depth + 1));
            }
            LivePart::Match(part) => {
                registry.resolve(domain, &mut part.expression, 0)?;
                pending.extend(
                    part.candidates
                        .iter_mut()
                        .rev()
                        .map(|arm| (&mut arm.part, depth + 1)),
                );
            }
        }
    }
    if let Some(binding) = &mut definition.window_expressions {
        for expression in &mut binding.expressions {
            registry.resolve(domain, expression, 0)?;
        }
    }
    Ok(())
}
