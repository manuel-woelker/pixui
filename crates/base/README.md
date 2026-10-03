# pixui-base

Shared arenas, strings, errors, and results for pixui.

## Typed arena

```rust
use pixui_base::{Arena, Key};

let mut arena = Arena::new();
let key: Key<String> = arena.insert("hello".into());
arena.get_mut(key).unwrap().push('!');
assert_eq!(arena.get(key).map(String::as_str), Some("hello!"));
assert_eq!(arena.remove(key).as_deref(), Some("hello!"));
assert!(!arena.contains(key));
```

`Arena<T>` owns values and supports insertion, checked shared/mutable lookup,
removal, clearing, and iteration. Removal returns ownership to the caller.
Clearing and dropping an arena drop its remaining values. Values can contain
borrows and need no special traits. Vector growth can move values, so this is
stable handle storage, not stable address storage.

### Key layout and limits

`Key<T>` packs all three components into a single `u64`:

| Component | Bits | Position | Limit |
|---|---|---|---|
| Index | 32 | 0–31 | 2^32 allocated slots per arena |
| Arena ID | 16 | 32–47 | 65,535 arenas over the process lifetime |
| Generation | 16 | 48–63 | 65,535 occupied generations per slot |

Compile-time assertions verify the total bit count and eight-byte key size.
The type marker occupies no space. Keys implement Copy, equality, ordering,
hashing, and Debug without requiring those traits on `T`. Different key types
cannot be mixed; runtime arena and generation checks reject foreign or stale
keys of the same type. Unknown, removed, or foreign keys return `None`.

Arena IDs are allocated atomically across all types and threads and never
reused after destruction. `try_new()` reports exhaustion; `new()` and `default()`
panic in that case. This finite lifetime allocation budget is a deliberate
consequence of the 32/16/16 layout. The process starts fresh with its own IDs;
keys must not be persisted or exchanged between processes.

Slots start at generation one. A reusable slot advances its generation after
removal. The last generation retires the slot permanently rather than wrapping,
so a stale key can never become valid again. Retired slots consume index space.
Retirement emits a `tracing` warning with the arena ID, slot index, and generation.
Insertion panics when no reusable slot exists and the index space is exhausted;
actual capacity is also subject to platform and allocator limits.

### Reserving a generation for vacant slots

Generation zero is reserved in entries to mean unoccupied. Zero is never issued
in a key. A vacant entry stores its **next generation** in its payload, and a
separate free list identifies reusable indices. A next generation of zero means
the slot is retired and must not enter the free list.

Reserving one value is sufficient to mark vacancy, but not to remember reuse
history. Resetting an entry to zero and restarting at one would revive its old
keys. Keeping the next generation in the vacant payload preserves that history
without adding a next-generation field to the occupied payload.

The payload uses a safe Rust enum: an occupied `T` or a vacant next-generation
counter. It still has a discriminant; the sentinel is not a claim that entries
have a compact or fixed layout. Eliminating the discriminant would require a
different representation and additional initialization/drop reasoning. The
current implementation uses no unsafe code.

### Complexity and reuse

Lookup and removal are O(1); insertion is amortized O(1). Vacant slots are reused
in free-list order. Iteration and clear scan all allocated slots, including holes
and retired entries. Iteration follows slot index order, which can differ from
insertion order after reuse. `clear()` preserves arena identity and generation
history; it does not reset the arena to generation one.

If a workload creates many short-lived arenas, reconsider the bit split before
depending on the current lifetime ID budget. Any scheme that recycles IDs must
also prevent surviving keys from matching a later arena.
