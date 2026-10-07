# Resource filesystems

Resource lookup is independent of decoding and rendering. A `ResourceFilesystem`
opens an owned `Read + Send` reader for a validated `ResourcePath`; only `None`
means absence. Readers do not need seeking. Empty files, read failures,
oversized files and malformed images are not absence.

`ResourcePath` owns a `PixuiString`. It accepts nonempty slash-separated
relative filenames and rejects absolute paths, empty/dot/parent components,
backslashes, colons and NULs. Paths are logically case-sensitive; use exact
spelling on disk for portability. Directory roots are canonicalized during
construction. Symlinks resolving outside the root fail; containment is not a
security sandbox against concurrent hostile filesystem mutations.

`LayeredFilesystem` searches in constructor order. The first opened reader wins;
opening errors stop lookup. Consumers must not retry lower layers after read or
decode errors. Layers can nest; an empty stack has no resources.

```rust,no_run
use std::sync::Arc;
use pixui_engine::resources::{
    directory::DirectoryFilesystem,
    image_loader::ImageLoader,
    layered::LayeredFilesystem,
};
let filesystem = LayeredFilesystem::new(vec![
    Arc::new(DirectoryFilesystem::new("user-assets")?),
    Arc::new(DirectoryFilesystem::new("assets")?),
]);
let loader = ImageLoader::new(Arc::new(filesystem));
let logo = loader.load_str("images/pixui-logo.png")?;
let shared = logo.clone(); // Same resource identity, no pixel copy.
# Ok::<(), pixui_base::PixuiError>(())
```

## Image loading and limits

`ImageLoader` is itself a shared handle; clone it without an extra `Arc`. Each
load creates a new snapshot. Retain the returned image for reuse; the loader has
no filename cache, invalidation protocol or automatic reload. Older display
lists keep old snapshots alive independently of replacement images.

PNG and JPEG are selected by their signatures, not extensions. PNG alpha is
preserved in RGBA; opaque formats use RGB. Higher precision channels are
converted to eight bits. APNG and other formats are rejected. There is no color
profile management, EXIF orientation transform or animation playback in this
first loader.

Default per-load limits are 32 MiB encoded bytes, 64 million pixels and 256 MiB
for decode buffers. `ImageLoadLimits` customizes these limits within the engine
pixel ceiling. Reads stop at the encoded bound plus one byte to detect overflow,
even for readers of unknown length. Dimensions and the combined decoded/final
buffer size are checked before decoding. Decoder internal allocation limits are
best effort in the underlying library; these limits are not a process-wide
memory budget. Encoded bytes and allocator overhead are additional memory.

Loading is synchronous. Use during setup, component preparation, or explicit
actions, never during
painting or on the native UI thread. Large runtime loads block their caller and
should eventually use background work. An embedded source can return a cursor
over static bytes. Network sources need background fetching or an explicit async
interface; pending data must never be reported as absent for fallback purposes.

## Application image component

`Application::set_image_loader` (also available on `ApplicationHandle`)
configures resource lookup for the standard core `ImageComponent`. Its prop is a
relative `ResourcePath`, constructed conveniently with
`ImageProps::new(filename)`. `Component::prepare` resolves it into `ImageState`
before any painting begins.

`Application::load_image` shares live snapshots through a worker-local weak
cache. The direct `ImageLoader::load` API still creates fresh snapshots. The
application cache releases pixel ownership to component states and render
outputs; dead lookup entries are pruned on successful misses. There is no
watcher or timestamp check. Replacing the application loader resets the cache
and invalidates existing UIs, making resource replacement explicit. Failure
leaves existing image state and published output intact. An application with no
configured loader reports a clear error when preparing its first image
component.
