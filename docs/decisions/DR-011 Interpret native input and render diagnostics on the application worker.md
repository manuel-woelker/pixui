# DR-011: Interpret native input and render diagnostics on the application worker

- Status: Accepted
- Date: 2026-10-06

## Decision

Forward low-level native mouse and keyboard input as owned, backend-independent
`UiInput` values through the application queue. Interpret activation,
navigation, scrolling and application shortcuts on the worker. This supersedes
the semantic input boundary in
[DR-004](<DR-004 Render UI instances on the worker and present display lists on the GUI thread.md>).

Render the per-window F11 performance overlay on the worker as part of its
complete display list. The host reports successful presentation timestamps and
renderer CPU timings, and executes outputs without adding drawing commands.

## Rationale

Input policy belongs beside shared interaction state and action dispatch.
Forwarding releases, repeats, arbitrary keys, modifiers and IME events preserves
information required by future controls and keeps native hosts consistent.
Headless clients can exercise the same policies and diagnostics.

The overlay uses the existing worker font and display-list infrastructure.
Caching the unadorned application output allows a worker timer to update visible
diagnostics without repainting application components. Separate output and
painting revisions preserve geometry validation and FPS accounting.

## Context

The GUI host previously translated a subset of keys and mouse releases into
semantic commands, losing other transitions before they reached the worker.
It also prepared the diagnostic font and appended the overlay locally, giving
it drawing responsibilities beyond native presentation.

Renderer timings and successful presentation are known only on the GUI thread.
Diagnostics remain local to each window, while component focus, hover and scroll
remain shared by definition as established in
[DR-010](<DR-010 Share interaction state across windows of a UI definition.md>).

## Consequences

- Native hosts convert coordinates and event representations; the application
  determines their behavior. Unhandled events currently have no default effect.
- Input remains revision-aware when interpreting it requires displayed geometry.
  Diagnostics and ignored raw inputs do not require compatible geometry.
- Presentation telemetry adds queue traffic and appears in a subsequent output.
  Renderer durations measure CPU work, not GPU execution or compositor latency.
- The worker caches one unadorned output per instance. Command storage is
  copied; immutable image and font payloads remain shared through resource
  handles.
- Diagnostic refreshes pause for hidden windows, preserve application painting
  revisions, and leave native animation handshakes and painter deadlines intact.
- Key names use the documented winit vocabulary without introducing a native
  window dependency in the engine. Unidentified native codes are opaque strings.
- Component event propagation, capture, text editing and configurable shortcut
  bindings are future work; forwarding their input does not implement them.

## Considered alternatives

### Retain semantic translation and diagnostic drawing in the host

Rejected because input policy would stay split across threads, other hosts would
need to duplicate it, and headless behavior would omit diagnostics.

### Expose winit event types directly in the engine

Rejected because native window dependencies and platform-specific event details
would become part of the application API. Small owned input types preserve the
current information needed without tying the worker to native event lifetimes.

### Repaint all components for diagnostic refreshes

Rejected because an idle UI would run preparation and painting four times per
second merely to update its FPS and timing text.

### Treat every diagnostic output as an application frame

Rejected because it would inflate FPS and restart delayed painter deadlines.
A separate painting revision identifies the underlying application pass.
