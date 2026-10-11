//! Retained physical layout tree. Expressions and preparation have already
//! completed; only recorded loop counts and match selections are traversed here.
use super::cache::{LayoutCache, Node, Probe};
use crate::{
    live_model::{part::LivePart, state::PartState},
    painters::measure::{AvailableSpace, MeasureConstraints},
    ui::geometry::{Rect, Size},
};
use pixui_base::{PixuiResult, pixui_error};
use taffy::{NodeId, TaffyTree};

pub(crate) struct Geometry {
    pub border: Rect,
    pub content: Rect,
    pub clip: Rect,
    pub navigation_clip: Rect,
}
pub(crate) struct LayoutResult {
    pub leaves: Vec<Geometry>,
    pub containers: Vec<Rect>,
    pub content_height: f32,
    pub scroll: f32,
    pub measurements: usize,
    pub construction: std::time::Duration,
    pub solving: std::time::Duration,
}
pub(super) fn failure(error: taffy::TaffyError) -> pixui_base::PixuiError {
    pixui_error!("layout: {error}")
}
/// Returns layout and measurement errors before any frame is published. Logical
/// rounding is disabled so drawing and hit testing share exactly the same edges.
pub(crate) fn compute(
    cache: &mut LayoutCache,
    template: &LivePart,
    state: &PartState,
    viewport: Size,
    requested_scroll: f32,
    measure: impl FnMut(usize, MeasureConstraints) -> PixuiResult<Size>,
) -> PixuiResult<LayoutResult> {
    let result = compute_inner(cache, template, state, viewport, requested_scroll, measure);
    if result.is_err() {
        // Partial solver updates must not poison the next render. Published
        // geometry is owned separately and stays usable after a failure.
        *cache = LayoutCache::default();
    }
    result
}

fn compute_inner(
    cache: &mut LayoutCache,
    template: &LivePart,
    state: &PartState,
    viewport: Size,
    requested_scroll: f32,
    mut measure: impl FnMut(usize, MeasureConstraints) -> PixuiResult<Size>,
) -> PixuiResult<LayoutResult> {
    let started = std::time::Instant::now();
    let mut nodes = Vec::new();
    let mut index = 0;
    let mut children = Vec::new();
    build(&mut nodes, template, state, &mut index, &mut children)?;
    // A definite viewport width and indefinite content height establish a
    // vertical scroll surface. Percent heights in this subtree resolve as auto
    // unless an explicit ancestor provides a definite height. Min-height alone
    // does not turn the natural scrolling height into a definite axis.
    let mut root_style = super::container::ContainerPart::column()
        .with_gap(8.0)
        .with_padding(16.0)
        .to_taffy()?;
    root_style.size.width = taffy::style::Dimension::length(viewport.width);
    root_style.min_size.height = taffy::style::LengthPercentageAuto::length(viewport.height);
    nodes.push(Node {
        style: root_style,
        children,
        component: None,
    });
    cache.sync(nodes, index)?;
    let root = *cache.ids.last().expect("implicit layout root");
    let construction = started.elapsed();
    let started = std::time::Instant::now();
    let mut error = None;
    let mut measurements = 0;
    cache.validate(&mut measure, &mut measurements)?;
    let probes = &mut cache.probes;
    let tree = &mut cache.tree;
    tree.compute_layout_with_measure(
        root,
        taffy::geometry::Size {
            width: taffy::style::AvailableSpace::Definite(viewport.width),
            height: taffy::style::AvailableSpace::MaxContent,
        },
        |inputs, _, context, style| {
            taffy::compute_leaf_layout(
                inputs,
                style,
                |_, _| 0.0,
                |known, available| {
                    let Some(index) = context.as_deref() else {
                        return taffy::geometry::Size::ZERO;
                    };
                    measurements += 1;
                    if error.is_some() {
                        return taffy::geometry::Size::ZERO;
                    }
                    let constraints = MeasureConstraints {
                        // Intrinsic Grid probes can subtract padding from a
                        // zero-sized track. Painters see empty content boxes,
                        // consistent with the geometry passed to painting.
                        width: known.width.map(|value| value.max(0.0)),
                        height: known.height.map(|value| value.max(0.0)),
                        available_width: space(available.width),
                        available_height: space(available.height),
                    };
                    match measure(*index, constraints) {
                        Ok(size) => {
                            if !probes[*index]
                                .iter()
                                .any(|probe| probe.constraints == constraints)
                            {
                                probes[*index].push(Probe { constraints, size });
                            }
                            taffy::geometry::Size {
                                width: size.width,
                                height: size.height,
                            }
                        }
                        Err(failure) => {
                            error = Some(failure);
                            taffy::geometry::Size::ZERO
                        }
                    }
                },
            )
        },
    )
    .map_err(failure)?;
    if let Some(error) = error {
        return Err(error);
    }
    let content_height = tree.layout(root).map_err(failure)?.size.height;
    let scroll = requested_scroll.clamp(0.0, (content_height - viewport.height).max(0.0));
    let mut result = LayoutResult {
        leaves: Vec::with_capacity(index),
        containers: Vec::new(),
        content_height,
        scroll,
        measurements,
        construction,
        solving: std::time::Duration::ZERO,
    };
    let clip = Rect {
        x: 0.0,
        y: 0.0,
        width: viewport.width,
        height: viewport.height,
    };
    collect(
        tree,
        root,
        0.0,
        -scroll,
        clip,
        Rect {
            x: 0.0,
            y: -scroll,
            width: viewport.width,
            height: content_height,
        },
        &mut result,
    )?;
    result.solving = started.elapsed();
    Ok(result)
}
fn space(space: taffy::style::AvailableSpace) -> AvailableSpace {
    match space {
        taffy::style::AvailableSpace::Definite(v) => AvailableSpace::Definite(v.max(0.0)),
        taffy::style::AvailableSpace::MinContent => AvailableSpace::MinContent,
        taffy::style::AvailableSpace::MaxContent => AvailableSpace::MaxContent,
    }
}
fn build(
    nodes: &mut Vec<Node>,
    part: &LivePart,
    state: &PartState,
    index: &mut usize,
    output: &mut Vec<usize>,
) -> PixuiResult<()> {
    match (part, state) {
        (LivePart::Component(part), PartState::Component(_))
            if part.component_address().is_some() =>
        {
            output.push(nodes.len());
            nodes.push(Node {
                style: part.layout.to_taffy()?,
                children: Vec::new(),
                component: Some(*index),
            });
            *index += 1;
        }
        (LivePart::Component(_), PartState::Component(_)) => {}
        (LivePart::Composite(part), PartState::Composite(state)) => {
            for (part, state) in part.parts.iter().zip(&state.parts) {
                build(nodes, part, state, index, output)?;
            }
        }
        (LivePart::Container(part), PartState::Container(state)) => {
            let mut children = Vec::new();
            for (part, state) in part.children.iter().zip(&state.parts) {
                build(nodes, part, state, index, &mut children)?;
            }
            output.push(nodes.len());
            nodes.push(Node {
                style: part.to_taffy()?,
                children,
                component: None,
            });
        }
        (LivePart::ForLoop(part), PartState::ForLoop(state)) => {
            for state in &state.items {
                build(nodes, &part.body, state, index, output)?;
            }
        }
        (LivePart::Match(part), PartState::Match(state)) => {
            if let Some(selected) = state.selected {
                build(
                    nodes,
                    &part.candidates[selected].part,
                    &state.part,
                    index,
                    output,
                )?;
            }
        }
        _ => return Err(pixui_error!("prepared layout tree and state do not match")),
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn collect(
    tree: &TaffyTree<usize>,
    node: NodeId,
    x: f32,
    y: f32,
    ancestor_clip: Rect,
    navigation_clip: Rect,
    output: &mut LayoutResult,
) -> PixuiResult<()> {
    let layout = tree.layout(node).map_err(failure)?;
    let border = Rect {
        x: x + layout.location.x,
        y: y + layout.location.y,
        width: layout.size.width,
        height: layout.size.height,
    };
    if [border.x, border.y, border.width, border.height]
        .iter()
        .any(|v| !v.is_finite())
    {
        return Err(pixui_error!("layout produced nonfinite geometry"));
    }
    let clip = ancestor_clip.intersect(border);
    let navigation_clip = navigation_clip.intersect(border);
    if tree.get_node_context(node).is_some() {
        let content = Rect {
            x: border.x + layout.padding.left,
            y: border.y + layout.padding.top,
            width: (border.width - layout.padding.left - layout.padding.right).max(0.0),
            height: (border.height - layout.padding.top - layout.padding.bottom).max(0.0),
        };
        output.leaves.push(Geometry {
            border,
            content,
            clip,
            navigation_clip,
        });
    } else {
        output.containers.push(border);
        for child in tree.children(node).map_err(failure)? {
            collect(
                tree,
                child,
                border.x,
                border.y,
                clip,
                navigation_clip,
                output,
            )?;
        }
    }
    Ok(())
}
