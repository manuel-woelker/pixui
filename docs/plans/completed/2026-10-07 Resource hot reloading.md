# Resource hot reloading

Status: completed; automated checks passed and the user confirmed it works.

## Goal

Reload filesystem images and PO translations without restarting the application.
Share directory watching, debounce, background loading, delivery, and lifecycle
handling. Keep decoding and catalog parsing off the application/UI threads.
Publish immutable replacements on the application worker and preserve the last
successful resource when a reload fails.

## Decisions

- Use native directory notifications through `notify::recommended_watcher`,
  recursively watching configured directory roots. Watch directories rather
  than individual files so atomic-save rename patterns and new files work.
  No periodic polling backend in this first version.
- Hot reload is available in normal builds through a regular `notify`
  dependency. It is disabled at runtime by default and starts only on explicit
  request. Production disables it by not starting a reload session; no Cargo
  feature is involved. The examples explicitly enable it by default and expose
  `--no-hot-reload` to disable it (`--hot-reload` enables it again).
- Use one reload session per application, with one background coordinator/loader
  thread initially. The OS backend may create its own internal threads.
  Image and catalog jobs share this thread and scheduling code; their decode
  and install steps remain concrete, type-specific operations.
- Start with a configurable 200 ms quiet period per logical resource. Reset it
  after each relevant event. Treat events as hints to reread current contents,
  not as an authoritative file-change log.
- Decode images and parse PO text in the background. Keep registry-dependent
  catalog validation and transactional installation on the application worker.
  This preserves existing append-only registration and atomic install behavior.
- Publish only complete successful replacements. Existing render outputs retain
  their old `Arc` image snapshots and owned text. Never mutate pixels in place.
- Use broad UI invalidation initially, matching existing image-loader and
  catalog replacement. Update native metadata for hidden windows without
  painting them.

## Existing integration points

`ResourceFilesystem` provides bounded-consumer synchronous readers.
`DirectoryFilesystem` canonicalizes roots and checks containment;
`LayeredFilesystem` resolves the first present resource. `ImageLoader` decodes
fresh immutable snapshots. The application `ImageService` shares snapshots using
weak references, and `ImageComponent::prepare` resolves its path each render.

Translations currently use `include_str!` in both examples. `PoFormat` produces
neutral catalogs; `Application::install_translations` validates, compiles, and
installs atomically, then invalidates UIs. The bounded application command queue
already supports transferring owned data to the worker.

## Shared resource sources and registration

Extend `ResourceFilesystem` with an object-safe, default-empty capability to
report native watch roots. A directory reports its canonical root mapping
directly onto relative resource paths; layered sources combine these roots.
Arbitrary mount-prefix remapping is outside the existing filesystem API. Custom
embedded/network sources remain unchanged and unwatched. Keep watching
configuration independent of decoding. Do not downcast filesystem trait objects
or require every source to implement OS watching.

Map event paths lexically relative to registered canonical roots, including
paths that no longer exist. Construct validated `ResourcePath`s and perform
actual loading through the original filesystem, preserving symlink containment
and layered priority. Watch every directory layer: creating an override must
replace a previously selected lower-layer resource. Read/decode failures in a
present higher-priority layer must not fall through to lower layers.

Register typed reload targets:

- Images: resource paths requested through the application's configured image
  service. Subscribe on first use when a reload session is active. A file event
  for an unused image does not eagerly decode it. A newly created file for a
  previously requested missing path triggers a retry and invalidation.
- Catalogs: explicitly associate a filesystem, `ResourcePath`, domain,
  `LanguageIndex`, and shared `TranslationFormat + Send + Sync` adapter. Initial
  loading and subsequent reloads use the same bounded background job. Do not
  infer domains/languages from arbitrary directory names or install unknown PO
  files automatically. A registered but absent catalog may be created later.

For the examples, register existing German PO paths explicitly and configure
image roots through the existing resource filesystem. Retain embedded catalogs
as the deterministic production/export baseline. Hot-reload mode installs disk
catalogs over that baseline; export must remain display-free and not start a
watcher. A newly added language requires explicit registration in this version.

Illustrative API, subject to implementation details:

```rust,ignore
let reload_session = ResourceReloadBuilder::new(application.clone())
    .quiet_period(Duration::from_millis(200))
    .watch_images() // Uses the application's configured loader.
    .catalog(
        translations_filesystem,
        ResourcePath::new("todos/de.po")?,
        "todos",
        german,
        Arc::new(PoFormat),
    )?
    .start()?;
reload_session.wait_initial(Duration::from_secs(5))?;
```

Enforce that watched image sources match the application's configured loader;
prefer a combined setup method over letting these configurations diverge.
Reject duplicate conflicting catalog targets. Specify startup completion/error
reporting so callers can await initial loads before creating windows if desired.

## Common event and loading pipeline

1. Register directory watches before initial reads; reconcile registered targets
   afterward so edits during startup are not lost. Deduplicate equivalent roots.
2. The native callback only queues lightweight dirty-path hints. Ignore access
   events so reading a resource does not trigger a reload loop. Handle creates,
   writes, removals, directory changes, and both sides of renames.
3. Map hints to logical targets. Directory-level changes dirty registered
   descendants. Coalesce repeated events per target and assign a monotonically
   increasing revision. Maintain a bounded dirty set; callback queue overflow or
   backend rescan flags request reconciliation of all registered targets.
4. After the quiet period, read with existing byte limits, decode an image or
   import a neutral catalog on the background thread. For PO, enforce bounded
   reads before UTF-8 conversion/parser allocation, not only inside `PoFormat`.
5. Compare file metadata before/after reads and drain queued hints before
   delivery; retry if a change was observed during loading. A quiet period is
   best effort, not proof that a writer finished. Decode failures keep the last
   good value and get a small bounded retry schedule for incomplete writes.
6. Send a typed prepared update containing session/configuration identity,
   target, revision, and owned payload to the application worker. Coalesce
   pending updates by target under backpressure. Bound queued/in-flight payloads
   and preserve latest work when the command queue is full.
7. The worker checks identities and revision, applies a successful replacement,
   and invalidates UIs. Drop results from stopped/replaced sessions and older
   revisions; serialize acceptance with dirty notifications. A newer change
   observed after acceptance will produce another update normally.

Use a small internal job/payload enum for images and catalogs with shared
scheduling. Do not build a general plugin framework or a second application
state owner. Compare accepted file bytes using a bounded retained digest or
content identity to avoid decoding/installing unchanged files after noisy
notifications; update the accepted identity only after successful installation.

## Worker-side installation and ownership

### Images

Add a direct image-service replacement operation; invalidating a weak cache and
rereading during preparation would defeat background loading. Watched active
paths retain the latest successful snapshot strongly for the session lifetime,
so a newly delivered image cannot disappear before the next prepare. The normal
non-watched image cache stays weak. Dropping the session releases these retained
snapshots; component states and old outputs continue owning theirs.

`ImageComponent` picks up the new snapshot on preparation. Explicitly retained
images in custom component state or native icon callbacks do not magically
change: those consumers must resolve the resource path again. Adapt example
icons to path-based lookup if needed and document this boundary.

### Catalogs

Pass a neutral parsed catalog to the existing transactional install path. Keep
placeholder/schema validation and compilation on the worker initially; these
are bounded by catalog/template limits. If profiling identifies installation
stalls, prepare compiled templates against a versioned registry snapshot later.
Missing catalog entries still fall back to source text as today. Invalid
catalogs leave all previous translations and titles intact.

### Removal and failure

- Removal of a winning layered file reloads from the remaining layers through
  normal lookup. A malformed present override never permits fallback.
- If no layer contains the target, keep its last good value and report absence.
  Do not silently blank an image or uninstall a whole catalog on deletion.
- Watch registration errors fail startup with useful path context; runtime read,
  decode, parser, installation, and backend errors emit structured diagnostics
  using the repository's tracing mechanism. Repeated errors should be coalesced.
- On a later valid edit/create, automatically recover. Retry timers are bounded;
  permanent failures do not create busy loops.

## Lifecycle and production behavior

Return an explicit `ResourceReloadSession` guard owned by the caller, separate
from `ApplicationHandle`, which remains a cheaply cloneable sender. The guard
owns cancellation and shutdown. Its drop stops watching, wakes the loader,
releases registrations, and joins the background thread. Never block a native
callback or the application worker waiting for shutdown. Provide an explicit
stop/join operation for deterministic teardown before dropping the application.

The reload service necessarily holds an application sender while running. Do
not store the guard on the worker and create a sender/worker ownership cycle.
Cancellation must interrupt pending sends/retries even when the application
queue is full; in-flight synchronous decoding may finish before join returns.
Worker disconnect or failure terminates the reload service and drops payloads.
Session/configuration identities make late queued updates harmless after stop
or image-loader replacement. No watcher, background loader thread, or watch-only
snapshot retention exists unless runtime reload is enabled.

## Implementation checklist

- [x] Add the normal dependency and document runtime opt-in, disabled by default
  in the engine API. Examples opt in by default.
- [x] Expose source watch-root mappings through directory/layered filesystems,
  preserving path validation and fallback rules.
- [x] Implement explicit target registration, image-use subscriptions, session
  identities, and configuration validation.
- [x] Implement the shared native notification/debounce/background job pipeline,
      bounded work queues, rescan reconciliation, retries, and stale-result
      handling.
- [x] Add prepared image replacement and watched-snapshot retention to the image
  service; preserve ordinary weak-cache behavior.
- [x] Add file-backed catalog loading through the same pipeline and existing
  transactional worker installation.
- [x] Implement cancellation, shutdown, disconnect handling, and diagnostics.
- [x] Add default-enabled example hot-reload wiring with `--no-hot-reload` for
      images and German catalogs; retain normal embedded catalogs and headless
      export behavior.
- [x] Update resource/i18n API guides and architecture documentation. Record
  background preparation, immutable swaps, and lifecycle choices in a decision
  record if adopted.
- [x] Add automated tests and run `./n check` after implementation/fix units.
- [x] Native GUI verification: user confirmed it works after rebooting to load
  the updated NVIDIA driver.

## Verification

- Deterministic scheduler tests with injected events/clock: quiet-period reset,
  independent paths, coalescing, duplicate events, bounded retries, queue
  overflow reconciliation, and no read-event loops.
- Shared pipeline tests exercise both image and PO targets, including initial
  reads, newly created registered paths, atomic rename saves, directory moves,
  malformed intermediate files, and successful recovery.
- Worker tests verify stale session/revision rejection, replacement-loader
  rejection, last-good data, unchanged-file suppression, multiple windows,
  image resource identity changes, and immutable old outputs.
- Layered filesystem tests cover creating/removing overrides, shadowed lower
  edits, nested layers, missing paths, invalid present files, and containment.
- Catalog tests cover live German text/title updates, placeholder errors,
  source fallback for missing entries, and domain/language isolation.
- Lifecycle tests cover full queues, dropped workers, stop during debounce/load,
  repeated start/stop, no sender cycle, and no watch-only image retention after
  shutdown. Assert expensive reads/decoding/parsing happen off the worker/UI.
- Native backend integration tests use temporary directories and bounded
  eventual assertions, avoiding exact event counts and arbitrary fixed sleeps.
  Exercise create, overwrite, rename, and remove for images and PO on CI hosts.
- Run `./n check`, using the normal compilation, clippy, and nextest tasks.
  Verify runtime-disabled behavior without special build flags.
- Manually run showcase/todo with their default reload settings. Edit the logo
  and German PO while windows are open, verify both windows and native titles
  update, try an incomplete/invalid save, then recover. Verify disabled mode
  never reacts.

## Implementation and verification results

- Runtime-only opt-in is implemented with normal dependencies; no Cargo feature
  or debug-build default is involved. The plan was committed before
  implementation.
- The session uses one coordinator/loader thread, a 256-event hint queue,
  4096-target cap, one prepared/in-flight update, and BLAKE3 accepted-content
  hashes. Retry policy is three attempts per observed change, with at least
  100 ms between retries. Debounce defaults to 200 ms and is configurable.
- Watch-root capability maps native roots directly to relative filenames.
  Layered sources combine roots and retain their normal resolution semantics.
- `watch_images()` captures the current application loader; initial watched
  misses return a pending error and are prepared in the background. The existing
  last-good-frame path bridges the wait; there is no new placeholder UI.
- `wait_initial` reports targets known at startup, once. The caller owns the
  guard; it stops without waiting for worker queue capacity, and joins in-flight
  synchronous loads. Application failure cancels the service too.
- Both examples enable hot reload by default and accept `--no-hot-reload` to
  disable it. Embedded catalog baselines and headless
  export remain unchanged. Icons already use resource paths and update through
  the same image service.
- Automated tests cover native create/write/atomic rename/directory move,
  layered overrides/removal, missing paths, malformed catalog recovery,
  background thread affinity, two windows, hidden-window titles/icons,
  immutable outputs, unchanged image identity, stale revisions/session/loader,
  edits during parsing, queue-full shutdown, worker failure, runtime-disabled
  behavior, target validation, watch-owned pixel release, scoped hints,
  rescan reconciliation, and deterministic bounded debounce/retry scheduling.
- `./n check` passed all seven tasks during implementation. API setup examples
  are compiled as documentation tests. The user confirmed it works on
  2026-10-07 after rebooting to resolve an NVIDIA driver/library mismatch.
- See
  [DR-015](../../decisions/DR-015%20Reload%20resources%20through%20a%20shared%20background%20pipeline.md)
  for the adopted decisions.

## Assumptions and limits

- Implemented defaults: 200 ms quiet period, one loader thread, explicit catalog
  registration, retain last good values on complete deletion.
- Watch roots must exist at startup. Creating new files/subdirectories under
  them is supported; deleting/replacing a watched root itself reports a backend
  failure and requires restarting the session initially.
- Native filesystem mechanisms are required. Network mounts or environments
  without useful native events are not guaranteed; report backend limitations
  rather than silently enabling polling.
- Directory symlink aliases outside the watched tree are not followed for
  additional watches. Loading retains the existing containment contract.
- Only filesystem images and catalogs are reloaded. Fonts, live UI definitions,
  embedded assets, and network invalidation are outside this first change.
- Pinning watched image snapshots trades memory for reliable reuse. Watch only
  requested paths, expose retained bytes in diagnostics if useful, and avoid
  eager decoding of every image in a directory.

## Sources

- [notify documentation](https://docs.rs/notify/latest/notify/) describes native
  recommended watchers, recursive watches, backend errors, and filesystem
  limitations.
