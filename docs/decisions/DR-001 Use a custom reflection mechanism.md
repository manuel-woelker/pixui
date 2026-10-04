# DR-001: Use a custom reflection mechanism

- Status: Accepted
- Date: 2026-10-03

## Decision

Use the repository's `pixui-reflect` and `pixui-reflect-macros` crates to read
struct fields and invoke methods dynamically while retaining ordinary Rust
structs and inherent `impl` blocks as the source of truth.

The mechanism consists of:

- A non-generic `TypeDescriptor` containing field getters, method adapters,
  and receiver type information.
- Separate field and method indices. Resolve a name once, then perform
  subsequent operations through its index. String convenience methods perform
  the same lookup and indexed dispatch.
- A `#[reflect]` module attribute that discovers fields and receiver methods
  without requiring users to repeat their names or signatures in registration
  lists.
- Descriptors initialized once per reflected type with `OnceLock` and shared
  through `&'static TypeDescriptor`, without reference counting or a global
  registry.
- A single `DynamicObject<'a>` with owned, shared, or mutable storage, allowing
  callers to handle different concrete types and storage modes uniformly.
- Distinct invocation paths for owned results and shared or mutable reflected
  reference results. Borrowed results retain the receiver's borrow lifetime.

Keep the implementation focused on these operations. Add broader reflection
features only when a concrete use case justifies their complexity. The
[crate documentation](../../crates/reflect/README.md) defines the detailed API
and supported signatures.

## Rationale

Dynamic consumers need a common way to inspect and operate on values without
knowing their concrete Rust types. A descriptor makes that information explicit
while leaving application data and behavior in idiomatic Rust definitions.

Indexed dispatch separates name resolution from repeated execution. Consumers
can cache resolved indices and avoid repeating a linear string lookup for every
read or call. This is a structural benefit, not a claim that reflection is as
fast as direct Rust access; adapters still perform dynamic type checks and
indirect calls.

Automatic registration avoids a second manually maintained inventory of fields
and methods. The module attribute sees both structs and their impl blocks,
which a derive on an isolated struct cannot do. Changes to ordinary definitions
therefore feed descriptor generation directly.

Using `Any`, checked downcasts, and Rust references allows type erasure without
unsafe pointer casts. Field reads borrow their original values, and reference
results can expose nested reflected objects without cloning those objects.
The compiler continues to enforce borrow lifetimes and exclusive access.

One dynamic object type simplifies handling mixed values and returned children.
Its storage enum preserves ownership and write capability explicitly. Shared
storage rejects mutation at runtime, while Rust still prevents overlapping
exclusive borrows.

Keeping this small mechanism in the repository lets its API follow the
project's requirements for method invocation, cached indices, and borrowed
results together. It avoids committing to a broader framework before those
requirements are established.

## Context

The project needs dynamic field access and method invocation on concrete Rust
structs. The chosen API must support name lookup followed by indexed operations,
convenient string access, and erased objects that can represent owned values
or references to existing values.

Methods can return references to fields. Treating every return value as an
owned box would lose that borrowing behavior or require copying. Returned
references should remain usable through reflection and must not outlive their
receiver.

Registration should require an attribute rather than re-enumerating members
or rewriting methods into a special declaration language. Rust's `Any` enables
checked type erasure but does not itself enumerate fields or invoke arbitrary
methods, so descriptors and generated adapters provide the missing operations.

This record documents the implemented choice. No comparative evaluation of
third-party reflection libraries has been completed; the decision does not
claim that all existing libraries are unsuitable.

## Consequences

- Dynamic consumers share one API for heterogeneous values, and borrowed
  returned objects can be inspected or mutated without transferring ownership.
- Descriptors are reused for the process lifetime. Owned dynamic objects still
  drop their values normally; borrowed wrappers do not own their referents.
- We own the maintenance of the runtime API, procedural macro, diagnostics,
  documentation, and tests, including tests for borrowing and error paths.
- Dynamic operations can fail for unknown names, invalid indices, mismatched
  receiver or argument types, incorrect arity, incorrect return paths, or
  unavailable mutable access. These are runtime errors rather than statically
  checked calls.
- Cached indices belong to a descriptor and are not persistent identifiers.
  An in-range index from another descriptor cannot be detected. Source changes
  can change registration order.
- Automatic registration includes private members. Applying the attribute
  intentionally exposes them through reflection; Rust visibility alone does
  not restrict reflected access.
- Reflected impl blocks must be visible within the attributed inline module. The
  initial macro supports a deliberately limited set of concrete, synchronous
  signatures. Generics, async methods, consuming receivers, and several
  reference forms remain unsupported.
- Owned arguments are cloned by generated adapters. Shared arguments can be
  borrowed. Owned return values remain boxed `Any`; borrowed reflected returns
  require a sized target implementing `Reflect`.
- Shared and mutable storage share a public type, so write capability is checked
  at runtime. The API exposes capability queries to help dynamic callers.
- Dispatch and argument errors remain distinct from a Rust method's returned
  `Result`, which is an ordinary return value. Errors do not roll back mutation,
  and panics propagate.

## Considered alternatives

### Use only statically typed calls and application-specific traits

Rejected for dynamic consumers because every new exposed field or operation
would require a known interface or application-specific dispatch code. Direct
typed access remains appropriate where callers know the concrete type.

### Use `Any` and downcast at every call site

Rejected as the complete mechanism because callers would still need to know
concrete types and write their own field and method dispatch. `Any` remains the
foundation for checked erasure inside the chosen implementation.

### Convert values into serialized maps or a generic value tree

Rejected because a copied value representation does not preserve arbitrary
Rust method behavior or live shared and exclusive references to original fields.
It would also require conversion rules beyond the current reflection needs.

### Require manual field and method registration everywhere

Rejected as the default because registration lists duplicate definitions and
can drift as structs and methods evolve. Manual adapters remain available for
custom operations and signatures outside automatic registration.

### Adopt a third-party reflection framework immediately

Deferred because the immediate scope is small and includes a specific
combination of indexed method dispatch and borrowed reflected results. We have
not established that adopting a framework would reduce total complexity for
these requirements. Reconsider this choice if maintenance grows or an evaluated
library meets the requirements with less custom code.

### Use separate owned, shared, and mutable dynamic object types

Rejected for the public API because it complicates heterogeneous collections
and handling returned values uniformly. Separate types offer stronger static
capability checking; the chosen enum trades that benefit for one dynamic API
with runtime write checks.

### Keep descriptors in `Arc` or explicitly leak them

Rejected for generated descriptors because their lifetime is already the
process lifetime. Static `OnceLock` storage provides lazy initialization and
shared references without reference-counting overhead or explicit leaking.

### Use fully constant descriptors

Rejected for the current representation because descriptors contain vectors
and boxed adapters built during initialization. Constant descriptors would
require changing storage to static slices and suitable function pointers.
That redesign is unnecessary for the current scope.
