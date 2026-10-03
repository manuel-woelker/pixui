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
`MyStruct::type_descriptor()`, which returns a `&'static TypeDescriptor` stored
in a `OnceLock<TypeDescriptor>`. `DynamicObject::from_reflect(value)` erases the value and obtains
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
compiler errors. Owned return values must have `'static` types. Shared `&T`
returns from `&self` and mutable `&mut T` returns from `&mut self` are also
supported when `T: Reflect`. Return references must use elided receiver
lifetimes; references borrowed from arguments, shared returns from `&mut self`,
and unsized return targets such as `str` and slices are unsupported.

An owned method's entire return value is boxed, including `Result<T, E>`. An
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

`DynamicObject<'a>` stores an owned `Box<dyn Any>`, a shared `&'a dyn Any`,
or an exclusive `&'a mut dyn Any`, alongside a `&'static TypeDescriptor`. Its manual
constructor requires a descriptor in static storage and rejects mismatched types.
For manual registration, initialize a static `OnceLock<TypeDescriptor>` and
pass the reference returned by `get_or_init`. Generated descriptors initialize
once and live for the process lifetime without `Arc` or explicit leaking. Different
concrete types and storage variants can coexist in `Vec<DynamicObject<'a>>`,
using a common borrow lifetime. `descriptor()` exposes
metadata; `downcast_ref` and `downcast_mut` optionally recover concrete access.
Reading a field borrows the object, so mutable invocation cannot overlap that
borrow. Objects need not be `Send` or `Sync`; descriptors can be shared across
threads.

## Owned and borrowed storage

`from_reflect` and `new` return `DynamicObject<'static>`: no receiver borrows
are stored. This does not make the value immortal; dropping the object drops
its owned value normally. `from_ref` / `from_mut` borrow reflected values;
`borrow` / `borrow_mut` do the same with explicit descriptors. Dropping a
borrowed wrapper never drops its underlying value. The concrete value's type
must satisfy `Any` (`'static`), but the wrapper's borrow may be short-lived.

All variants support field reads and shared method calls. `is_owned()` reports
ownership and `is_mutable()` reports write capability. `downcast_mut()` returns
`None` for shared storage. Even `&mut DynamicObject` cannot turn a shared
underlying reference into a mutable one.

| Invocation | Result | Receiver capability |
|---|---|---|
| `invoke` | Owned `Box<dyn Any>` | Either; mutable methods require write access |
| `invoke_shared` | Owned `Box<dyn Any>` | Shared |
| `invoke_ref` | Shared `DynamicObject<'_>` | Shared |
| `invoke_mut` | Mutable `DynamicObject<'_>` | Mutable |

Each has a corresponding `*_named` convenience method. Index lookup is shared
across all paths. Calling the wrong return path, or requesting mutable access
through shared storage, returns an error before the method runs. Manual adapters
can use `Method::shared`, `Method::returning_ref`, and `Method::returning_mut`.

```rust
use pixui_reflection::{reflect, DynamicObject};
#[reflect]
mod model {
    pub struct Child { pub value: i32 }
    impl Child {
        pub fn increment(&mut self) { self.value += 1; }
        pub fn value(&self) -> i32 { self.value }
    }
    pub struct Parent { pub child: Child }
    impl Parent {
        pub fn child(&self) -> &Child { &self.child }
        pub fn child_mut(&mut self) -> &mut Child { &mut self.child }
    }
}
let mut parent: DynamicObject<'static> = DynamicObject::from_reflect(
    model::Parent { child: model::Child { value: 7 } }
);
{
    let child = parent.invoke_ref_named("child", &[])?;
    assert!(!child.is_mutable());
    assert_eq!(*child.invoke_shared_named("value", &[])?.downcast::<i32>().unwrap(), 7);
}
{
    let mut child = parent.invoke_mut_named("child_mut", &[])?;
    child.invoke_named("increment", &[])?;
}
assert_eq!(parent.downcast_ref::<model::Parent>().unwrap().child.value, 8);
# Ok::<(), pixui_base::PixuiError>(())
```

Returned objects borrow the receiver, not the argument list. Rust prevents
dropping or mutably accessing the receiver while a borrowed result is used:

```compile_fail
use pixui_reflection::{reflect, DynamicObject};
#[reflect]
mod model {
    pub struct Child;
    pub struct Parent { pub child: Child }
    impl Parent { pub fn child(&self) -> &Child { &self.child } }
}
let parent = DynamicObject::from_reflect(model::Parent { child: model::Child });
let child = parent.invoke_ref_named("child", &[]).unwrap();
drop(parent); // Cannot move the receiver while its result is borrowed.
assert!(!child.is_mutable());
```

```compile_fail
use pixui_reflection::{reflect, DynamicObject};
#[reflect]
mod model {
    pub struct Child;
    pub struct Parent { pub child: Child }
    impl Parent {
        pub fn child(&self) -> &Child { &self.child }
        pub fn child_mut(&mut self) -> &mut Child { &mut self.child }
    }
}
let mut parent = DynamicObject::from_reflect(model::Parent { child: model::Child });
let shared = parent.invoke_ref_named("child", &[]).unwrap();
let mutable = parent.invoke_mut_named("child_mut", &[]).unwrap();
assert!(!shared.is_mutable()); // Shared and exclusive borrows cannot overlap.
```

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
  methods. Borrowed reflected returns use separate invocation paths; consuming
  receivers are unsupported.
- Errors use `PixuiResult` and adapters can return application errors unchanged.
  Validate all arguments before mutation. Errors do not roll back mutations;
  panics propagate. Registration callbacks are trusted to expose the intended
  members and behavior; the library does not inspect actual struct definitions.
- Only registered fields and methods exist in the reflection API. Shared methods
  use `&T` adapters; mutable methods use `&mut T` adapters.
  Direct field writes and overloads are outside the current API.

Register descriptors once and reuse them. For frequent access, cache indices
rather than repeating name lookup. Use `#[reflect]` for ordinary structs and methods; manual adapters remain useful
for custom operations.
