#![doc = include_str!("README.md")]
use super::{
    grid::Track,
    style::{Alignment, Distribution, LayoutStyle, nonnegative},
};
use crate::live_model::part::LivePart;
use pixui_base::PixuiResult;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Row,
    Column,
}
#[derive(Clone, Debug)]
pub struct FlexLayout {
    pub direction: Direction,
    pub wrap: bool,
    pub justify: Distribution,
}
#[derive(Clone, Debug, Default)]
pub struct GridLayout {
    pub columns: Vec<Track>,
    pub rows: Vec<Track>,
}
#[derive(Clone, Debug)]
pub enum ContainerLayout {
    Flex(FlexLayout),
    Grid(GridLayout),
}
/// Containers clip descendants to their border box. They have no painter or
/// interaction identity; their layout as a parent is independent of their own
/// item style in the enclosing Flex or Grid container.
#[derive(Clone)]
pub struct ContainerPart {
    pub layout: ContainerLayout,
    pub style: LayoutStyle,
    pub gap: f32,
    pub row_gap: Option<f32>,
    pub align: Alignment,
    pub children: Vec<LivePart>,
}
impl ContainerPart {
    pub fn row() -> Self {
        Self::flex(Direction::Row)
    }
    pub fn column() -> Self {
        Self::flex(Direction::Column)
    }
    fn flex(direction: Direction) -> Self {
        Self {
            layout: ContainerLayout::Flex(FlexLayout {
                direction,
                wrap: false,
                justify: Distribution::Start,
            }),
            style: LayoutStyle::default(),
            gap: 0.0,
            row_gap: None,
            align: Alignment::Stretch,
            children: Vec::new(),
        }
    }
    pub fn grid() -> Self {
        Self {
            layout: ContainerLayout::Grid(GridLayout::default()),
            ..Self::column()
        }
    }
    /// Selects Grid layout, retaining existing Grid rows when present.
    pub fn with_columns(mut self, columns: Vec<Track>) -> Self {
        let mut grid = match self.layout {
            ContainerLayout::Grid(grid) => grid,
            ContainerLayout::Flex(_) => GridLayout::default(),
        };
        grid.columns = columns;
        self.layout = ContainerLayout::Grid(grid);
        self
    }
    /// Selects Grid layout, retaining existing Grid columns when present.
    pub fn with_rows(mut self, rows: Vec<Track>) -> Self {
        let mut grid = match self.layout {
            ContainerLayout::Grid(grid) => grid,
            ContainerLayout::Flex(_) => GridLayout::default(),
        };
        grid.rows = rows;
        self.layout = ContainerLayout::Grid(grid);
        self
    }
    pub fn with_layout(mut self, style: LayoutStyle) -> Self {
        self.style = style;
        self
    }
    pub fn with_gap(mut self, value: f32) -> Self {
        self.gap = value;
        self
    }
    pub fn with_padding(mut self, value: f32) -> Self {
        self.style = self.style.with_padding(value);
        self
    }
    pub fn with_children(mut self, children: Vec<LivePart>) -> Self {
        self.children = children;
        self
    }
    pub(crate) fn to_taffy(&self) -> PixuiResult<taffy::Style> {
        let mut style = self.style.to_taffy()?;
        style.gap = taffy::geometry::Size {
            width: taffy::style::LengthPercentage::length(nonnegative(self.gap)?),
            height: taffy::style::LengthPercentage::length(nonnegative(
                self.row_gap.unwrap_or(self.gap),
            )?),
        };
        style.align_items = Some(self.align.to_taffy());
        match &self.layout {
            ContainerLayout::Flex(flex) => {
                style.display = taffy::style::Display::Flex;
                style.flex_direction = match flex.direction {
                    Direction::Row => taffy::style::FlexDirection::Row,
                    Direction::Column => taffy::style::FlexDirection::Column,
                };
                style.flex_wrap = if flex.wrap {
                    taffy::style::FlexWrap::Wrap
                } else {
                    taffy::style::FlexWrap::NoWrap
                };
                style.justify_content = Some(flex.justify.to_taffy());
            }
            ContainerLayout::Grid(grid) => {
                style.display = taffy::style::Display::Grid;
                style.justify_items = Some(self.align.to_taffy());
                style.grid_template_columns = grid
                    .columns
                    .iter()
                    .map(|track| {
                        Ok(taffy::style::GridTemplateComponent::Single(
                            track.to_taffy()?,
                        ))
                    })
                    .collect::<PixuiResult<Vec<_>>>()?;
                style.grid_template_rows = grid
                    .rows
                    .iter()
                    .map(|track| {
                        Ok(taffy::style::GridTemplateComponent::Single(
                            track.to_taffy()?,
                        ))
                    })
                    .collect::<PixuiResult<Vec<_>>>()?;
            }
        }
        Ok(style)
    }
}
impl From<ContainerPart> for LivePart {
    fn from(part: ContainerPart) -> Self {
        Self::Container(part)
    }
}
