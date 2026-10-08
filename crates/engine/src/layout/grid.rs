//! Small Grid vocabulary. Tracks are logical lengths, automatic sizes, or shares
//! of remaining space. Fraction tracks default to a zero minimum to allow clipping.
use pixui_base::{PixuiResult, pixui_error};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrackMinimum {
    Length(f32),
    Auto,
    MinContent,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrackMaximum {
    Length(f32),
    Auto,
    Fraction(f32),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Track {
    pub min: TrackMinimum,
    pub max: TrackMaximum,
}
impl Track {
    pub fn length(value: f32) -> Self {
        Self {
            min: TrackMinimum::Length(value),
            max: TrackMaximum::Length(value),
        }
    }
    pub fn auto() -> Self {
        Self {
            min: TrackMinimum::Auto,
            max: TrackMaximum::Auto,
        }
    }
    pub fn fraction(value: f32) -> Self {
        Self {
            min: TrackMinimum::Length(0.0),
            max: TrackMaximum::Fraction(value),
        }
    }
    pub(crate) fn to_taffy(self) -> PixuiResult<taffy::style::TrackSizingFunction> {
        use taffy::style::{MaxTrackSizingFunction as Max, MinTrackSizingFunction as Min};
        let validate = |value: f32| {
            if value.is_finite() && value >= 0.0 {
                Ok(value)
            } else {
                Err(pixui_error!("invalid grid track size"))
            }
        };
        Ok(taffy::geometry::MinMax {
            min: match self.min {
                TrackMinimum::Length(v) => Min::length(validate(v)?),
                TrackMinimum::Auto => Min::auto(),
                TrackMinimum::MinContent => Min::min_content(),
            },
            max: match self.max {
                TrackMaximum::Length(v) => Max::length(validate(v)?),
                TrackMaximum::Auto => Max::auto(),
                TrackMaximum::Fraction(v) => Max::fr(validate(v)?),
            },
        })
    }
}
/// Positive, one-based track start and positive span. None selects automatic
/// placement. Explicitly overlapping placements retain definition paint order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridPlacement {
    pub start: Option<u16>,
    pub span: u16,
}
impl Default for GridPlacement {
    fn default() -> Self {
        Self {
            start: None,
            span: 1,
        }
    }
}
impl GridPlacement {
    pub(crate) fn to_taffy(
        self,
    ) -> PixuiResult<taffy::geometry::Line<taffy::style::GridPlacement>> {
        if self.span == 0 || self.start.is_some_and(|v| v == 0 || v > i16::MAX as u16) {
            return Err(pixui_error!(
                "grid starts must be positive i16 values and spans nonzero"
            ));
        }
        use taffy::style::GridPlacement as Placement;
        Ok(taffy::geometry::Line {
            start: self
                .start
                .map_or(Placement::Auto, |v| taffy::style_helpers::line(v as i16)),
            end: taffy::style_helpers::span(self.span),
        })
    }
}
