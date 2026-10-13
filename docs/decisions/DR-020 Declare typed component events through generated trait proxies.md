# DR-020: Declare typed component events through generated trait proxies

- Status: Accepted
- Date: 2026-10-13

## Decision

Give each component an associated `Events` interface alongside `Props` and
`State`. Define events as methods on an object-safe trait and use
`#[component_events]` to generate worker-local proxies, reflected parameter
payloads and indexed metadata. Methods take `&self`, owned reflected sendable
parameters and return `PixuiResult<()>`.

Keep callback bindings separate from component props and painters. In `.pixui`,
use `@event=action(field: expression)` with named request arguments. Event
method parameters are implicit callback locals, also available through
`event.field`. Containers establish inherited slice scopes; explicit
`slice.action(...)` calls override the nearest scope. Compile against an
explicit action export allowlist.

Capture application values and generational references during successful UI
preparation. Evaluate payload-dependent work at emission. Queue emissions
locally and dispatch their actions in order after the input hook returns,
stopping at failure. Retain published bindings for compatible visual redraws.

## Rationale

Trait methods let component authors emit events using ordinary Rust calls with
named parameters and compiler-checked types. Reflection exposes the same payload
shape to the UI language, where request names improve clarity and survive
argument reordering. Registration and compilation resolve names once; actual
emission and dispatch use indices and handles.

Shared event proxies make emission convenient without requiring a mutable
context. A short worker-local `RefCell` borrow needs no synchronization
primitive. Deferring actions prevents reentrancy while component state is
borrowed. Snapshot semantics preserve the relationship between input and the UI
actually presented.

Container scopes associate a subtree with its application's slice without
binding reusable component types to an application domain. Explicit exports
preserve the language's capability boundary; inheritance changes lookup, not
authorization.

## Context

Activation and controlled text changes previously had independent factories and
runtime storage. The source language supported only activation and positional
calls under a global `actions` namespace. Custom component events and text input
callbacks needed a common mechanism, while preserving per-window geometry,
shared interaction state, revision checks and controlled editing behavior.

Application state already lives on one worker. Painters define appearance and
must remain independent of input behavior and action dispatch.

## Consequences

- Custom components can expose events without adding special engine or parser
  cases for each event name or payload shape.
- A proc-macro crate and generated adapters are required. Rust requires macro
  entry points at its crate root; those functions delegate to named modules.
- Event interfaces initially exclude borrowed parameters, async/generic methods,
  default bodies, supertraits and callback return values beyond emission errors.
- Event parameters need neither Clone nor Sync merely for emission or field
  inspection. Captured custom application roots use explicit ownership adapters,
  which currently require Clone, Send and Sync.
- Event parameters shadow enclosing loop locals. Authors rename a loop variable
  when both values are needed; no callback parameter-renaming syntax is
  provided.
- Existing `.pixui` callbacks migrate to `@event`, named arguments and slice
  lookup. Rust activation/change helpers delegate to common event storage.
- Pointer gestures require a matching press/release target. Direct targeting is
  implemented; ancestor propagation and general drag capture remain future work.
- Earlier actions and hook state mutations are not rolled back if later work
  fails. Source callback/action errors retain source origins and event names.

## Considered alternatives

- **An associated event enum:** simple metadata and dispatch, but explicit
  variant construction makes emission less convenient than ordinary method
  calls.
- **Separate typed event constants:** preserve types but duplicate event lists
  and payload declarations. Generated keys remain useful for Rust bindings.
- **More special callback fields:** extend activation/change with less initial
  machinery, but repeat ownership, preparation, language checking and dispatch
  for each new event.
- **Untyped callback maps:** flexible but move payload errors to runtime and
  reduce compiler diagnostics. Use strings at lookup boundaries only.
- **Mutable event receivers:** avoid interior mutability, but complicate
  emission through shared contexts. Short local queue borrows make `&self`
  practical.
- **Callbacks in props:** mix executable behavior into reflected data
  constructors and obscure ownership. Keep props authoritative data inputs.
- **Global action lookup:** loses slice ownership and makes names ambiguous as
  applications grow. Use explicit qualification and inherited container scopes.
- **Live application reads on emission:** could target data from a different
  tree than the displayed control. Retain published captures and revalidate
  opaque refs.
- **Immediate action execution or DOM-style propagation:** introduce reentrancy
  or additional ordering/cancellation policy. Start with queued direct
  targeting.
