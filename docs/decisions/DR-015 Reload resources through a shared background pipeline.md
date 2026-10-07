# DR-015: Reload resources through a shared background pipeline

- Status: Accepted
- Date: 2026-10-07

## Decision

Provide optional runtime hot reload for directory-backed images and translation
catalogs. It is disabled by default, available in ordinary builds, and enabled
only by starting a caller-owned reload session. The examples start sessions by
default for development convenience and expose `--no-hot-reload` to disable
them. Use native recursive directory notifications, a per-resource quiet period,
and one shared background loader. Send complete prepared resources to the
existing application worker for checked replacement and UI/native metadata
invalidation.

Share scheduling, retries, stale-result handling, and bounded delivery. Keep
image decoding and catalog import as concrete jobs. Install catalogs using the
existing registry-dependent transactional validation. Preserve immutable image
versions and last-good resources after failure or complete deletion.

## Context

Image lookup already supports layered filesystems and immutable shared
snapshots. Catalog installation already supports atomic domain/language
replacement. Their initial file-loading workflows differ, but editor saves,
incomplete writes, backpressure, and shutdown are common concerns. The
application owns state on one worker and GUI clients consume immutable outputs.

The desired development workflow edits images and PO files while multiple
windows are open. Production must incur no watching or loader-thread activity
unless explicitly enabled. A Cargo feature is unnecessary: runtime opt-in is
the required control, and the reload code should receive ordinary test coverage.

## Rationale

Directory notifications support newly created paths and atomic-save renames
without frame-by-frame timestamp checks. Reading through the original filesystem
preserves override priority and containment. A short quiet period coalesces
save bursts; bounded retries and last-good values handle incomplete files.

Background reads, decoding, and PO parsing avoid blocking painting or the native
event loop. Worker-side acceptance keeps mutation serialized with actions and
registration. Session identities and target revisions prevent older queued work
from undoing newer changes. Content hashes avoid unnecessary new GPU/image
identities and catalog invalidations after noisy notifications.

A caller-owned guard exposes the service lifetime and avoids changing the cheap
application handle or creating a worker/sender ownership cycle. One outstanding
prepared update bounds queue payload memory and simplifies cancellation.

## Consequences

- Normal builds include the native watcher dependency. Disabled mode creates no
  watcher, loader thread, or watch-owned pixel retention.
- Sources gain a default-empty native-root capability. Embedded/network sources
  retain their existing contracts; native watch behavior is filesystem
  dependent.
- Images and catalogs share a pipeline but retain separate validation/install
  rules. Catalog compilation still occupies bounded application-worker time.
- Requested watched image snapshots are strongly retained for the session;
  ordinary caches remain weak. The 4096-target limit is not a global pixel
  budget.
- First uncached watched images report pending through the existing error path;
  a successful background replacement triggers another preparation.
- Broad UI invalidation follows current content-update behavior, including
  clearing focus/hover. More selective invalidation can be added if measured.
- Complete deletion keeps last-good values. Removing a winning layered file
  still exposes lower layers through ordinary lookup.
- Stop/drop may wait for in-flight synchronous decoding but never for worker
  queue capacity. Guards belong outside worker operations and native callbacks.
- Replacing image configuration requires restarting the session to watch the
  new source. Explicitly retained old images remain immutable.

## Considered alternatives

### Compile reload only behind a Cargo feature

Rejected because runtime-disabled behavior is sufficient and explicitly
requested. Always compiling it keeps the path covered by normal checks.

### Poll file timestamps during rendering

Rejected because it adds recurring I/O to the render path and fails the native
filesystem-notification requirement. Polling fallback for network mounts is
outside the initial scope.

### Independent watchers for images and translations

Rejected because debounce, save races, queue limits, cancellation, and
diagnostics would be duplicated and could diverge.

### Invalidate caches and synchronously reload on the next frame

Rejected because decoding or PO parsing would stall the application worker.
Prepared replacement keeps expensive file work in the background.

### Mutate existing image pixels in place

Rejected because old outputs and GPU caches rely on immutable resource identity.
A replacement snapshot naturally creates a new cache identity.

### Unlimited parallel loads and queued payloads

Rejected because rapid edits to large images could consume unbounded memory. One
preparation/delivery pipeline is simpler; add parallelism only after profiling.

### Automatically discover catalog domains and languages

Rejected because filesystem naming should not implicitly change application
language registration or translation ownership. Register catalog targets
explicitly.
