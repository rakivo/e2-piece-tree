// Portions of this file are copied from the `cranelift_entity` crate.
// Original code Copyright (c) The Cranelift Project Developers.
// Licensed under the Apache License, Version 2.0 with LLVM Exception.
// See: https://llvm.org/LICENSE.txt

extern crate alloc;

use alloc::vec::Vec;
use core::fmt;
use core::marker::PhantomData;
use core::ops::{Index, IndexMut};
use core::slice;
use alloc::vec;
use core::iter::Enumerate;

/// A type wrapping a small integer index should implement `EntityRef` so it can be used as the key
/// of an `SecondaryMap` or `SparseMap`.
pub trait EntityRef: Copy + Eq {
    /// Create a new entity reference from a small integer.
    /// This should crash if the requested index is not representable.
    fn new(_: usize) -> Self;

    /// Get the index that was used to create this entity reference.
    fn index(self) -> usize;
}

/// Macro which provides the common implementation of a 32-bit entity reference.
#[macro_export]
macro_rules! entity_impl {
    // Basic traits.
    ($entity:ident) => {
        impl $crate::EntityRef for $entity {
            #[inline]
            fn new(index: usize) -> Self {
                debug_assert!(index < (core::u32::MAX as usize));
                $entity(index as u32)
            }

            #[inline]
            fn index(self) -> usize {
                self.0 as usize
            }
        }

        impl $entity {
            /// Create a new instance from a `u32`.
            #[allow(dead_code, reason = "macro-generated code")]
            #[inline]
            pub fn from_u32(x: u32) -> Self {
                debug_assert!(x < core::u32::MAX);
                $entity(x)
            }

            /// Return the underlying index value as a `u32`.
            #[allow(dead_code, reason = "macro-generated code")]
            #[inline]
            pub fn as_u32(self) -> u32 {
                self.0
            }

            /// Return the raw bit encoding for this instance.
            ///
            /// __Warning__: the raw bit encoding is opaque and has no
            /// guaranteed correspondence to the entity's index. It encodes the
            /// entire state of this index value: either a valid index or an
            /// invalid-index sentinel. The value returned by this method should
            /// only be passed to `from_bits`.
            #[allow(dead_code, reason = "macro-generated code")]
            #[inline]
            pub fn as_bits(self) -> u32 {
                self.0
            }

            /// Create a new instance from the raw bit encoding.
            ///
            /// __Warning__: the raw bit encoding is opaque and has no
            /// guaranteed correspondence to the entity's index. It encodes the
            /// entire state of this index value: either a valid index or an
            /// invalid-index sentinel. The value returned by this method should
            /// only be given bits from `as_bits`.
            #[allow(dead_code, reason = "macro-generated code")]
            #[inline]
            pub fn from_bits(x: u32) -> Self {
                $entity(x)
            }
        }
    };

    // Include basic `Display` impl using the given display prefix.
    // Display a `Block` reference as "block12".
    ($entity:ident, $display_prefix:expr) => {
        $crate::entity_impl!($entity);

        impl core::fmt::Display for $entity {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                write!(f, concat!($display_prefix, "{}"), self.0)
            }
        }

        impl core::fmt::Debug for $entity {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                (self as &dyn core::fmt::Display).fmt(f)
            }
        }
    };

    // Alternate form for tuples we can't directly construct; providing "to" and "from" expressions
    // to turn an index *into* an entity, or get an index *from* an entity.
    ($entity:ident, $display_prefix:expr, $arg:ident, $to_expr:expr, $from_expr:expr) => {
        impl $crate::EntityRef for $entity {
            #[inline]
            fn new(index: usize) -> Self {
                debug_assert!(index < (core::u32::MAX as usize));
                let $arg = index as u32;
                $to_expr
            }

            #[inline]
            fn index(self) -> usize {
                let $arg = self;
                $from_expr as usize
            }
        }

        impl $crate::packed_option::ReservedValue for $entity {
            #[inline]
            fn reserved_value() -> $entity {
                $entity::from_u32(core::u32::MAX)
            }

            #[inline]
            fn is_reserved_value(&self) -> bool {
                self.as_u32() == core::u32::MAX
            }
        }

        impl $entity {
            /// Create a new instance from a `u32`.
            #[allow(dead_code, reason = "macro-generated code")]
            #[inline]
            pub fn from_u32(x: u32) -> Self {
                debug_assert!(x < core::u32::MAX);
                let $arg = x;
                $to_expr
            }

            /// Return the underlying index value as a `u32`.
            #[allow(dead_code, reason = "macro-generated code")]
            #[inline]
            pub fn as_u32(self) -> u32 {
                let $arg = self;
                $from_expr
            }
        }

        impl core::fmt::Display for $entity {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                write!(f, concat!($display_prefix, "{}"), self.as_u32())
            }
        }

        impl core::fmt::Debug for $entity {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                (self as &dyn core::fmt::Display).fmt(f)
            }
        }
    };
}

/// Densely numbered entity references as mapping keys.

/// A primary mapping `K -> V` allocating dense entity references.
///
/// The `PrimaryMap` data structure uses the dense index space to implement a map with a vector.
///
/// A primary map contains the main definition of an entity, and it can be used to allocate new
/// entity references with the `push` method.
///
/// There should only be a single `PrimaryMap` instance for a given `EntityRef` type, otherwise
/// conflicting references will be created. Using unknown keys for indexing will cause a panic.
///
/// Note that `PrimaryMap` doesn't implement `Deref` or `DerefMut`, which would allow
/// `&PrimaryMap<K, V>` to convert to `&[V]`. One of the main advantages of `PrimaryMap` is
/// that it only allows indexing with the distinct `EntityRef` key type, so converting to a
/// plain slice would make it easier to use incorrectly.
#[derive(Clone, Hash, PartialEq, Eq)]
pub struct PrimaryMap<K, V>
where
    K: EntityRef,
{
    elems: Vec<V>,
    unused: PhantomData<K>,
}

impl<K, V> PrimaryMap<K, V>
where
    K: EntityRef,
{
    /// Create a new empty map.
    pub fn new() -> Self {
        Self {
            elems: Vec::new(),
            unused: PhantomData,
        }
    }

    /// Create a new empty map with the given capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            elems: Vec::with_capacity(capacity),
            unused: PhantomData,
        }
    }

    /// Check if `k` is a valid key in the map.
    pub fn is_valid(&self, k: K) -> bool {
        k.index() < self.elems.len()
    }

    /// Get the element at `k` if it exists.
    pub fn get(&self, k: K) -> Option<&V> {
        self.elems.get(k.index())
    }

    /// Get the slice of values associated with the given range of keys, if any.
    pub fn get_range(&self, range: core::ops::Range<K>) -> Option<&[V]> {
        self.elems.get(range.start.index()..range.end.index())
    }

    /// Get the element at `k` if it exists, mutable version.
    pub fn get_mut(&mut self, k: K) -> Option<&mut V> {
        self.elems.get_mut(k.index())
    }

    /// Is this map completely empty?
    pub fn is_empty(&self) -> bool {
        self.elems.is_empty()
    }

    /// Get the total number of entity references created.
    pub fn len(&self) -> usize {
        self.elems.len()
    }

    /// Iterate over all the keys in this map.
    pub fn keys(&self) -> Keys<K> {
        Keys::with_len(self.elems.len())
    }

    /// Iterate over all the values in this map.
    pub fn values(&self) -> slice::Iter<'_, V> {
        self.elems.iter()
    }

    /// Iterate over all the values in this map, mutable edition.
    pub fn values_mut(&mut self) -> slice::IterMut<'_, V> {
        self.elems.iter_mut()
    }

    /// Get this map's underlying values as a slice.
    pub fn as_values_slice(&self) -> &[V] {
        &self.elems
    }

    /// Iterate over all the keys and values in this map.
    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter::new(self.elems.iter())
    }

    /// Iterate over all the keys and values in this map, mutable edition.
    pub fn iter_mut(&mut self) -> IterMut<'_, K, V> {
        IterMut::new(self.elems.iter_mut())
    }

    /// Remove all entries from this map.
    pub fn clear(&mut self) {
        self.elems.clear()
    }

    /// Get the key that will be assigned to the next pushed value.
    pub fn next_key(&self) -> K {
        K::new(self.elems.len())
    }

    /// Append `v` to the mapping, assigning a new key which is returned.
    pub fn push(&mut self, v: V) -> K {
        let k = self.next_key();
        self.elems.push(v);
        k
    }

    /// Returns the last element that was inserted in the map.
    pub fn last(&self) -> Option<(K, &V)> {
        let len = self.elems.len();
        let last = self.elems.last()?;
        Some((K::new(len - 1), last))
    }

    /// Returns the last element that was inserted in the map.
    pub fn last_mut(&mut self) -> Option<(K, &mut V)> {
        let len = self.elems.len();
        let last = self.elems.last_mut()?;
        Some((K::new(len - 1), last))
    }

    /// Reserves capacity for at least `additional` more elements to be inserted.
    pub fn reserve(&mut self, additional: usize) {
        self.elems.reserve(additional)
    }

    /// Reserves the minimum capacity for exactly `additional` more elements to be inserted.
    pub fn reserve_exact(&mut self, additional: usize) {
        self.elems.reserve_exact(additional)
    }

    /// Shrinks the capacity of the `PrimaryMap` as much as possible.
    pub fn shrink_to_fit(&mut self) {
        self.elems.shrink_to_fit()
    }

    /// Returns mutable references to many elements at once.
    ///
    /// Returns an error if an element does not exist, or if the same key was passed more than
    /// once.
    pub fn get_disjoint_mut<const N: usize>(
        &mut self,
        indices: [K; N],
    ) -> Result<[&mut V; N], slice::GetDisjointMutError> {
        self.elems.get_disjoint_mut(indices.map(|k| k.index()))
    }

    /// Performs a binary search on the values with a key extraction function.
    ///
    /// Assumes that the values are sorted by the key extracted by the function.
    ///
    /// If the value is found then `Ok(K)` is returned, containing the entity key
    /// of the matching value.
    ///
    /// If there are multiple matches, then any one of the matches could be returned.
    ///
    /// If the value is not found then Err(K) is returned, containing the entity key
    /// where a matching element could be inserted while maintaining sorted order.
    pub fn binary_search_values_by_key<'a, B, F>(&'a self, b: &B, f: F) -> Result<K, K>
    where
        F: FnMut(&'a V) -> B,
        B: Ord,
    {
        self.elems
            .binary_search_by_key(b, f)
            .map(|i| K::new(i))
            .map_err(|i| K::new(i))
    }

    /// Analog of `get_raw` except that a raw pointer is returned rather than a
    /// mutable reference.
    ///
    /// The default accessors of items in [`PrimaryMap`] will invalidate all
    /// previous borrows obtained from the map according to miri. This function
    /// can be used to acquire a pointer and then subsequently acquire a second
    /// pointer later on without invalidating the first one. In other words
    /// this is only here to help borrow two elements simultaneously with miri.
    pub fn get_raw_mut(&mut self, k: K) -> Option<*mut V> {
        if k.index() < self.elems.len() {
            // SAFETY: the `add` function requires that the index is in-bounds
            // with respect to the allocation which is satisfied here due to
            // the bounds-check above.
            unsafe { Some(self.elems.as_mut_ptr().add(k.index())) }
        } else {
            None
        }
    }
}

impl<K, V> Default for PrimaryMap<K, V>
where
    K: EntityRef,
{
    fn default() -> PrimaryMap<K, V> {
        PrimaryMap::new()
    }
}

/// Immutable indexing into an `PrimaryMap`.
/// The indexed value must be in the map.
impl<K, V> Index<K> for PrimaryMap<K, V>
where
    K: EntityRef,
{
    type Output = V;

    fn index(&self, k: K) -> &V {
        &self.elems[k.index()]
    }
}

/// Mutable indexing into an `PrimaryMap`.
impl<K, V> IndexMut<K> for PrimaryMap<K, V>
where
    K: EntityRef,
{
    fn index_mut(&mut self, k: K) -> &mut V {
        &mut self.elems[k.index()]
    }
}

impl<K, V> IntoIterator for PrimaryMap<K, V>
where
    K: EntityRef,
{
    type Item = (K, V);
    type IntoIter = IntoIter<K, V>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter::new(self.elems.into_iter())
    }
}

impl<'a, K, V> IntoIterator for &'a PrimaryMap<K, V>
where
    K: EntityRef,
{
    type Item = (K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        Iter::new(self.elems.iter())
    }
}

impl<'a, K, V> IntoIterator for &'a mut PrimaryMap<K, V>
where
    K: EntityRef,
{
    type Item = (K, &'a mut V);
    type IntoIter = IterMut<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        IterMut::new(self.elems.iter_mut())
    }
}

impl<K, V> FromIterator<V> for PrimaryMap<K, V>
where
    K: EntityRef,
{
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = V>,
    {
        Self {
            elems: Vec::from_iter(iter),
            unused: PhantomData,
        }
    }
}

impl<K, V> Extend<V> for PrimaryMap<K, V>
where
    K: EntityRef,
{
    fn extend<T>(&mut self, iter: T)
    where
        T: IntoIterator<Item = V>,
    {
        self.elems.extend(iter);
    }
}

impl<K, V> From<Vec<V>> for PrimaryMap<K, V>
where
    K: EntityRef,
{
    fn from(elems: Vec<V>) -> Self {
        Self {
            elems,
            unused: PhantomData,
        }
    }
}

impl<K, V> From<PrimaryMap<K, V>> for Vec<V>
where
    K: EntityRef,
{
    fn from(map: PrimaryMap<K, V>) -> Self {
        map.elems
    }
}

impl<K: EntityRef + fmt::Debug, V: fmt::Debug> fmt::Debug for PrimaryMap<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut struct_ = f.debug_struct("PrimaryMap");
        for (k, v) in self {
            struct_.field(&alloc::format!("{k:?}"), v);
        }
        struct_.finish()
    }
}

/// A double-ended iterator over entity references.
///
/// When `core::iter::Step` is stabilized, `Keys` could be implemented as a wrapper around
/// `core::ops::Range`, but for now, we implement it manually.

/// Iterate over all keys in order.
pub struct Keys<K: EntityRef> {
    pos: usize,
    rev_pos: usize,
    unused: PhantomData<K>,
}

impl<K: EntityRef> Keys<K> {
    /// Create a `Keys` iterator that visits `len` entities starting from 0.
    pub fn with_len(len: usize) -> Self {
        Self {
            pos: 0,
            rev_pos: len,
            unused: PhantomData,
        }
    }
}

impl<K: EntityRef> Iterator for Keys<K> {
    type Item = K;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos < self.rev_pos {
            let k = K::new(self.pos);
            self.pos += 1;
            Some(k)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let size = self.rev_pos - self.pos;
        (size, Some(size))
    }
}

impl<K: EntityRef> DoubleEndedIterator for Keys<K> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.rev_pos > self.pos {
            let k = K::new(self.rev_pos - 1);
            self.rev_pos -= 1;
            Some(k)
        } else {
            None
        }
    }
}

impl<K: EntityRef> ExactSizeIterator for Keys<K> {}

/// Iterate over all keys in order.
pub struct Iter<'a, K: EntityRef, V>
where
    V: 'a,
{
    enumerate: Enumerate<slice::Iter<'a, V>>,
    unused: PhantomData<K>,
}

impl<'a, K: EntityRef, V> Iter<'a, K, V> {
    /// Create an `Iter` iterator that visits the `PrimaryMap` keys and values
    /// of `iter`.
    pub fn new(iter: slice::Iter<'a, V>) -> Self {
        Self {
            enumerate: iter.enumerate(),
            unused: PhantomData,
        }
    }
}

impl<'a, K: EntityRef, V> Iterator for Iter<'a, K, V> {
    type Item = (K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        self.enumerate.next().map(|(i, v)| (K::new(i), v))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.enumerate.size_hint()
    }
}

impl<'a, K: EntityRef, V> DoubleEndedIterator for Iter<'a, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.enumerate.next_back().map(|(i, v)| (K::new(i), v))
    }
}

impl<'a, K: EntityRef, V> ExactSizeIterator for Iter<'a, K, V> {}

/// Iterate over all keys in order.
pub struct IterMut<'a, K: EntityRef, V>
where
    V: 'a,
{
    enumerate: Enumerate<slice::IterMut<'a, V>>,
    unused: PhantomData<K>,
}

impl<'a, K: EntityRef, V> IterMut<'a, K, V> {
    /// Create an `IterMut` iterator that visits the `PrimaryMap` keys and values
    /// of `iter`.
    pub fn new(iter: slice::IterMut<'a, V>) -> Self {
        Self {
            enumerate: iter.enumerate(),
            unused: PhantomData,
        }
    }
}

impl<'a, K: EntityRef, V> Iterator for IterMut<'a, K, V> {
    type Item = (K, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        self.enumerate.next().map(|(i, v)| (K::new(i), v))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.enumerate.size_hint()
    }
}

impl<'a, K: EntityRef, V> DoubleEndedIterator for IterMut<'a, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.enumerate.next_back().map(|(i, v)| (K::new(i), v))
    }
}

impl<'a, K: EntityRef, V> ExactSizeIterator for IterMut<'a, K, V> {}

/// Iterate over all keys in order.
pub struct IntoIter<K: EntityRef, V> {
    enumerate: Enumerate<vec::IntoIter<V>>,
    unused: PhantomData<K>,
}

impl<K: EntityRef, V> IntoIter<K, V> {
    /// Create an `IntoIter` iterator that visits the `PrimaryMap` keys and values
    /// of `iter`.
    pub fn new(iter: vec::IntoIter<V>) -> Self {
        Self {
            enumerate: iter.enumerate(),
            unused: PhantomData,
        }
    }
}

impl<K: EntityRef, V> Iterator for IntoIter<K, V> {
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        self.enumerate.next().map(|(i, v)| (K::new(i), v))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.enumerate.size_hint()
    }
}

impl<K: EntityRef, V> DoubleEndedIterator for IntoIter<K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.enumerate.next_back().map(|(i, v)| (K::new(i), v))
    }
}

impl<K: EntityRef, V> ExactSizeIterator for IntoIter<K, V> {}
