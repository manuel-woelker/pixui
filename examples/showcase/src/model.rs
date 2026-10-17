//! Shared gallery data and actions, independent of native windows.
use pixui_base::{Arena, PixuiResult, pixui_error};
use pixui_engine::application::{
    action::slice_actions, application_handle::ApplicationHandle,
    application_slice::ApplicationSlice, collection::Collection, entity_mut::EntityMut,
};

#[pixui_reflect::reflect]
pub mod data {
    /// Reusable views and titles are values, separate from mounted UI state.
    #[derive(Clone)]
    pub struct NavigationEntry {
        pub id: u64,
        pub title: pixui_engine::composition::localized_text::LocalizedText,
        pub component: pixui_engine::composition::component_ref::ComponentRef,
    }
    pub struct NavigationState {
        pub entries: Vec<NavigationEntry>,
        pub selected: u64,
    }

    /// A row created at runtime to demonstrate collection-backed rendering.
    pub struct Sample {
        pub label: String,
    }

    /// Data for the nested live loops on the layout stress page.
    pub struct LayoutProject {
        pub name: String,
        pub tasks: Vec<LayoutTask>,
    }

    pub struct LayoutTask {
        pub name: String,
        pub status: String,
    }
}

#[slice_actions(slice = "showcase", facade = ShowcaseActions)]
pub mod actions {
    use super::*;
    use data::Sample;

    /// Accept the proposed text unchanged.
    #[action]
    pub fn edit_text(mut text: EntityMut<String>, content: String) {
        *text = content;
    }

    /// Demonstrate application-owned normalization.
    #[action]
    pub fn edit_uppercase(mut uppercase: EntityMut<String>, content: String) {
        *uppercase = content.to_uppercase();
    }

    /// Reject a proposal without changing the authoritative value.
    #[action]
    pub fn edit_limited(mut limited: EntityMut<String>, content: String) -> PixuiResult<()> {
        if content.chars().count() > 12 {
            return Err(pixui_error!("at most 12 characters"));
        }
        *limited = content;
        Ok(())
    }

    /// Record a custom component click using its typed payload coordinates.
    #[action]
    pub fn record_click(mut last_click: EntityMut<String>, x: f32, y: f32) {
        *last_click = format!("{x:.0}, {y:.0}");
    }

    /// Select a registered entry; IDs are independent of list position.
    #[action]
    pub fn select(mut navigation: EntityMut<data::NavigationState>, id: u64) -> PixuiResult<()> {
        if !navigation.entries.iter().any(|entry| entry.id == id) {
            return Err(pixui_error!("unknown showcase page"));
        }
        navigation.selected = id;
        Ok(())
    }

    /// Startup/fixture binding: validate the complete list before replacing it.
    /// This host action is deliberately not exported to source event bindings.
    #[action]
    pub fn install_navigation(
        mut navigation: EntityMut<data::NavigationState>,
        entries: Vec<data::NavigationEntry>,
    ) -> PixuiResult<()> {
        let ids: std::collections::HashSet<_> = entries.iter().map(|entry| entry.id).collect();
        if entries.is_empty() || ids.len() != entries.len() {
            return Err(pixui_error!(
                "navigation entries must be nonempty and have unique IDs"
            ));
        }
        if !ids.contains(&navigation.selected) {
            navigation.selected = entries[0].id;
        }
        navigation.entries = entries;
        Ok(())
    }

    /// Increase the shared counter in both windows.
    #[action]
    pub fn increment(mut count: EntityMut<u64>) {
        *count = count.saturating_add(1);
    }

    /// Reset the counter without changing the checkbox or collection state.
    #[action]
    pub fn reset(mut count: EntityMut<u64>) {
        *count = 0;
    }

    /// Toggle a named boolean independent of the details flag.
    #[action]
    pub fn toggle_checked(mut checked: EntityMut<bool>) {
        *checked = !*checked;
    }

    /// Insert or remove the conditional explanation from the live tree.
    #[action]
    pub fn toggle_details(mut details: EntityMut<bool>) {
        *details = !*details;
    }

    /// Append a row through an injected typed collection.
    #[action]
    pub fn add_sample(samples: &mut Arena<Sample>) {
        let number = samples.len() + 1;
        samples.insert(Sample {
            label: format!("Sample {number}"),
        });
    }
}

/// Register the shared slice and validate action argument bindings.
pub fn register(application: &ApplicationHandle) -> PixuiResult<()> {
    let mut slice = ApplicationSlice::new("showcase");
    let samples =
        application.register_collection(Collection::new_reflected::<data::Sample>("samples"))?;
    slice.bind_collection("samples", samples)?;
    slice.bind("last_click", String::new())?;
    slice.bind("text", String::new())?;
    slice.bind("uppercase", String::new())?;
    slice.bind("limited", String::new())?;
    slice.bind("count", 0_u64)?;
    slice.bind(
        "navigation",
        data::NavigationState {
            entries: vec![],
            selected: 0,
        },
    )?;
    slice.bind("checked", false)?;
    slice.bind("details", true)?;
    let projects: Vec<_> = (1..=36)
        .map(|project| data::LayoutProject {
            name: format!("Project {project:02}"),
            tasks: (1..=6)
                .map(|task| data::LayoutTask {
                    name: format!("Task {task:02}"),
                    status: if task % 3 == 0 { "Ready" } else { "Active" }.into(),
                })
                .collect(),
        })
        .collect();
    slice.bind("layout_projects", projects)?;
    let id = application.add_slice(slice)?;
    actions::ShowcaseActions::register(application, id)?;
    let actions = actions::ShowcaseActions::bind(application)?;
    actions.add_sample()?;
    Ok(())
}
