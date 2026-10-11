//! Retained solver state with conservative invalidation for arbitrary painters.
//!
//! Props and component state are type-erased and need not implement equality.
//! Instead, every distinct measurement used by the last complete solve is
//! checked against this frame's prepared inputs. Equal answers and equal styles
//! let the deterministic solver reuse its results. No props or state are retained.

use crate::{painters::measure::MeasureConstraints, ui::geometry::Size};
use pixui_base::PixuiResult;
use taffy::{NodeId, TaffyTree};

/// Postorder physical hierarchy. Children refer to earlier entries; component
/// indices refer to this frame's prepared arrays, never persistent identities.
#[derive(PartialEq)]
pub(crate) struct Node {
    pub style: taffy::Style,
    pub children: Vec<usize>,
    pub component: Option<usize>,
}

pub(crate) struct Probe {
    pub constraints: MeasureConstraints,
    pub size: Size,
}

/// Owned by one UI instance. Style or measurement changes clear all solver
/// entries and probes; structural changes rebuild the hierarchy. This bounds
/// probe history to one full solve instead of accumulating viewport histories.
pub(crate) struct LayoutCache {
    pub tree: TaffyTree<usize>,
    pub ids: Vec<NodeId>,
    pub probes: Vec<Vec<Probe>>,
    nodes: Vec<Node>,
}

impl Default for LayoutCache {
    fn default() -> Self {
        let mut tree = TaffyTree::new();
        tree.disable_rounding();
        Self {
            tree,
            ids: Vec::new(),
            probes: Vec::new(),
            nodes: Vec::new(),
        }
    }
}

impl LayoutCache {
    pub fn sync(&mut self, nodes: Vec<Node>, components: usize) -> PixuiResult<()> {
        let same_structure = self.nodes.len() == nodes.len()
            && self.nodes.iter().zip(&nodes).all(|(previous, current)| {
                previous.children == current.children && previous.component == current.component
            });
        if !same_structure {
            *self = Self::default();
            for node in &nodes {
                let id = if let Some(component) = node.component {
                    self.tree
                        .new_leaf_with_context(node.style.clone(), component)
                } else {
                    let children: Vec<_> =
                        node.children.iter().map(|index| self.ids[*index]).collect();
                    self.tree.new_with_children(node.style.clone(), &children)
                }
                .map_err(super::adapter::failure)?;
                self.ids.push(id);
            }
            self.probes = (0..components).map(|_| Vec::new()).collect();
        } else if self.nodes != nodes {
            for ((previous, current), id) in self.nodes.iter().zip(&nodes).zip(&self.ids) {
                if previous.style != current.style {
                    self.tree
                        .set_style(*id, current.style.clone())
                        .map_err(super::adapter::failure)?;
                }
            }
            self.invalidate()?;
        }
        self.nodes = nodes;
        Ok(())
    }

    pub fn validate(
        &mut self,
        measure: &mut impl FnMut(usize, MeasureConstraints) -> PixuiResult<Size>,
        measurements: &mut usize,
    ) -> PixuiResult<()> {
        for (index, probes) in self.probes.iter().enumerate() {
            for probe in probes {
                *measurements += 1;
                if measure(index, probe.constraints)? != probe.size {
                    self.invalidate()?;
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    fn invalidate(&mut self) -> PixuiResult<()> {
        // Clearing every node also drops cached results for constraints that
        // were not visited in the latest frame. No unseen stale entries survive.
        for id in &self.ids {
            self.tree.mark_dirty(*id).map_err(super::adapter::failure)?;
        }
        for probes in &mut self.probes {
            probes.clear();
        }
        Ok(())
    }
}
