//! Mutating declaration passes preserve shared composite bodies within a definition.
use super::definition::CompositeDefinition;
use crate::live_model::part::LivePart;
use pixui_base::{PixuiResult, pixui_error};
use std::{collections::HashMap, sync::Arc};
/// Visits every call site/slot but prepares each immutable body once, then reuses
/// that prepared Arc. This avoids exponential expansion of shared source DAGs.
pub(crate) fn prepare(
    root: &mut LivePart,
    mut visit: impl FnMut(&mut LivePart) -> PixuiResult<()>,
) -> PixuiResult<()> {
    descend(root, &mut visit, &mut HashMap::new(), 0)
}
fn descend(
    root: &mut LivePart,
    visit: &mut impl FnMut(&mut LivePart) -> PixuiResult<()>,
    cache: &mut HashMap<u64, Arc<CompositeDefinition>>,
    depth: usize,
) -> PixuiResult<()> {
    if depth > 1024 {
        return Err(pixui_error!(
            "UI definition nesting exceeds registration limit"
        ));
    }
    visit(root)?;
    match root {
        LivePart::Invoke(part) => {
            if let Some(definition) = cache.get(&part.definition.id) {
                part.definition = definition.clone();
            } else {
                let mut definition = part.definition.as_ref().clone();
                descend(&mut definition.body, visit, cache, depth + 1)?;
                let definition = Arc::new(definition);
                cache.insert(definition.id, definition.clone());
                part.definition = definition;
            }
            descend(&mut part.children, visit, cache, depth + 1)?;
        }
        LivePart::Composite(part) => {
            for child in &mut part.parts {
                descend(child, visit, cache, depth + 1)?;
            }
        }
        LivePart::Container(part) => {
            for child in &mut part.children {
                descend(child, visit, cache, depth + 1)?;
            }
        }
        LivePart::ForLoop(part) => descend(&mut part.body, visit, cache, depth + 1)?,
        LivePart::Match(part) => {
            for arm in &mut part.candidates {
                descend(&mut arm.part, visit, cache, depth + 1)?;
            }
        }
        LivePart::Component(_) | LivePart::Dynamic(_) | LivePart::Slot => {}
    }
    Ok(())
}
