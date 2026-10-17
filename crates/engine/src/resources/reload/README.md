# Runtime resource hot reloading

Hot reload is disabled by default. Start a `ResourceReloadSession` explicitly to
watch filesystem images, translation catalogs and installed `.pixui` bundles.
The implementation is always compiled and tested; there is no Cargo feature or
automatic debug-build switch. Production applications simply do not start a
session. The showcase and todo examples explicitly start one by default; pass
`--no-hot-reload` to disable it.

## Setup

Configure the application image loader as usual, then start the shared service:

```rust,no_run
use std::{sync::Arc, time::Duration};
use pixui_engine::{
    application::app::Application,
    i18n::po::PoFormat,
    resources::{
        directory::DirectoryFilesystem, image_loader::ImageLoader,
        path::ResourcePath, reload::builder::ResourceReloadBuilder,
    },
};
let application = Application::new();
application.set_image_loader(ImageLoader::new(Arc::new(
    DirectoryFilesystem::new("assets")?,
)))?;
let german = application.register_language("de")?;
let session = ResourceReloadBuilder::new(application.clone())
    .watch_images()
    .catalog(
        Arc::new(DirectoryFilesystem::new("translations")?),
        ResourcePath::new("todos/de.po")?,
        "todos", german, Arc::new(PoFormat),
    )?
    .start()?;
session.wait_initial(Duration::from_secs(5))?;
// Create windows and run the native event loop here, retaining session.
session.stop(); // Or let the guard drop after the native loop returns.
# Ok::<(), pixui_base::PixuiError>(())
```

`watch_images()` uses the exact current application loader, including its limits
and directory layers. It does not configure a second, potentially different
image source. Catalog registration explicitly supplies the domain and registered
language, independently of directory naming. An adapter must implement
`TranslationFormat + Send + Sync`. Duplicate domain/language targets are errors.
You can watch catalogs without enabling image watching.

Startup registers recursive native watches before reading. Root directories
must already exist. `start()` reports watch/configuration errors immediately;
`wait_initial()` waits once for targets known at startup to install or exhaust
three attempts. A timeout can be retried. A file error keeps the session alive
for later recovery. Images requested after startup are not part of that wait.

## Shared pipeline

Native callbacks queue change hints without blocking. One background loader
thread coalesces changes by target, waits for 200 ms without observed changes,
then performs bounded reads and image decoding or catalog parsing. Configure
the delay with `quiet_period`; it must be greater than zero and at most 60 s.
Images retain their existing decode limits; catalogs have a 16 MiB encoded bound
before UTF-8 conversion and parsing. UI graphs use their `BundleLimits` and the
language lexer/parser limits.

The service hashes bytes with BLAKE3 before decoding. Unchanged successful bytes
do not create new image identities or reinstall catalogs. Only a successful
worker installation updates the accepted hash. Metadata changes or observed
notifications during loading discard that result and schedule another attempt.
A quiet period reduces incomplete saves but cannot prove a writer has finished.
Failures get at most three attempts per change, with at least 100 ms between
attempts. Distinct errors are reported through tracing and successful edits
recover automatically.

Prepared updates travel through the existing bounded application queue. Only
one prepared/sent update is outstanding, limiting extra payload memory and
avoiding a queue full of decoded images. New dirty hints supersede unsent
updates; session identity and per-target revisions reject stale queued results.
Catalog placeholder validation and template compilation remain on the worker
inside the existing transactional install path. They are not file reads or PO
parsing; expensive file work stays on the loader thread.

The worker swaps image snapshots directly into the image service and installs
catalogs atomically. Both invalidate all UIs and native metadata. Hidden windows
update titles/icons without painting. Old render outputs retain their strings
and immutable image snapshots. There is no renderer-specific reload logic.

## Images and layering

Only application image paths requested by components, icons, or `load_image`
are subscribed. Unused directory images are not eagerly decoded. First use of
an uncached watched image queues a background load and returns a pending-load
error through the existing result API. Existing published frames/properties are
kept until loading succeeds and invalidation causes another prepare. Direct
`ImageLoader::load` remains synchronous and is outside automatic reload.

Watched paths retain their latest successful image strongly for the session
lifetime; this guarantees reuse before and between prepares. Normal image
lookup remains weak when reload is disabled. Stopping clears watch-owned strong
references, while components, icons, and outputs keep their existing snapshots.
At most 4096 image/catalog targets can be subscribed per session. This bounds
bookkeeping, not aggregate pixel memory; large sets of watched images can retain
substantial memory until the session stops.

All directory layers are watched and every load resolves through the original
filesystem. Creating an override replaces a lower layer; removing it reveals
that lower layer. Malformed present overrides remain errors and never fall
through. Complete deletion keeps the last successful image/catalog rather than
blanking the UI. Create the registered path later to recover. New unknown PO
files do not invent domains/languages: register them explicitly in a new
session.

Directory and layered sources report native roots through the default-empty
`ResourceFilesystem::watch_roots` capability. Roots map directly to logical
relative paths. Custom sources without roots remain usable for ordinary loading
but cannot be watched. Loading retains directory containment and symlink checks.

## Lifecycle and limitations

Retain the guard outside the worker and native callbacks, for example on the
main stack around `host::run_with_renderer`. The guard holds the service's
application sender; `ApplicationHandle` itself remains only a cloneable sender.
Storing the guard on the application worker would create an ownership cycle.

`stop()` and drop cancel queued updates, release watch-owned images, stop native
watching, and join the loader. They do not wait for application queue capacity;
in-flight synchronous I/O/decoding may delay joining. Call them outside the
worker and latency-sensitive callbacks. Application failure stops the session.
Restart after replacing an image loader; updates from its old configuration are
rejected. Already installed catalogs remain installed after stopping.

Native notification behavior depends on the OS and filesystem. Queue overflow or
rescan hints reconcile all registered targets, without periodic filesystem
polling. Root deletion/replacement may require restarting the session. External
symlink trees, embedded/network invalidation, fonts and automatic language
discovery are outside this implementation. Explicit image handles in custom
state stay immutable: resolve their resource path again to get a new version.

See
[DR-015](../../../../../docs/decisions/DR-015%20Reload%20resources%20through%20a%20shared%20background%20pipeline.md)
for rationale and alternatives.

## UI source bundles

Install a baseline with `ApplicationHandle::install_bundle`, then add
`.ui_bundle(filesystem, &installed_bundle)?` to the same builder as images and
catalogs. Filesystem paths must match the bundle's canonical relative module
IDs. Point the directory filesystem at the UI source directory rather than the
repository root to avoid watching build outputs. Embedded/in-memory sources
remain useful for production startup and headless translation extraction.

The initial disk read and every later change compile the entire reachable
bundle on `pixui-resource-loader`: lexing, parsing, name resolution, type
checking, lowering and message preparation. The worker supplies immutable host
export/translation snapshots and links only a checked graph. It does not read
source files or rerun the language compiler during reload. Final application
ownership, event-schema and painter/style checks remain on the worker.

Imports use private relocation symbols during compilation. Successful worker
integration updates stable slots by canonical module/export identity and swaps
translations atomically. Stored `ComponentRef`s for surviving entries/exports
continue working. Removed exports are tombstones; they never redirect to a
new component, and restoring the same export restores its original slot.
Existing public prop/child-slot signatures and configured UI entries cannot
change incompatibly. A host `clone_template()` remains a frozen independent
copy; compose live roots through `DynamicPart` instead.

`InstalledBundle` retains its original export metadata snapshots. To inspect
current exports, call `Application::installed_bundle(installed.id())`; hosts can
also locate an unambiguous installed bundle using `bundle_for_entry(path)`.
The allowlist is the `Environment` supplied at initial installation. Rust
component/action registrations and code are not reloaded. Cross-bundle source
imports and automatic updates of externally supplied composite snapshots are
outside this first version.

Each accepted source graph resets **all UI state in the application**, including
unrelated definitions: mounted component/loop/match/slot state, retained layout,
focus, hover, scroll, text selection, IME/capture and pending clipboard
requests. New component state is created through `Default`. Window IDs,
subscriptions, presentation settings, diagnostics visibility and application
data survive. Hidden instances reset without painting and prepare when shown.
Focus handles carry an opaque epoch, and old input/editing revisions cannot
target replacement controls. Animation request counters stay monotonic; obsolete
schedules are removed with the old render snapshot.

An invalid or missing file leaves the complete last-good graph and state
untouched. A valid graph that fails at runtime keeps the last published frame
but disables its obsolete interaction geometry until recovery. Source/catalog
preparation uses a revisioned translation snapshot, so a concurrent PO install
or registry change rejects the stale candidate and triggers background retry.
Existing catalogs apply to newly introduced message keys as well. Snapshot
requests use nonblocking queue admission and cancellation-aware waiting, so
stopping a session does not wait for worker queue capacity.

Module contents and canonical names form one accepted content hash; an identical
save does not reset UI state. Dependency changes during reads/compilation are
discarded. Failed/missing imports remain watched and recover when created;
new unrelated files do not become entry points. All configured filesystem layers
remain active. Stable declarations reuse slots; genuinely new export names
consume the existing 4096-slot limit. Message indices remain append-only, with
identical keys reused. Stopping/restarting follows the same cancellation and
last-good-data rules as other resources.

`wait_initial()` reports initial source errors but leaves its session watching.
Keep the guard to recover from a later valid save. The showcase logs initial
errors and continues displaying its embedded baseline while watching.
