//! Append-only registry. IDs do not own trees, preventing Arc ownership cycles.
use super::component_ref::ComponentRef;
use crate::live_model::part::LivePart;
use pixui_base::{PixuiResult, pixui_error};
use std::sync::atomic::{AtomicU64, Ordering};
#[derive(Clone)]
pub(crate) struct Entry {
    pub name: String,
    pub domain: String,
    pub template: std::sync::Arc<LivePart>,
    pub unbound: bool,
}
#[derive(Clone)]
pub(crate) struct Registry {
    owner: u64,
    entries: Vec<Entry>,
}
impl Default for Registry {
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let owner = NEXT
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("component registry identity exhausted");
        Self {
            owner,
            entries: vec![],
        }
    }
}
impl Registry {
    pub fn validate_styles(
        &self,
        painters: &crate::painters::registry::PainterRegistry,
        styles: &crate::styles::registry::StyleRegistry,
    ) -> PixuiResult<()> {
        for entry in &self.entries {
            painters.validate_template_styles(&entry.template, styles)?;
        }
        Ok(())
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    /// Validate statically known references without recursively expanding shared
    /// definitions. Data-dependent references are checked at the render site.
    pub fn validate(&self, root: &LivePart, domain: &str) -> PixuiResult<()> {
        use crate::expression::{expression::ExpressionKind, program::ProgramKind};
        let mut pending = vec![root];
        let mut composite_seen = std::collections::HashSet::new();
        while let Some(part) = pending.pop() {
            match part {
                LivePart::Invoke(part) => {
                    if composite_seen.insert(part.definition.id) {
                        pending.push(&part.definition.body);
                    }
                    pending.push(&part.children);
                }
                LivePart::Slot => {}
                LivePart::Dynamic(part) => {
                    let reference = match part.component.kind() {
                        ExpressionKind::ComponentRef(reference) => Some(*reference),
                        ExpressionKind::Program(program) => match &program.kind {
                            ProgramKind::Constant { value, .. } => {
                                value.downcast_ref::<ComponentRef>().copied()
                            }
                            _ => None,
                        },
                        _ => None,
                    };
                    if let Some(reference) = reference {
                        let entry = self.get(reference)?;
                        if entry.domain != domain {
                            return Err(pixui_error!(
                                "component `{}` has incompatible translation domain",
                                entry.name
                            ));
                        }
                    }
                }
                LivePart::Composite(part) => pending.extend(&part.parts),
                LivePart::Container(part) => pending.extend(&part.children),
                LivePart::ForLoop(part) => pending.push(&part.body),
                LivePart::Match(part) => {
                    pending.extend(part.candidates.iter().map(|candidate| &candidate.part))
                }
                LivePart::Component(_) => {}
            }
        }
        Ok(())
    }
    pub fn insert(&mut self, entry: Entry) -> PixuiResult<ComponentRef> {
        if self.entries.len() >= 4096 {
            return Err(pixui_error!("component reference registry limit exceeded"));
        }
        let reference = ComponentRef {
            owner: self.owner,
            index: self.entries.len(),
        };
        self.entries.push(entry);
        Ok(reference)
    }
    pub fn get(&self, reference: ComponentRef) -> PixuiResult<&Entry> {
        if reference.owner != self.owner {
            return Err(pixui_error!("foreign component reference"));
        }
        self.entries
            .get(reference.index)
            .ok_or_else(|| pixui_error!("unknown component reference"))
    }
}
