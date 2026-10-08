//! Constant definition styles. All explicit sizes describe border boxes in
//! logical pixels; percentage values use 0.0–1.0 units (values above 1 are valid).
//! Validation occurs at registration and rendering. Taffy types stay private.
use super::grid::GridPlacement;
use pixui_base::{PixuiResult, pixui_error};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Length {
    #[default]
    Auto,
    Pixels(f32),
    Percent(f32),
}
impl Length {
    fn constraint(self) -> PixuiResult<taffy::style::LengthPercentageAuto> {
        use taffy::style::LengthPercentageAuto as L;
        Ok(match self {
            Self::Auto => L::auto(),
            Self::Pixels(v) => L::length(nonnegative(v)?),
            Self::Percent(v) => L::percent(nonnegative(v)?),
        })
    }
    fn dimension(self) -> PixuiResult<taffy::style::Dimension> {
        Ok(match self {
            Self::Auto => taffy::style::Dimension::auto(),
            Self::Pixels(v) => taffy::style::Dimension::length(nonnegative(v)?),
            Self::Percent(v) => taffy::style::Dimension::percent(nonnegative(v)?),
        })
    }
}
/// Logical insets. Margins, padding, and gaps are nonnegative in the initial API.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}
impl Insets {
    pub fn all(value: f32) -> Self {
        Self {
            left: value,
            right: value,
            top: value,
            bottom: value,
        }
    }
    fn rect(self) -> PixuiResult<taffy::geometry::Rect<f32>> {
        Ok(taffy::geometry::Rect {
            left: nonnegative(self.left)?,
            right: nonnegative(self.right)?,
            top: nonnegative(self.top)?,
            bottom: nonnegative(self.bottom)?,
        })
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Alignment {
    Start,
    Center,
    End,
    #[default]
    Stretch,
}
impl Alignment {
    pub(crate) fn to_taffy(self) -> taffy::style::AlignItems {
        use taffy::style::AlignItems;
        match self {
            Self::Start => AlignItems::START,
            Self::Center => AlignItems::CENTER,
            Self::End => AlignItems::END,
            Self::Stretch => AlignItems::STRETCH,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Distribution {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}
impl Distribution {
    pub(crate) fn to_taffy(self) -> taffy::style::JustifyContent {
        use taffy::style::JustifyContent as J;
        match self {
            Self::Start => J::START,
            Self::Center => J::CENTER,
            Self::End => J::END,
            Self::SpaceBetween => J::SPACE_BETWEEN,
            Self::SpaceAround => J::SPACE_AROUND,
            Self::SpaceEvenly => J::SPACE_EVENLY,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutStyle {
    pub width: Length,
    pub height: Length,
    pub min_width: Length,
    pub min_height: Length,
    pub max_width: Length,
    pub max_height: Length,
    pub aspect_ratio: Option<f32>,
    pub margin: Insets,
    pub padding: Insets,
    pub grow: f32,
    pub shrink: f32,
    pub basis: Length,
    pub align_self: Option<Alignment>,
    pub justify_self: Option<Alignment>,
    pub grid_column: GridPlacement,
    pub grid_row: GridPlacement,
}
impl Default for LayoutStyle {
    fn default() -> Self {
        Self {
            width: Length::Auto,
            height: Length::Auto,
            min_width: Length::Auto,
            min_height: Length::Auto,
            max_width: Length::Auto,
            max_height: Length::Auto,
            aspect_ratio: None,
            margin: Insets::default(),
            padding: Insets::default(),
            grow: 0.0,
            shrink: 1.0,
            basis: Length::Auto,
            align_self: None,
            justify_self: None,
            grid_column: GridPlacement::default(),
            grid_row: GridPlacement::default(),
        }
    }
}
impl LayoutStyle {
    pub fn auto() -> Self {
        Self::default()
    }
    pub fn fixed(width: f32, height: f32) -> Self {
        Self {
            width: Length::Pixels(width),
            height: Length::Pixels(height),
            shrink: 0.0,
            ..Self::default()
        }
    }
    pub fn grow(value: f32) -> Self {
        Self {
            grow: value,
            min_width: Length::Pixels(0.0),
            ..Self::default()
        }
    }
    pub fn with_padding(mut self, value: f32) -> Self {
        self.padding = Insets::all(value);
        self
    }
    pub(crate) fn to_taffy(&self) -> PixuiResult<taffy::Style> {
        if self
            .aspect_ratio
            .is_some_and(|v| !v.is_finite() || v <= 0.0)
        {
            return Err(pixui_error!("aspect ratio must be finite and positive"));
        }
        Ok(taffy::Style {
            box_sizing: taffy::style::BoxSizing::BorderBox,
            size: taffy::geometry::Size {
                width: self.width.dimension()?,
                height: self.height.dimension()?,
            },
            min_size: taffy::geometry::Size {
                width: self.min_width.constraint()?,
                height: self.min_height.constraint()?,
            },
            max_size: taffy::geometry::Size {
                width: self.max_width.constraint()?,
                height: self.max_height.constraint()?,
            },
            aspect_ratio: self.aspect_ratio,
            margin: self
                .margin
                .rect()?
                .map(taffy::style::LengthPercentageAuto::length),
            padding: self
                .padding
                .rect()?
                .map(taffy::style::LengthPercentage::length),
            flex_grow: nonnegative(self.grow)?,
            flex_shrink: nonnegative(self.shrink)?,
            flex_basis: self.basis.dimension()?,
            align_self: self.align_self.map(Alignment::to_taffy),
            justify_self: self.justify_self.map(Alignment::to_taffy),
            grid_column: self.grid_column.to_taffy()?,
            grid_row: self.grid_row.to_taffy()?,
            ..Default::default()
        })
    }
}
pub(crate) fn nonnegative(value: f32) -> PixuiResult<f32> {
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(pixui_error!("layout values must be finite and nonnegative"))
    }
}
