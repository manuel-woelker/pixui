use pixui_base::{Arena, Key};
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

#[test]
fn insert_lookup_mutation_and_growth_preserve_keys() {
    let mut arena = Arena::new();
    assert!(arena.is_empty());
    let keys: Vec<_> = (0..1000).map(|value| arena.insert(value)).collect();
    assert_eq!(arena.len(), 1000);
    for (value, key) in keys.iter().copied().enumerate() {
        assert_eq!(key.index(), value as u32);
        assert_eq!(key.arena_id(), arena.arena_id());
        assert_eq!(key.generation(), 1);
        assert!(arena.contains(key));
        assert_eq!(arena.get(key), Some(&value));
    }
    *arena.get_mut(keys[0]).unwrap() = 42;
    assert_eq!(arena.get(keys[0]), Some(&42));
}

#[test]
fn removing_and_reusing_slots_invalidates_all_old_keys() {
    let mut arena = Arena::new();
    let first = arena.insert("first");
    let retained = arena.insert("retained");
    assert_eq!(arena.remove(first), Some("first"));
    assert_eq!(arena.remove(first), None);
    let replacement = arena.insert("replacement");
    assert_eq!(replacement.index(), first.index());
    assert_eq!(replacement.generation(), first.generation() + 1);
    assert_eq!(replacement.arena_id(), first.arena_id());
    assert_ne!(first, replacement);
    assert!(!arena.contains(first));
    assert!(arena.get_mut(first).is_none());
    assert_eq!(arena.remove(first), None);
    assert_eq!(arena.get(replacement), Some(&"replacement"));
    assert_eq!(arena.get(retained), Some(&"retained"));
    assert_eq!(arena.len(), 2);
}

#[test]
fn foreign_keys_cannot_read_mutate_or_remove_same_type_values() {
    let mut first = Arena::new();
    let mut second = Arena::new();
    let a = first.insert(1);
    let b = second.insert(2);
    assert_eq!(a.index(), b.index());
    assert_eq!(a.generation(), b.generation());
    assert_ne!(a.arena_id(), b.arena_id());
    assert!(second.get(a).is_none());
    assert!(second.get_mut(a).is_none());
    assert!(second.remove(a).is_none());
    assert!(!first.contains(b));
    assert_eq!(second.get(b), Some(&2));
    let former_id = first.arena_id();
    drop(first);
    assert_ne!(Arena::<i32>::new().arena_id(), former_id);
}

#[test]
fn clear_invalidates_keys_and_preserves_identity_and_generation_history() {
    let mut arena = Arena::new();
    let id = arena.arena_id();
    let keys: Vec<_> = (0..10).map(|value| arena.insert(value)).collect();
    arena.remove(keys[3]);
    arena.clear();
    arena.clear();
    assert!(arena.is_empty());
    assert_eq!(arena.arena_id(), id);
    assert_eq!(arena.iter().count(), 0);
    let replacements: Vec<_> = (0..10).map(|value| arena.insert(value + 10)).collect();
    assert_eq!(
        replacements
            .iter()
            .map(|key| key.index())
            .collect::<HashSet<_>>()
            .len(),
        10
    );
    for key in keys {
        assert!(arena.get(key).is_none());
    }
    assert!(replacements.iter().all(|key| key.generation() == 2));
    assert_eq!(arena.len(), 10);
}

#[test]
fn iteration_skips_holes_and_exposes_valid_keys_in_index_order() {
    let mut arena = Arena::new();
    let keys: Vec<_> = (0..5).map(|value| arena.insert(value)).collect();
    arena.remove(keys[1]);
    arena.remove(keys[3]);
    assert_eq!(
        arena
            .iter()
            .map(|(key, value)| (key.index(), *value))
            .collect::<Vec<_>>(),
        [(0, 0), (2, 2), (4, 4)]
    );
    for (key, value) in arena.iter_mut() {
        *value += key.index() as i32;
    }
    for (key, value) in arena.iter() {
        assert_eq!(arena.get(key), Some(value));
    }
    assert_eq!(
        arena.iter().map(|(_, value)| *value).collect::<Vec<_>>(),
        [0, 4, 8]
    );
}

#[test]
fn keys_have_no_element_trait_bounds_and_can_be_hashed_and_sorted() {
    struct NoTraits(u32);
    let mut arena = Arena::default();
    let key = arena.insert(NoTraits(7));
    let copied: Key<NoTraits> = key;
    assert_eq!(key, copied);
    assert_eq!(std::mem::size_of_val(&key), 8);
    assert!(format!("{key:?}").contains("generation"));
    let mut map = HashMap::new();
    map.insert(key, "value");
    assert_eq!(map.get(&copied), Some(&"value"));
    let next = arena.insert(NoTraits(8));
    let mut keys = [next, key];
    keys.sort();
    assert_eq!(keys, [key, next]);
    assert_eq!(arena.get(key).unwrap().0, 7);
}

#[test]
fn values_are_dropped_once_and_removal_transfers_ownership() {
    struct Probe(Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let mut arena = Arena::new();
    let key = arena.insert(Probe(drops.clone()));
    arena.insert(Probe(drops.clone()));
    let removed = arena.remove(key).unwrap();
    assert_eq!(drops.get(), 0);
    drop(removed);
    assert_eq!(drops.get(), 1);
    arena.clear();
    assert_eq!(drops.get(), 2);
    arena.insert(Probe(drops.clone()));
    drop(arena);
    assert_eq!(drops.get(), 3);
}

#[test]
fn arena_ids_are_unique_across_types_and_threads() {
    let ids: Vec<_> = (0..32)
        .map(|_| std::thread::spawn(|| Arena::<()>::new().arena_id()))
        .collect();
    let mut ids: HashSet<_> = ids
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    assert_eq!(ids.len(), 32);
    assert!(ids.insert(Arena::<String>::new().arena_id()));
    assert!(ids.insert(Arena::<u32>::new().arena_id()));
    assert!(!ids.contains(&0));
}

#[test]
fn arena_can_store_borrowed_values() {
    let text = String::from("borrowed");
    let mut arena = Arena::new();
    let key = arena.insert(text.as_str());
    assert_eq!(arena.get(key), Some(&"borrowed"));
}
