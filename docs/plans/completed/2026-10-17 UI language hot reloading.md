# UI language hot reloading

Status: complete; automated checks and native verification passed.

## Goal

Reload `.pixui` entry definitions and reusable composite components through the
existing optional resource hot reload service. Read, lex, parse, resolve, type
check and lower the entire affected source bundle on the resource loader
thread. Send checked results to the application worker for atomic integration.
Keep the last working UI when a save is incomplete or invalid.

Hot reload remains runtime opt-in, disabled by default in the engine, enabled
by default in the examples and disabled there with `--no-hot-reload`. It is not
a Cargo feature. Rust component, painter and application code are not reloaded.

## Current constraints

- `resources/reload` already provides native directory watches, a quiet period,
  bounded reads, content hashing, retries, revision checks and bounded delivery
  through the application queue. Images and PO catalogs use this pipeline.
- `ui_language/bundle::install` currently combines recursive source loading,
  compilation and application registration. Imports acquire application-owned
  `ComponentRef`s during that process; moving this function to another thread
  would incorrectly move application mutation there too.
- The template registry is append-only and limited to 4096 entries. Reloading
  by repeatedly registering new templates would leak slots and leave references
  stored in navigation/application data pointing at old definitions.
- Static composite calls hold checked definition snapshots. Dynamic calls and
  projected slots retain mounted template clones. `invalidate_all()` only marks
  instances dirty; it does not discard these templates or retained component
  state. In-place replacement alone will not refresh them reliably.

## Proposed design

### Watched bundle registration

Register one reload target per source bundle, with a resource filesystem, entry
module paths, translation domain, `BundleLimits`, host `Environment` and its
installed bundle handle. Watch all filesystem layers using the same roots and
containment rules as images/catalogs. Keep canonical bundle-relative identities
and resolve `use` paths relative to the importing module.

Expose this through `ResourceReloadBuilder` rather than a second watcher or
thread. A bundle is the atomic reload unit: rebuild all its reachable entry and
library modules when any dependency changes. This handles shared imports,
changed prop signatures, private declarations and removed dependencies without
incremental compiler machinery. Several entries can share one bundle.

Keep a manifest of successful module paths and paths attempted during failed
loads. Watch roots before initial reads. Edits, creates, deletes, directory
changes, atomic editor renames and rescan hints must schedule a new attempt.
Creating a previously missing import must recover without restarting. Unknown
files do not become new entry modules automatically. New imports resolve through
the original filesystem, including layer overrides and containment checks.

Retain embedded `SourceBundle` loading for production and extraction. Add a
filesystem-backed source resolver for development; do not read source files on
the worker or from a painter. Initial watched loading should use the same
preparation/integration stages and participate in `wait_initial()`.

### Detached compilation

Split bundle installation into background preparation and worker integration:

1. Capture an immutable compilation environment on the worker. Include the
   application identity and a configuration revision for relevant host exports,
   component/event schemas, styles and externally referenced templates. Do not
   copy live application values, mutable registries or retained UI state.
2. The loader reads a bounded immutable source snapshot, discovers dependencies
   and compiles the complete graph against this environment. Preserve existing
   cycle/domain checks, module/depth/byte/declaration limits and source spans.
3. Produce an owned, `Send` checked bundle with declarations, dependency hashes,
   diagnostics/source snapshots and relocations for bundle-local references.
   Internal imports use symbolic module/export identities until integration;
   never manufacture usable references owned by a temporary application.
4. The worker links these declarations to stable application references,
   registers/reuses translation indices and commits the candidate.

Refactor synchronous `install_bundle` to share these stages so compiler behavior
cannot diverge. Checking must not call host action handlers, evaluate live
expressions, create component state or run painters. Confirm the prepared
representation's thread bounds with compile-time assertions.

Application-specific ownership/link checks may remain on the worker. All source
reading and language compilation must finish on the loader. If final i18n or
style integration contains substantial compilation work, prepare immutable
artifacts against a revisioned snapshot off-thread; do not hide another full
compile in the commit step.

### Scheduling and stale results

Reuse the coordinator's quiet period, retry policy, one outstanding delivery,
accepted-hash acknowledgement and shutdown behavior. Generalize its current
single-path jobs to bundle dependency sets instead of watching every module as
an independently installable resource.

Hash canonical module identities and contents as one manifest. Recheck stamps
and observed notifications across every dependency before delivery. A new hint
supersedes the bundle result, including while it waits for queue capacity.
As with existing resources, this detects observed changes but cannot promise a
filesystem transaction across several files.

Check session identity, bundle identity, target version, base installation
revision and compilation-environment revision again on the worker. Stale results
must not mutate registries, clear state or acknowledge successful bytes. Refresh
the environment snapshot and schedule another background preparation when it
changes; do not recompile on the worker. Report failed attempts with retained,
source-located diagnostics and clear the error after recovery.

### Atomic integration and stable references

Use stable keys `(bundle, canonical module, ui entry or exported name)`.
Preserve `ComponentRef`s for surviving public declarations and `UiDefinitionId`s
for bound roots. Add a revision to replaceable registry entries, separate from
their identity. Update the entire graph together, including embedded static
calls; swapping only a library entry will not update its importers' captured
bodies.

Prepare candidate template entries, export metadata and translation changes
before swapping. Failure leaves templates, translations, names, exports and
mounted UI state unchanged. Domain identity is fixed for a watched bundle.
Reject an incompatible exported prop/slot signature in this first version;
internal/private signatures may change when the whole graph checks together.

Removed public exports become unavailable tombstones. Never reuse their slots
for different declarations. Existing references fail clearly if selected rather
than resolving to another component. Reintroducing the same export restores its
stable slot. Repeated edits of the same declarations must not grow registry
slots. Document limits on genuinely new export names and retained append-only
i18n messages; changed messages may add entries, identical messages must reuse
indices. Continue using installed catalogs for newly registered message keys.

Root UIs should use an explicit binding to a bundle entry or a small stable
`Dynamic` wrapper. A host `clone_template()` is an independent frozen copy and
must not silently acquire live-update semantics. Migrate the showcase's host
composition around cloned templates to wrappers around stable references.

### Clear stale physical and interaction state

For the first version, every accepted UI bundle replacement resets
**all UI definitions and instances in the application**. This is deliberately
conservative: Dynamic references can come from application data, and
affected-instance tracking is not currently available. Image/PO-only reloads
keep their existing behavior.

Provide one worker-owned reset operation, used only after a successful commit:

- Replace each instance's `LiveState` with a fresh root `PartState::Unknown`.
  Discard mounted dynamic/invocation/slot templates, loop item state and match
  state so the next walk instantiates the new definitions through `Default`.
- Drop retained Taffy/layout caches, component paths/indices, hit regions,
  prepared event closures/props and text-input geometry. Mark geometry stale
  and force full preparation rather than a visual-only redraw.
- Reset definition-wide focus, hover and scrolling, focus/hover source maps,
  pointer presses/capture, editing/IME sessions and pending clipboard requests.
  Notify native clients to end obsolete text-input sessions. Late clipboard or
  IME replies must never reach newly created components.
- Keep window/definition identities, channels, presentation settings, size,
  language/theme, visibility and native-focus facts. Preserve the diagnostics
  toggle, but discard old preparation/animation scheduling state and timings.
- Keep render revisions monotonic. Establish a fresh compatibility boundary;
  old clicks, focus targets, text-session tokens and animation acknowledgements
  must not become valid because a new component occupies an old index/path.
- Invalidate native metadata and schedule visible instances for full rendering.
  Hidden instances reset immediately but prepare only when shown again.

Application slices, collections, named entities, registered actions and their
values survive. Actions already accepted before the swap are not rolled back;
queued UI events targeting pre-swap geometry are rejected. Old render outputs
remain immutable and may stay displayed until a replacement renders. A runtime
expression failure keeps the last published output while recording the error;
its old hit targets remain disabled. Compile success cannot guarantee that
runtime data-dependent evaluation succeeds.

## Implementation checklist

- [x] Separate detached bundle preparation from atomic application integration;
  preserve the synchronous API and existing compiler diagnostics/tests.
- [x] Add stable bundle registrations, replacement revisions, root bindings,
  export signature checks and tombstones without per-edit registry growth.
- [x] Add filesystem source snapshots and dependency manifests, sharing resource
  reads, limits, watches, quiet periods, retries and bounded delivery.
- [x] Extend reload builder/session/targets for bundles and environment refresh;
  reject obsolete session, configuration and compilation results atomically.
- [x] Implement the centralized UI state/cache/interaction reset and fresh
  revision boundary, including hidden windows and native text-input shutdown.
- [x] Enable watched `.pixui` bundles in the showcase and todo where applicable;
  preserve embedded sources for disabled reload and translation extraction.
- [x] Update architecture documentation, language/resource reload documentation
  and example instructions, including reset behavior and failure recovery.
- [x] Add the verification below and run `./n check` after each unit of work.
- [x] Verify native behavior in two windows before moving the plan to completed.

## Verification

- Pure graph compilation runs on the loader, with no worker filesystem reads or
  compiler invocation. Keep the application responsive during a blocked load.
- Reload entry bodies, exported/private composites, nested imports, props,
  child slots, event bindings and bare prop expressions. A shared library edit
  updates every importer and both visible instances atomically.
- Exercise quiet-period/coalescing, identical bytes, new/missing imports,
  deletion/recreation, renamed saves, layered overrides, limits and malformed
  UTF-8/source. Failures preserve the last good graph and recover after fixes.
- Delay compilation and delivery; edit again, replace configuration, restart or
  stop the session. Obsolete results never install or clear current state.
- Preserve stored navigation `ComponentRef`s across many reloads; assert stable
  slot counts, correct tombstones/reintroduction and no half-linked imports or
  partial translation registration after failure.
- Change loop/match structure and component types at the same structural path.
  Assert new default state, new mounted bodies and no stale layout or callbacks.
- Reload while focused, selecting text, dragging, awaiting paste/IME, pressed
  between mouse transitions, animating, or hidden. Old events cannot activate
  replacement controls; fresh input works after publication/show.
- Preserve model entities and window settings; keep old render resources usable.
  A runtime evaluation failure keeps the previous frame without routing input
  through obsolete geometry. UI edits and PO/image reloads work in one session.
- Native showcase: edit an imported wrapper, button label/style and loop body;
  check both windows, resize, keyboard navigation, text editing, hidden/show and
  syntax-error recovery. `--no-hot-reload` and headless extraction remain
  usable.

## Assumptions and open decisions

- Recommended reset policy is correctness first: reset all UI state on an
  accepted source change. Preserving unaffected subtrees, focus/selection or
  scroll requires dependency tracking and explicit migration rules; defer it.
- Recommended compatibility policy rejects changing/removing configured entry
  bindings and changing existing public prop/slot signatures. Other export
  removal uses tombstones. Confirm whether a development-only breaking-reload
  mode is desirable later.
- Bundle-to-bundle source imports are out of scope initially. Host references
  into other bundles need configuration revision checks; replacing one must
  invalidate any compilation snapshot depending on it.
- Initial installation failure should use existing startup/error reporting and
  keep the watch alive for recovery. An application with no valid UI must not
  invent an empty successfully installed graph.
- Diagnostics use existing tracing/source reports initially. An in-window error
  panel and a user-facing “UI state reset” indicator are optional later work.

## Implementation notes

- Plan committed as `c484ca9` before implementation.
- Detached graphs use private owner-zero relocation symbols; no temporary
  application is constructed. Loader-side i18n registration uses a revisioned
  translation snapshot; worker acceptance swaps it only if configuration still
  matches. Normal bundle installation shares the preparation/link stages.
- The first compilation/reload pipeline unit passed `./n check`.
- Todo currently defines its UI entirely in Rust, so it has no `.pixui` bundle
  to watch. Its image/catalog reload remains enabled; source reload is exercised
  in the showcase rather than migrating the todo UI as unrelated work.
- The showcase installs its shell and pages as one bundle with module IDs rooted
  at `examples/showcase/ui`. The host-only dynamic style example wraps a live
  reference instead of editing a cloned page. Disabled reload/extraction use
  embedded source snapshots. Initial disk errors retain that baseline and guard.
- Registry slots are stable and bundle revisions track whole-graph replacement;
  per-entry revisions are unnecessary with the conservative global UI reset.
  Focus handles additionally carry a definition epoch. Animation request
  counters are retained as monotonic watermarks while prior scheduling output is
  discarded.
- Reload environments retain the explicit startup allowlist. Configuration
  stamps cover template, style, host-registration and translation revisions.
  Candidate i18n work happens on the loader, with stale snapshots rejected
  before mutation.
- Native two-window verification passed, confirmed by the user on 2026-10-17.
- Automated verification: detached preparation is `Send`; a lowering hook
  records execution on the background thread. Unit tests cover signature/root
  rejection, stable references, tombstone restoration, constant slot counts and
  obsolete configuration/base revisions without partial mutations.
- Native-watch integration tests cover imported graph updates in two instances,
  hidden-instance Unknown state, stale focus/input/IME/paste, retained model
  data, invalid-source recovery, creation of imports, runtime-render
  failure/recovery, unchanged saves, initial failure recovery, layer overrides,
  atomic renames and malformed UTF-8. UI, PO and image jobs also run together in
  one session.
- Blocking filesystem tests confirm worker actions/inspection remain responsive.
  Snapshot requests use nonblocking admission and cancellable reply polling;
  stopping a source reload does not wait for a busy worker or queue capacity.
- `./n check` passed after pipeline, showcase/reset and final lifecycle/test
  units. All implementation and verification checklist items are complete.
