# Shared render resources

Images, font snapshots, and glyph atlases share the same ownership mechanism:
`Resource<T>` wraps an `Arc<T>` and exposes read-only payload access. Cloning
copies the handle, not the contents. Creating a resource creates a new version,
even if its contents are equal. Resource and table equality compare allocation
identity; compare payload fields explicitly when checking contents.

Domain aliases retain their own validated constructors and payload contracts:

- `Image = Resource<ImageData>`: immutable RGB pixels and optional color key.
- `Font = Resource<FontResource>`: character map, metrics, scale, and atlas.
- `GlyphAtlas = Resource<GlyphAtlasData>`: immutable grayscale coverage.

Parsed font faces also use the shared handle internally, while hiding their
rasterizer payload. Resource cloning, identity, and table operations do not
require `T: Clone`, `T: Eq`, or `T: Hash`. Debug output requires `T: Debug`.
Send/Sync follow the payload. The generic wrapper cannot forbid interior
mutability in arbitrary T; rendering payloads must remain immutable.

## Tables and indices

`ResourceTableBuilder<T>::insert` deduplicates by snapshot identity and returns
`ResourceIndex<T>`. The builder retains each allocation while its reverse lookup
exists, preventing address reuse from matching a different snapshot. `finish`
discards lookup metadata and returns an immutable `ResourceTable<T>`.

```rust
use pixui_engine::ui::{
    resource::Resource,
    resource_table::{ResourceIndex, ResourceTableBuilder},
};
let resource = Resource::from_value(String::from("snapshot"));
let mut builder = ResourceTableBuilder::default();
let index = builder.insert(&resource);
assert_eq!(index, builder.insert(&resource.clone()));
let table = builder.finish();
assert_eq!(&**table.get(index).unwrap(), "snapshot");
assert!(table.get(ResourceIndex::from_raw(1)).is_none());
```

Image and font tables stay separate and typed. An `ImageIndex` cannot be used to
index the font table, and a `FontIndex` cannot index the image table. Indices
are local to a table/frame; the type does not distinguish two tables with the
same payload type. They are not persistent IDs or checked handles.

`ResourceIndex::from_raw` is deliberately unchecked for deserialization and
validation. Use `get` for potentially invalid indices; indexing panics outside
bounds. `ResourceTable::from(Vec<_>)` preserves order and duplicates for
externally constructed command lists. Positional `usize` indexing is available
for inspection; commands use the typed indices.

The display-list builder inserts images immediately. Font commands initially
refer to pending demands. After all fonts finalize, the builder inserts their
snapshots into the shared table builder and translates demand indices to final
resource indices. A failed preparation returns no partial list.

## Lifetime and caching

Tables own resources, and cloning a table retains its snapshots. Cache eviction
or dropping newer/intermediate frames cannot invalidate a retained frame.
Dropping the last owner releases the payload. `downgrade` provides a standard
weak reference for observing lifetime without retaining the payload.

`ResourceIdentity<T>` is only a lookup token; it owns nothing. An allocation
address can be reused after its resource dies. Every cache keyed by identities
must retain the corresponding resource for as long as the key is stored. The
resource-table builder and font cache enforce this relationship.

This infrastructure supplies ownership and indexing, not pixel formats,
rasterization, cache budgets, upload protocols, or renderer-specific handles.
See [images](Images.md) and [text](Text.md) for those domain contracts.
