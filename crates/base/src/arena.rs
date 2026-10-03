//! A typed arena with packed 64-bit generational keys.
//!
//! See [`Arena`] for limits and the vacant-generation convention.

use std::{
    fmt,
    hash::{Hash, Hasher},
    marker::PhantomData,
    sync::atomic::{AtomicU32, Ordering},
};

use crate::{PixuiResult, message};

const INDEX_BITS: u32 = 32;
const ARENA_BITS: u32 = 16;
const GENERATION_BITS: u32 = 16;
const ARENA_SHIFT: u32 = INDEX_BITS;
const GENERATION_SHIFT: u32 = INDEX_BITS + ARENA_BITS;
const VACANT: u16 = 0;

const _: () = assert!(INDEX_BITS + ARENA_BITS + GENERATION_BITS == 64);
const _: () = assert!(size_of::<Key<()>>() == 8);
const _: () = assert!(size_of::<Key<String>>() == 8);

static NEXT_ARENA_ID: AtomicU32 = AtomicU32::new(1);

/// A typed handle containing a 32-bit index, 16-bit arena ID, and 16-bit generation.
///
/// The entire key occupies exactly 64 bits, including its zero-sized type marker.
/// Keys are Copy regardless of `T`, do not own `T`, and may outlive the arena.
/// The marker prevents mixing keys of different element types. Runtime arena
/// identity and generation checks reject foreign and stale keys of the same type.
/// Keys are process-local handles, not persistent identifiers or capabilities.
#[repr(transparent)]
pub struct Key<T> {
    bits: u64,
    marker: PhantomData<fn() -> T>,
}

impl<T> Key<T> {
    fn new(index: u32, arena_id: u16, generation: u16) -> Self {
        Self {
            bits: u64::from(index)
                | (u64::from(arena_id) << ARENA_SHIFT)
                | (u64::from(generation) << GENERATION_SHIFT),
            marker: PhantomData,
        }
    }

    pub fn index(self) -> u32 {
        self.bits as u32
    }
    pub fn arena_id(self) -> u16 {
        (self.bits >> ARENA_SHIFT) as u16
    }
    pub fn generation(self) -> u16 {
        (self.bits >> GENERATION_SHIFT) as u16
    }
    /// Exposes the packed bits for diagnostics. No unchecked key constructor is provided.
    pub fn bits(self) -> u64 {
        self.bits
    }
}

impl<T> Copy for Key<T> {}
impl<T> Clone for Key<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> PartialEq for Key<T> {
    fn eq(&self, other: &Self) -> bool {
        self.bits == other.bits
    }
}
impl<T> Eq for Key<T> {}
impl<T> PartialOrd for Key<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> Ord for Key<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.bits.cmp(&other.bits)
    }
}
impl<T> Hash for Key<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bits.hash(state);
    }
}
impl<T> fmt::Debug for Key<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Key")
            .field("index", &self.index())
            .field("arena_id", &self.arena_id())
            .field("generation", &self.generation())
            .finish()
    }
}

struct Slot<T> {
    // Zero means unoccupied; live generations are always nonzero.
    generation: u16,
    value: SlotValue<T>,
}

enum SlotValue<T> {
    Occupied(T),
    // Zero next_generation means retired. Otherwise it is the generation to
    // assign on reuse, preserving history despite the vacant sentinel above.
    Vacant { next_generation: u16 },
}

/// Owns values addressed by typed generational keys.
///
/// Insert, lookup, and removal are O(1) (insertion amortized). Iteration and clear
/// scan allocated slots, including holes and permanently retired slots. Values
/// need not implement Clone, Copy, or any reflection trait. Vec growth may move
/// values; keys remain valid, but this is not stable-address or pinned storage.
///
/// Keys allocate 32 bits to index, 16 to arena identity, and 16 to generation.
/// At most 2^32 slots can be allocated per arena on platforms supporting that
/// capacity. IDs 1..=65535 are issued process-wide and never recycled, including
/// after arena destruction. [`Self::try_new`] returns an error at ID exhaustion.
/// Different types share the ID allocator; allocation is safe across threads.
///
/// Entry generation zero denotes an unoccupied slot. Its vacant payload retains
/// the next generation, since forgetting it would make stale keys valid again.
/// Slots are retired after generation 65535 rather than wrapping. Retired slots
/// consume index space but are never reused. The safe Rust payload enum still
/// has a discriminant; using zero does not promise a smaller entry layout.
///
/// ```
/// use pixui_base::Arena;
/// let mut arena = Arena::new();
/// let old = arena.insert(String::from("first"));
/// assert_eq!(arena.get(old).map(String::as_str), Some("first"));
/// assert_eq!(arena.remove(old).as_deref(), Some("first"));
/// let new = arena.insert(String::from("replacement"));
/// assert_eq!(old.index(), new.index());
/// assert_ne!(old.generation(), new.generation());
/// assert!(arena.get(old).is_none());
/// ```
///
/// Keys of different element types cannot be mixed:
/// ```compile_fail
/// use pixui_base::Arena;
/// let mut strings = Arena::<String>::new();
/// let key = strings.insert(String::from("value"));
/// let numbers = Arena::<u32>::new();
/// numbers.get(key);
/// ```
pub struct Arena<T> {
    id: u16,
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
    len: usize,
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Arena<T> {
    /// Creates an arena. Panics if the process-wide arena ID space is exhausted.
    pub fn new() -> Self {
        Self::try_new().expect("arena ID space exhausted")
    }

    pub fn try_new() -> PixuiResult<Self> {
        let id = allocate_id(&NEXT_ARENA_ID).ok_or_else(|| message("arena ID space exhausted"))?;
        Ok(Self {
            id,
            slots: Vec::new(),
            free: Vec::new(),
            len: 0,
        })
    }

    pub fn arena_id(&self) -> u16 {
        self.id
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Inserts a value. Panics if the arena's index space is exhausted.
    /// Allocation failure follows the standard Vec allocation behavior.
    pub fn insert(&mut self, value: T) -> Key<T> {
        let index = if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            let SlotValue::Vacant { next_generation } = slot.value else {
                unreachable!("free slot must be vacant")
            };
            debug_assert_ne!(next_generation, VACANT);
            slot.generation = next_generation;
            slot.value = SlotValue::Occupied(value);
            index
        } else {
            let index = u32::try_from(self.slots.len()).expect("arena index space exhausted");
            self.slots.push(Slot {
                generation: 1,
                value: SlotValue::Occupied(value),
            });
            index
        };
        self.len += 1;
        Key::new(index, self.id, self.slots[index as usize].generation)
    }

    pub fn contains(&self, key: Key<T>) -> bool {
        self.get(key).is_some()
    }

    pub fn get(&self, key: Key<T>) -> Option<&T> {
        let slot = self.slots.get(self.valid_index(key)?)?;
        match &slot.value {
            SlotValue::Occupied(value) => Some(value),
            SlotValue::Vacant { .. } => None,
        }
    }

    pub fn get_mut(&mut self, key: Key<T>) -> Option<&mut T> {
        let index = self.valid_index(key)?;
        match &mut self.slots[index].value {
            SlotValue::Occupied(value) => Some(value),
            SlotValue::Vacant { .. } => None,
        }
    }

    /// Removes a value, invalidating its key immediately. Returns ownership to the caller.
    pub fn remove(&mut self, key: Key<T>) -> Option<T> {
        let index = self.valid_index(key)?;
        let slot = &mut self.slots[index];
        let next_generation = slot.generation.checked_add(1).unwrap_or(VACANT);
        let old = std::mem::replace(&mut slot.value, SlotValue::Vacant { next_generation });
        slot.generation = VACANT;
        if next_generation != VACANT {
            self.free.push(index as u32);
        } else {
            tracing::warn!(
                arena_id = self.id,
                index = key.index(),
                generation = key.generation(),
                "Arena slot retired after generation exhaustion"
            );
        }
        self.len -= 1;
        match old {
            SlotValue::Occupied(value) => Some(value),
            SlotValue::Vacant { .. } => unreachable!("valid keys address occupied slots"),
        }
    }

    /// Removes all live values while preserving generation history and arena identity.
    pub fn clear(&mut self) {
        for index in 0..self.slots.len() {
            let generation = self.slots[index].generation;
            if generation != VACANT {
                self.remove(Key::new(index as u32, self.id, generation));
            }
        }
    }

    /// Iterates occupied slots in index order; insertion order is not guaranteed after reuse.
    pub fn iter(&self) -> impl Iterator<Item = (Key<T>, &T)> {
        let id = self.id;
        self.slots
            .iter()
            .enumerate()
            .filter_map(move |(index, slot)| match &slot.value {
                SlotValue::Occupied(value) => {
                    Some((Key::new(index as u32, id, slot.generation), value))
                }
                SlotValue::Vacant { .. } => None,
            })
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Key<T>, &mut T)> {
        let id = self.id;
        self.slots
            .iter_mut()
            .enumerate()
            .filter_map(move |(index, slot)| match &mut slot.value {
                SlotValue::Occupied(value) => {
                    Some((Key::new(index as u32, id, slot.generation), value))
                }
                SlotValue::Vacant { .. } => None,
            })
    }

    fn valid_index(&self, key: Key<T>) -> Option<usize> {
        if key.arena_id() != self.id || key.generation() == VACANT {
            return None;
        }
        let index = key.index() as usize;
        (self.slots.get(index)?.generation == key.generation()).then_some(index)
    }
}

fn allocate_id(next: &AtomicU32) -> Option<u16> {
    next.try_update(Ordering::Relaxed, Ordering::Relaxed, |id| {
        (id <= u32::from(u16::MAX)).then_some(id + 1)
    })
    .ok()
    .map(|id| id as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_layout_and_boundary_fields_are_exact() {
        assert_eq!(size_of::<Key<()>>(), size_of::<u64>());
        let key = Key::<String>::new(u32::MAX, u16::MAX, u16::MAX);
        assert_eq!(key.bits(), u64::MAX);
        assert_eq!(key.index(), u32::MAX);
        assert_eq!(key.arena_id(), u16::MAX);
        assert_eq!(key.generation(), u16::MAX);
        assert_eq!(Key::<()>::new(1, 2, 3).bits(), 1 | (2 << 32) | (3 << 48));
    }

    #[test]
    fn id_allocator_never_wraps_or_reuses_ids() {
        let next = AtomicU32::new(u32::from(u16::MAX));
        assert_eq!(allocate_id(&next), Some(u16::MAX));
        assert_eq!(allocate_id(&next), None);
        assert_eq!(allocate_id(&next), None);
        assert_eq!(next.load(Ordering::Relaxed), u32::from(u16::MAX) + 1);
    }

    #[test]
    fn vacant_generation_zero_preserves_next_generation_and_retires_at_limit() {
        let mut arena = Arena::new();
        let first = arena.insert(1);
        arena.remove(first);
        assert_eq!(arena.slots[0].generation, VACANT);
        assert!(matches!(
            arena.slots[0].value,
            SlotValue::Vacant { next_generation: 2 }
        ));
        let second = arena.insert(2);
        arena.slots[0].generation = u16::MAX;
        let last = Key::new(0, arena.id, u16::MAX);
        assert_eq!(arena.remove(last), Some(2));
        assert_eq!(arena.slots[0].generation, VACANT);
        assert!(matches!(
            arena.slots[0].value,
            SlotValue::Vacant { next_generation: 0 }
        ));
        let next = arena.insert(3);
        assert_eq!(next.index(), 1);
        for stale in [first, second, last] {
            assert!(arena.get(stale).is_none());
        }
    }

    #[test]
    fn forged_zero_and_out_of_bounds_keys_are_rejected() {
        let mut arena = Arena::new();
        let live = arena.insert(1);
        for key in [
            Key::new(live.index(), arena.id, 0),
            Key::new(u32::MAX, arena.id, 1),
        ] {
            assert!(arena.get(key).is_none());
            assert!(arena.get_mut(key).is_none());
            assert!(arena.remove(key).is_none());
        }
        assert_eq!(arena.len(), 1);
    }
}
