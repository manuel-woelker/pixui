//! Immutable shared snapshots. Identity is allocation identity, never content.

use std::{
    fmt,
    hash::{Hash, Hasher},
    ops::Deref,
    sync::{Arc, Weak},
};

/// Cheaply cloneable ownership of one immutable version. Constructing another
/// resource creates a new identity even when its contents are equal. Send/Sync
/// follow T; no mutable access to the allocation is exposed. Payloads used as
/// snapshots must not mutate through interior mutability; the generic wrapper
/// cannot enforce that property of T.
pub struct Resource<T>(Arc<T>);

impl<T> Resource<T> {
    /// Takes ownership without cloning the payload. Named separately so domain
    /// aliases can provide validated `new` constructors with their own arguments.
    pub fn from_value(value: T) -> Self {
        Self(Arc::new(value))
    }
    /// Retains a still-live snapshot observed through `downgrade`. Returns None
    /// after its last owning handle is dropped; never constructs a new version.
    pub fn upgrade(weak: &Weak<T>) -> Option<Self> {
        weak.upgrade().map(Self)
    }
    pub fn identity(&self) -> ResourceIdentity<T> {
        ResourceIdentity {
            address: Arc::as_ptr(&self.0) as usize,
            marker: std::marker::PhantomData,
        }
    }
    pub fn ptr_eq(left: &Self, right: &Self) -> bool {
        Arc::ptr_eq(&left.0, &right.0)
    }
    /// Observe release without retaining the snapshot. Upgrading keeps the
    /// payload alive; a weak reference never extends its lifetime by itself.
    pub fn downgrade(&self) -> Weak<T> {
        Arc::downgrade(&self.0)
    }
}
impl<T> Clone for Resource<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T> Deref for Resource<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}
impl<T> PartialEq for Resource<T> {
    fn eq(&self, other: &Self) -> bool {
        Self::ptr_eq(self, other)
    }
}
impl<T> Eq for Resource<T> {}
impl<T: fmt::Debug> fmt::Debug for Resource<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Lookup token only. It does not retain ownership: an address can be reused
/// after its allocation is freed. Keep an owning Resource or its Weak alongside
/// every cached key. A Weak reserves the allocation address; check upgrade before
/// reuse and evict dead entries without extending snapshot payload lifetime.
pub struct ResourceIdentity<T> {
    address: usize,
    marker: std::marker::PhantomData<fn() -> T>,
}
impl<T> Copy for ResourceIdentity<T> {}
impl<T> Clone for ResourceIdentity<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> PartialEq for ResourceIdentity<T> {
    fn eq(&self, other: &Self) -> bool {
        self.address == other.address
    }
}
impl<T> Eq for ResourceIdentity<T> {}
impl<T> Hash for ResourceIdentity<T> {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.address.hash(hasher);
    }
}
