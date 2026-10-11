//! Nested Grid and Flex composition, using ordinary gallery controls and actions.

use crate::{
    gallery::{button, increment, label, reset},
    model::data::{LayoutProject, LayoutTask},
};
use pixui_base::PixuiResult;
use pixui_engine::{
    application::object_ref::ObjectRef,
    expression::expression::Expression,
    layout::{
        container::{ContainerLayout, ContainerPart},
        grid::{GridPlacement, Track},
        style::{Alignment, Distribution, LayoutStyle, Length},
    },
    live_model::part::{ForLoopPart, LivePart},
    painters::standard::StandardComponents,
};
use pixui_reflect::Reflect;

fn heading(components: StandardComponents, text: &str) -> PixuiResult<LivePart> {
    Ok(label(components, Expression::text(text)?))
}

fn span(columns: u16) -> LayoutStyle {
    LayoutStyle {
        min_width: Length::Pixels(0.0),
        grid_column: GridPlacement {
            start: None,
            span: columns,
        },
        ..Default::default()
    }
}

pub(crate) fn page(
    components: StandardComponents,
    count: ObjectRef<u64>,
    projects: ObjectRef<Vec<LayoutProject>>,
) -> PixuiResult<LivePart> {
    let mut toolbar = ContainerPart::row().with_gap(8.0).with_children(vec![
        heading(components, "Workspace")?,
        button(components, "Increment counter", increment)?,
        button(components, "Reset counter", reset)?,
    ]);
    toolbar.align = Alignment::Center;
    if let ContainerLayout::Flex(flex) = &mut toolbar.layout {
        flex.wrap = true;
        flex.justify = Distribution::SpaceBetween;
    }

    let mut cards = Vec::new();
    for (title, value) in [
        ("Projects", "12"),
        ("Tasks", "48"),
        ("Teams", "4"),
        ("Releases", "7"),
        ("Reviews", "16"),
        ("Milestones", "3"),
    ] {
        cards.push(
            ContainerPart::column()
                .with_padding(8.0)
                .with_gap(4.0)
                .with_children(vec![
                    heading(components, title)?,
                    heading(components, value)?,
                ])
                .into(),
        );
    }
    let metrics = ContainerPart::grid()
        .with_columns(vec![Track::fraction(1.0); 3])
        .with_rows(vec![Track::length(72.0); 2])
        .with_gap(8.0)
        .with_children(cards);

    let main = ContainerPart::column()
        .with_layout(span(2))
        .with_gap(12.0)
        .with_children(vec![
            heading(components, "Overview")?,
            metrics.into(),
            label(
                components,
                Expression::i18n("Counter: {count}", [("count", Expression::entity(count))])?,
            ),
        ]);
    let mut activity = vec![heading(components, "Activity")?];
    for text in [
        "Design ready",
        "Tests passed",
        "Review queued",
        "Build shipped",
    ] {
        activity.push(heading(components, text)?);
    }
    let activity = ContainerPart::column()
        .with_gap(8.0)
        .with_padding(8.0)
        .with_children(activity);

    let mut actions = ContainerPart::row().with_layout(span(3)).with_gap(8.0);
    if let ContainerLayout::Flex(flex) = &mut actions.layout {
        flex.wrap = true;
    }
    for title in [
        "Plan sprint",
        "Create task",
        "Review work",
        "Ship release",
        "Invite team",
        "Archive project",
    ] {
        let control = ContainerPart::column()
            .with_layout(LayoutStyle {
                basis: Length::Pixels(140.0),
                grow: 1.0,
                min_width: Length::Pixels(140.0),
                ..Default::default()
            })
            .with_children(vec![button(components, title, increment)?]);
        actions.children.push(control.into());
    }

    let dashboard = ContainerPart::grid()
        .with_columns(vec![
            Track::fraction(1.0),
            Track::fraction(1.0),
            Track::fraction(1.0),
        ])
        .with_gap(12.0)
        .with_children(vec![
            toolbar.with_layout(span(3)).into(),
            main.into(),
            activity.into(),
            actions.into(),
        ]);
    Ok(ContainerPart::column()
        .with_layout(LayoutStyle {
            width: Length::Percent(1.0),
            min_width: Length::Pixels(0.0),
            ..Default::default()
        })
        .with_gap(12.0)
        .with_children(vec![
            heading(components, "Layout")?,
            heading(components, "Nested Grid and Flex")?,
            dashboard.into(),
            heading(components, "Project board · 36 projects / 216 tasks")?,
            project_board(components, projects)?,
        ])
        .into())
}

/// One card definition expands into 36 physical cards; a nested loop adds six
/// two-column task rows to each. This intentionally exercises full-tree layout
/// and painting, including offscreen content, rather than virtualizing it.
fn project_board(
    components: StandardComponents,
    projects: ObjectRef<Vec<LayoutProject>>,
) -> PixuiResult<LivePart> {
    let project = LayoutProject::type_descriptor();
    let task = LayoutTask::type_descriptor();
    let tasks = LivePart::ForLoop(ForLoopPart {
        key: None,
        expression: Expression::field(project.field_index("tasks")?),
        body: Box::new(
            ContainerPart::row()
                .with_gap(8.0)
                .with_children(vec![
                    ContainerPart::column()
                        .with_layout(LayoutStyle::grow(1.0))
                        .with_children(vec![label(
                            components,
                            Expression::field(task.field_index("name")?),
                        )])
                        .into(),
                    label(components, Expression::field(task.field_index("status")?)),
                ])
                .into(),
        ),
    });
    let mut controls = ContainerPart::row().with_gap(8.0).with_children(vec![
        button(components, "Create task", increment)?,
        button(components, "Review work", increment)?,
    ]);
    if let ContainerLayout::Flex(flex) = &mut controls.layout {
        flex.wrap = true;
    }
    let card = ContainerPart::column()
        .with_layout(LayoutStyle {
            min_width: Length::Pixels(0.0),
            ..Default::default()
        })
        .with_padding(12.0)
        .with_gap(8.0)
        .with_children(vec![
            label(components, Expression::field(project.field_index("name")?)),
            heading(components, "Tasks")?,
            tasks,
            controls.into(),
        ]);
    Ok(ContainerPart::grid()
        .with_columns(vec![Track::fraction(1.0); 2])
        .with_gap(12.0)
        .with_children(vec![LivePart::ForLoop(ForLoopPart {
            key: None,
            expression: Expression::entity(projects),
            body: Box::new(card.into()),
        })])
        .into())
}
