//! Shared gallery data and actions, independent of native windows.
use pixui_base::{Arena, PixuiResult};
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
    slice.bind("count", 0_u64)?;
    slice.bind("checked", false)?;
    slice.bind("details", true)?;
    let id = application.add_slice(slice)?;
    actions::ShowcaseActions::register(application, id)?;
    let actions = actions::ShowcaseActions::bind(application)?;
    actions.add_sample()?;
    Ok(())
}
