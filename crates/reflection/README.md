# pixui-reflection

Small, explicit reflection for reading struct fields and invoking methods.
A non-generic `TypeDescriptor` holds immutable lists of erased field getters
and method adapters, plus the receiver’s Rust `TypeId` and diagnostic type name. Automatic registration uses a module attribute, without a global registry.

```rust
use pixui_reflection::{reflect, Reflect, DynamicObject};

#[reflect]
mod model {
    pub struct Counter { pub value: i32 }

    // Ordinary Rust methods: no registration lists or adapters.
    impl Counter {
        pub fn add(&mut self, amount: i32) -> i32 {
            self.value += amount;
            self.value
        }
    }
}

let descriptor = model::Counter::type_descriptor();
let mut object = DynamicObject::from_reflect(model::Counter { value: 10 });

// Resolve once, then reuse indices on objects with this descriptor.
let field = descriptor.field_index("value")?;
let method = descriptor.method_index("add")?;
let result = object.invoke(method, &[&5_i32])?;
assert_eq!(*result.downcast::<i32>().unwrap(), 15);
assert_eq!(object.read(field)?.downcast_ref::<i32>(), Some(&15));

object.invoke_named("add", &[&2_i32])?;
assert_eq!(object.read_named("value")?.downcast_ref::<i32>(), Some(&17));
# Ok::<(), pixui_base::PixuiError>(())
```

## Automatic registration

Apply `#[reflect]` to an inline module containing concrete named or unit structs
and their ordinary inherent `impl` blocks. The macro discovers every field and
receiver method, including private members. Reflection therefore explicitly
exposes those members to callers. Fields follow declaration order; methods
follow impl block and method source order. Multiple impl blocks are supported.
Associated functions (including constructors) and trait impls are ignored.
`cfg` conditions on structs, fields, impls, and methods are preserved.

Each struct implements `Reflect`. Import that trait to call
`MyStruct::type_descriptor()`, which returns an `Arc<TypeDescriptor>` cached
with `OnceLock`. `DynamicObject::from_reflect(value)` erases the value and obtains
its descriptor automatically. No member lists or adapters are necessary.

The module attribute is intentional: a derive on a struct cannot inspect
separate impl blocks. Keep reflected impl blocks directly in that same module,
using the struct's unqualified name (`impl Counter`). Methods defined elsewhere
are not discovered. Nested modules need their own attribute. The dependency
must be available under the canonical `pixui_reflection` crate name.

Supported signatures are safe synchronous non-generic methods with `&self` or
`&mut self`. Owned arguments must implement `Clone` because the invocation API
borrows its input values. Shared `&T` arguments borrow a concrete `T` from the
argument list; `&str` specifically borrows a `String`. All argument checks and
clones run before the method is called. Mutable references, explicit argument
lifetimes, generic structs/impls/methods, tuple structs, consuming receivers,
async methods, and unsafe methods are unsupported. Unsupported signatures get
macro diagnostics; non-`Clone` arguments and non-`'static` return types get Rust
compiler errors. Return values must be owned and `'static`.

The method's entire return value is boxed, including `Result<T, E>`. An
application error is therefore an ordinary reflected return value, while
`PixuiResult` from invocation represents dispatch or argument errors. Methods
without a return value produce boxed `()`.

## Manual registration and erased objects

The existing `type_descriptor!` macro and `TypeDescriptor::new::<T>`,
`Field::new::<T>`, and `Method::new::<T>` constructors remain available for
custom adapters. Manual registration is optional.

The descriptor accepts `&dyn Any` for reads and `&mut dyn Any` for calls, checking
the receiver type before dispatch. Each erased adapter also checks its receiver
when downcasting. Mismatched manual registrations return errors when accessed.
There are no unsafe casts. Erased callbacks are boxed once during registration.

`DynamicObject` owns a `Box<dyn Any>` and an `Arc<TypeDescriptor>`. Its manual
constructor rejects a mismatched value and descriptor. Different concrete types
can coexist in `Vec<DynamicObject>` and use the same API. `descriptor()` exposes
metadata; `downcast_ref` and `downcast_mut` optionally recover concrete access.
Reading a field borrows the object, so mutable invocation cannot overlap that
borrow. Objects need not be `Send` or `Sync`; descriptors can be shared across
threads.

## Mechanism and assumptions

- Registration order defines zero-based indices. Field and method indices
  have distinct types. Keep cached indices with their descriptor; indices
  are not stable across descriptor changes or suitable for persistence.
  An in-range index from another descriptor silently selects that position.
- Name lookup is linear and exact, including case. Indexed dispatch checks
  bounds and directly calls a function pointer. Construction rejects empty
  and duplicate names within each namespace. Fields and methods may share names.
- Getters borrow the original field as `&dyn Any`, without cloning. The result
  lives as long as the receiver borrow, preventing mutation while it is used.
  Exposed field types must be `'static`, as required by `Any`; they cannot
  contain non-static references. The receiver must also be `'static` for erasure, but need not be `Clone` or `Send`.
- Arguments are borrowed `&dyn Any` values. Adapters validate exact types with
  `argument`, then copy or clone values when their Rust method needs ownership.
  No coercions occur: `String`, `&str`, and numeric types remain distinct.
- Arity is checked before calling the adapter. Return values are owned
  `Box<dyn Any>` and must have `'static` types. Return `Box::new(())` for void
  methods. Borrowed return values and consuming receivers are unsupported.
- Errors use `PixuiResult` and adapters can return application errors unchanged.
  Validate all arguments before mutation. Errors do not roll back mutations;
  panics propagate. Registration callbacks are trusted to expose the intended
  members and behavior; the library does not inspect actual struct definitions.
- Only registered fields and methods exist in the reflection API. Methods
  use a typed `&mut T` adapter behind an erased `&mut dyn Any` callback, even if their underlying Rust method needs only `&T`.
  Field writes, overloads are outside the current API.

Register descriptors once and reuse them. For frequent access, cache indices
rather than repeating name lookup. Use `#[reflect]` for ordinary structs and methods; manual adapters remain useful
for custom operations.
