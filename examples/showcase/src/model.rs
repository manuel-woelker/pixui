//! Shared gallery data and actions, independent of native windows.
use pixui_base::{Arena, PixuiResult, pixui_error};
use pixui_engine::application::{
    action::slice_actions, application_handle::ApplicationHandle,
    application_slice::ApplicationSlice, collection::Collection, entity_mut::EntityMut,
};

#[pixui_reflect::reflect]
pub mod data {
    /// A row created at runtime to demonstrate collection-backed rendering.
    pub struct Sample {
        pub label: String,
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

    /// Select a gallery page in every presentation, preserving demo data.
    #[action]
    pub fn select(mut selected: EntityMut<u64>, page: u64) -> PixuiResult<()> {
        if page >= 5 {
            return Err(pixui_error!("unknown showcase page"));
        }
        *selected = page;
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
    slice.bind("text", String::new())?;
    slice.bind("uppercase", String::new())?;
    slice.bind("limited", String::new())?;
    slice.bind("count", 0_u64)?;
    slice.bind("selected", 0_u64)?;
    slice.bind("checked", false)?;
    slice.bind("details", true)?;
    let id = application.add_slice(slice)?;
    actions::ShowcaseActions::register(application, id)?;
    let actions = actions::ShowcaseActions::bind(application)?;
    actions.add_sample()?;
    Ok(())
}
