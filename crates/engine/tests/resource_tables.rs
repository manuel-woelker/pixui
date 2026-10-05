//! Shared ownership/indexing works without requiring traits on the payload.

use pixui_engine::ui::{
    resource::Resource,
    resource_table::{ResourceIndex, ResourceTable, ResourceTableBuilder},
};
use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

struct Payload {
    value: usize,
    drops: Arc<AtomicUsize>,
}
impl Drop for Payload {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
fn snapshot(value: usize, drops: &Arc<AtomicUsize>) -> Resource<Payload> {
    Resource::from_value(Payload {
        value,
        drops: drops.clone(),
    })
}

#[test]
fn handles_and_tables_compare_identity_without_payload_trait_bounds() {
    let drops = Arc::new(AtomicUsize::new(0));
    let first = snapshot(7, &drops);
    let cloned = first.clone();
    let separate = snapshot(7, &drops);
    assert!(first == cloned);
    assert!(first != separate);
    assert_eq!(first.value, separate.value); // equal contents, distinct version
    let identities: HashSet<_> = [first.identity(), cloned.identity(), separate.identity()]
        .into_iter()
        .collect();
    assert_eq!(identities.len(), 2);
    let mut builder = ResourceTableBuilder::default();
    let index = builder.insert(&first);
    assert_eq!(index.as_usize(), 0);
    assert_eq!(builder.insert(&cloned), index);
    assert_eq!(builder.insert(&separate).as_usize(), 1);
    let table = builder.finish();
    assert_eq!(table.len(), 2);
    assert_eq!(table[index].value, 7);
    assert!(table.get(ResourceIndex::from_raw(2)).is_none());
    assert!(table == table.clone());
    assert_eq!(
        table
            .iter()
            .map(|resource| resource.value)
            .collect::<Vec<_>>(),
        [7, 7]
    );
    // Index traits likewise do not require Copy/Eq/Hash/Debug on Payload.
    let indices: HashSet<_> = [index, index, ResourceIndex::from_raw(1)]
        .into_iter()
        .collect();
    assert_eq!(indices.len(), 2);
    assert_eq!(std::mem::size_of_val(&index), std::mem::size_of::<usize>());
}

#[test]
fn ownership_survives_finish_and_retained_tables_and_releases_on_last_drop() {
    let drops = Arc::new(AtomicUsize::new(0));
    let resource = snapshot(9, &drops);
    let weak = resource.downgrade();
    let mut builder = ResourceTableBuilder::default();
    let index = builder.insert(&resource);
    drop(resource);
    assert_eq!(weak.upgrade().unwrap().value, 9);
    let table = builder.finish();
    let retained = table.clone();
    drop(table);
    assert_eq!(retained[index].value, 9);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(retained);
    assert!(weak.upgrade().is_none());
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn abandoning_builder_releases_its_resources_and_raw_tables_preserve_order() {
    let drops = Arc::new(AtomicUsize::new(0));
    let resource = snapshot(1, &drops);
    let mut builder = ResourceTableBuilder::default();
    builder.insert(&resource);
    drop(resource);
    drop(builder);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    let first = snapshot(2, &drops);
    let second = snapshot(3, &drops);
    let table: ResourceTable<_> = vec![second.clone(), first.clone(), second.clone()].into();
    assert_eq!(
        table
            .iter()
            .map(|resource| resource.value)
            .collect::<Vec<_>>(),
        [3, 2, 3]
    );
    assert!(table[0] == table[2]);
    assert!(ResourceTable::<Payload>::default().is_empty());
}

#[test]
fn resource_threads_retain_send_sync_payloads() {
    let resource = Resource::from_value(String::from("shared"));
    let reader = resource.clone();
    let returned = std::thread::spawn(move || {
        assert_eq!(&**reader, "shared");
        reader
    })
    .join()
    .unwrap();
    assert!(resource == returned);
}
