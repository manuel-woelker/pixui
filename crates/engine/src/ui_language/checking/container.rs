//! Constant container configuration. Exported item styles are applied before
//! explicit attributes so source attribute ordering never changes the layout.
use super::{
    checker::{CheckResult, Checker},
    typed::ExpressionKind,
};
use crate::{
    layout::{
        container::{ContainerLayout, ContainerPart},
        grid::Track,
        style::{Alignment, Distribution, Insets, Length},
    },
    ui_language::{ast::expression::Name, resolution::resolved},
};
use std::any::TypeId;

impl Checker<'_> {
    pub(super) fn container_attribute(
        &mut self,
        container: &mut ContainerPart,
        name: &Name,
        expression: &resolved::Expression,
    ) -> CheckResult<()> {
        let style = &mut container.style;
        match name.value.as_str() {
            "layout" => {}
            "width" => style.width = self.layout_length(expression)?,
            "height" => style.height = self.layout_length(expression)?,
            "min_width" => style.min_width = self.layout_length(expression)?,
            "min_height" => style.min_height = self.layout_length(expression)?,
            "max_width" => style.max_width = self.layout_length(expression)?,
            "max_height" => style.max_height = self.layout_length(expression)?,
            "basis" => style.basis = self.layout_length(expression)?,
            "grow" => style.grow = self.layout_number(expression)?,
            "shrink" => style.shrink = self.layout_number(expression)?,
            "aspect_ratio" => style.aspect_ratio = Some(self.layout_number(expression)?),
            "padding" => style.padding = Insets::all(self.layout_number(expression)?),
            "margin" => style.margin = Insets::all(self.layout_number(expression)?),
            "padding_left" => style.padding.left = self.layout_number(expression)?,
            "padding_right" => style.padding.right = self.layout_number(expression)?,
            "padding_top" => style.padding.top = self.layout_number(expression)?,
            "padding_bottom" => style.padding.bottom = self.layout_number(expression)?,
            "margin_left" => style.margin.left = self.layout_number(expression)?,
            "margin_right" => style.margin.right = self.layout_number(expression)?,
            "margin_top" => style.margin.top = self.layout_number(expression)?,
            "margin_bottom" => style.margin.bottom = self.layout_number(expression)?,
            "align_self" => style.align_self = Some(self.layout_alignment(expression)?),
            "justify_self" => style.justify_self = Some(self.layout_alignment(expression)?),
            "grid_column" => style.grid_column.start = Some(self.grid_index(expression)?),
            "grid_row" => style.grid_row.start = Some(self.grid_index(expression)?),
            "column_span" => style.grid_column.span = self.grid_index(expression)?,
            "row_span" => style.grid_row.span = self.grid_index(expression)?,
            "gap" => container.gap = self.layout_number(expression)?,
            "row_gap" => container.row_gap = Some(self.layout_number(expression)?),
            "align" => container.align = self.layout_alignment(expression)?,
            "wrap" => {
                let ContainerLayout::Flex(flex) = &mut container.layout else {
                    return Err(self.error(name.span, "wrap requires Row or Column"));
                };
                let typed = self.expression(expression, Some(TypeId::of::<bool>()))?;
                let ExpressionKind::Constant { value, .. } = typed.kind else {
                    return Err(self.error(expression.span, "wrap must be a boolean constant"));
                };
                flex.wrap = *value.downcast_ref::<bool>().unwrap();
            }
            "justify" => {
                let ContainerLayout::Flex(flex) = &mut container.layout else {
                    return Err(self.error(name.span, "justify requires Row or Column"));
                };
                flex.justify = match self.literal_text(expression)?.as_str() {
                    "start" => Distribution::Start,
                    "center" => Distribution::Center,
                    "end" => Distribution::End,
                    "space_between" => Distribution::SpaceBetween,
                    "space_around" => Distribution::SpaceAround,
                    "space_evenly" => Distribution::SpaceEvenly,
                    _ => return Err(self.error(
                        expression.span,
                        "expected start, center, end, space_between, space_around, or space_evenly",
                    )),
                };
            }
            "columns" | "rows" => {
                let ContainerLayout::Grid(grid) = &mut container.layout else {
                    return Err(self.error(name.span, "tracks require Grid"));
                };
                let tracks =
                    self.literal_text(expression)?
                        .split_whitespace()
                        .map(|track| {
                            if track == "auto" {
                                Ok(Track::auto())
                            } else if let Some(value) = track.strip_suffix("fr") {
                                value.parse().map(Track::fraction).map_err(|_| {
                                    self.error(expression.span, "invalid fraction track")
                                })
                            } else if let Some(value) = track.strip_suffix("px") {
                                value
                                    .parse()
                                    .map(Track::length)
                                    .map_err(|_| self.error(expression.span, "invalid pixel track"))
                            } else {
                                Err(self.error(expression.span, "tracks must be auto, Npx, or Nfr"))
                            }
                        })
                        .collect::<CheckResult<Vec<_>>>()?;
                if name.value == "columns" {
                    grid.columns = tracks;
                } else {
                    grid.rows = tracks;
                }
            }
            _ => return Err(self.error(name.span, "unknown container attribute")),
        }
        Ok(())
    }
    fn layout_number(&mut self, expression: &resolved::Expression) -> CheckResult<f32> {
        let typed = self.expression(expression, Some(TypeId::of::<f32>()))?;
        let ExpressionKind::Constant { value, .. } = typed.kind else {
            return Err(self.error(expression.span, "layout values must be numeric constants"));
        };
        let value = *value.downcast_ref::<f32>().unwrap();
        crate::layout::style::nonnegative(value)
            .map_err(|error| self.error(expression.span, error.to_string()))
    }
    fn layout_length(&mut self, expression: &resolved::Expression) -> CheckResult<Length> {
        if matches!(expression.kind, resolved::ExpressionKind::Template { .. }) {
            let text = self.literal_text(expression)?;
            if text == "auto" {
                return Ok(Length::Auto);
            }
            let (value, percent) = if let Some(value) = text.strip_suffix('%') {
                (value, true)
            } else if let Some(value) = text.strip_suffix("px") {
                (value, false)
            } else {
                return Err(self.error(expression.span, "expected auto, Npx, or N%"));
            };
            let value = value
                .parse::<f32>()
                .map_err(|_| self.error(expression.span, "invalid layout length"))?;
            let value = crate::layout::style::nonnegative(value)
                .map_err(|error| self.error(expression.span, error.to_string()))?;
            Ok(if percent {
                Length::Percent(value / 100.0)
            } else {
                Length::Pixels(value)
            })
        } else {
            self.layout_number(expression).map(Length::Pixels)
        }
    }
    fn layout_alignment(&self, expression: &resolved::Expression) -> CheckResult<Alignment> {
        match self.literal_text(expression)?.as_str() {
            "start" => Ok(Alignment::Start),
            "center" => Ok(Alignment::Center),
            "end" => Ok(Alignment::End),
            "stretch" => Ok(Alignment::Stretch),
            _ => Err(self.error(expression.span, "expected start, center, end, or stretch")),
        }
    }
    fn grid_index(&mut self, expression: &resolved::Expression) -> CheckResult<u16> {
        let typed = self.expression(expression, Some(TypeId::of::<u16>()))?;
        let ExpressionKind::Constant { value, .. } = typed.kind else {
            return Err(self.error(
                expression.span,
                "grid placement must be an integer constant",
            ));
        };
        let value = *value.downcast_ref::<u16>().unwrap();
        if value == 0 {
            return Err(self.error(expression.span, "grid placement must be positive"));
        }
        Ok(value)
    }
}
