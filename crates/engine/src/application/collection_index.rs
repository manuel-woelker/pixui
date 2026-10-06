//! Stable addresses into the application's append-only collection store.

/// An opaque, process-local collection address. The position enables direct
/// indexing; identity validation rejects indices minted by another application.
/// Collections are never removed or replaced, so no generation is needed.
/// Removing a slice removes its bindings, without invalidating this address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CollectionIndex {
    pub(super) position: usize,
    pub(super) identity: u16,
}

#[cfg(test)]
mod tests {
    use crate::application::{app::Application, collection::Collection};

    #[test]
    fn invalid_positions_fail_without_indexing_panics() {
        let mut app = Application::default();
        let mut index = app
            .register_collection(Collection::new::<i32>("numbers"))
            .unwrap();
        index.position = usize::MAX;
        assert!(app.resolve_collection(index).is_err());
        assert!(app.resolve_collection_mut::<i32>(index).is_err());
    }
}
