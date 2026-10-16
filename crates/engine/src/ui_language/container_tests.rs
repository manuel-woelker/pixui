//! Static container settings compile to the same layout objects as Rust definitions.
use super::{compiler::compile, resolution::environment::Environment};
use crate::{
    layout::{
        container::{ContainerLayout, ContainerPart},
        style::{Alignment, Distribution, Insets, LayoutStyle, Length},
    },
    live_model::part::LivePart,
};
fn container(attributes: &str, env: &Environment) -> ContainerPart {
    let source = format!("ui X {{<Row {attributes} />}}");
    let definition = compile("layout.pixui", &source, env).unwrap().definition;
    let LivePart::Container(container) = definition.template else {
        panic!()
    };
    container
}
#[test]
fn flex_settings_and_box_constraints_are_preserved() {
    let part = container(
        "wrap=true justify='space_between' align='center' gap=8 row_gap=12 width='100%' height='auto' min_width=0 max_height='240px' basis='25%' grow=1 shrink=0 aspect_ratio=2 padding=4 margin=3 align_self='end' justify_self='start' grid_column=2 column_span=3 grid_row=1 row_span=2",
        &Environment::new(),
    );
    let ContainerLayout::Flex(flex) = part.layout else {
        panic!()
    };
    assert!(flex.wrap);
    assert_eq!(flex.justify, Distribution::SpaceBetween);
    assert_eq!(part.align, Alignment::Center);
    assert_eq!((part.gap, part.row_gap), (8.0, Some(12.0)));
    assert_eq!(part.style.width, Length::Percent(1.0));
    assert_eq!(part.style.height, Length::Auto);
    assert_eq!(part.style.max_height, Length::Pixels(240.0));
    assert_eq!(part.style.basis, Length::Percent(0.25));
    assert_eq!(part.style.aspect_ratio, Some(2.0));
    assert_eq!(part.style.padding, Insets::all(4.0));
    assert_eq!(part.style.margin, Insets::all(3.0));
    assert_eq!(part.style.align_self, Some(Alignment::End));
    assert_eq!(part.style.justify_self, Some(Alignment::Start));
    assert_eq!(part.style.grid_column.start, Some(2));
    assert_eq!(part.style.grid_column.span, 3);
    assert_eq!(part.style.grid_row.span, 2);
}
#[test]
fn exported_base_is_overridden_independently_of_attribute_order() {
    let mut env = Environment::new();
    env.layout("base", LayoutStyle::fixed(90.0, 70.0)).unwrap();
    let before = container(
        "width=40 layout=layouts.base padding_left=7 padding=4 margin_bottom=3 margin=2",
        &env,
    );
    let after = container(
        "layout=layouts.base width=40 padding=4 padding_left=7 margin=2 margin_bottom=3",
        &env,
    );
    assert_eq!(before.style, after.style);
    assert_eq!(before.style.width, Length::Pixels(40.0));
    assert_eq!(before.style.height, Length::Pixels(70.0));
    assert_eq!(before.style.padding.left, 7.0);
    assert_eq!(before.style.margin.bottom, 3.0);
}
#[test]
fn grid_tracks_and_alignment_coexist_with_item_placement() {
    let compiled = compile("grid.pixui", "ui X {<Grid columns='100px 1fr auto' rows='30px auto' align='end' row_gap=6 column_span=2 />}", &Environment::new()).unwrap();
    let LivePart::Container(part) = compiled.definition.template else {
        panic!()
    };
    let ContainerLayout::Grid(grid) = part.layout else {
        panic!()
    };
    assert_eq!(grid.columns.len(), 3);
    assert_eq!(grid.rows.len(), 2);
    assert_eq!(part.align, Alignment::End);
    assert_eq!(part.style.grid_column.span, 2);
}
#[test]
fn invalid_settings_report_checking_errors_at_source() {
    for (tag, attributes) in [
        ("Row", "wrap=1"),
        ("Row", "align='middle'"),
        ("Row", "justify='stretch'"),
        ("Row", "width='infinitypx'"),
        ("Row", "width='-1%'"),
        ("Row", "width='30em'"),
        ("Row", "width=\"100%\""),
        ("Row", "align=\"center\""),
        ("Row", "column_span=0"),
        ("Row", "grid_row=32768"),
        ("Row", "row_span=1.5"),
        ("Row", "aspect_ratio=0"),
        ("Row", "columns='1fr'"),
        ("Grid", "wrap=true"),
        ("Grid", "justify='center'"),
        ("Grid", "columns='NaNfr'"),
    ] {
        let source = format!("ui X {{<{tag} {attributes} />}}");
        let error = compile("bad-layout.pixui", &source, &Environment::new())
            .err()
            .unwrap_or_else(|| panic!("accepted {source}"));
        assert!(
            error
                .diagnostics
                .iter()
                .all(|d| d.stage == super::diagnostic::report::Stage::Checking)
        );
        assert!(error.to_string().contains("bad-layout.pixui"));
    }
}
