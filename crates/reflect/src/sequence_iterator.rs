use crate::DynamicObject;

/// A lazy iterator borrowing reflected sequence elements in sequence order.
/// Elements are shared views, even when the source permits mutation. The iterator
/// and its elements cannot outlive the source borrow. Creating it boxes only the
/// iterator; it does not collect or clone the underlying elements.
pub type SequenceIter<'a> = Box<dyn Iterator<Item = DynamicObject<'a>> + 'a>;
