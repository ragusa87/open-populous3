//! Typed stable ids and the registry that keeps objects by them (architecture.md "The tick", "Stable ids").
//!
//! An `Id<T>` only fits where a `T` is expected:
//! ```compile_fail
//! use game_core::id::Id;
//! struct Fight;
//! struct Melee;
//! let fight: Id<Fight> = Id::from_raw(1);
//! let melee: Id<Melee> = fight;
//! ```

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

/// The id of a `T`, from `GameMap::new_id_of`: never given twice on a map, the same on every lockstep peer.
pub struct Id<T>(u32, PhantomData<fn() -> T>);

impl<T> Id<T> {
    pub const fn from_raw(raw: u32) -> Self {
        Id(raw, PhantomData)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Id<T> {}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T> Eq for Id<T> {}

impl<T> PartialOrd for Id<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Id<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl<T> Hash for Id<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// Objects of one kind by id, in id order (a sorted `Vec`: lookups are binary searches, iteration is deterministic).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Registry<T> {
    items: Vec<(Id<T>, T)>,
}

impl<T> Default for Registry<T> {
    fn default() -> Self {
        Registry { items: Vec::new() }
    }
}

impl<T> Registry<T> {
    /// Adds `value` under `id`, which must be newer than every id in it (fresh from `GameMap::new_id_of`).
    pub fn insert(&mut self, id: Id<T>, value: T) {
        assert!(self.items.last().is_none_or(|(last, _)| *last < id), "ids only grow");
        self.items.push((id, value));
    }

    fn index(&self, id: Id<T>) -> Option<usize> {
        self.items.binary_search_by_key(&id, |(i, _)| *i).ok()
    }

    pub fn get(&self, id: Id<T>) -> Option<&T> {
        self.index(id).map(|i| &self.items[i].1)
    }

    pub fn get_mut(&mut self, id: Id<T>) -> Option<&mut T> {
        self.index(id).map(|i| &mut self.items[i].1)
    }

    pub fn contains(&self, id: Id<T>) -> bool {
        self.index(id).is_some()
    }

    pub fn iter(&self) -> impl Iterator<Item = (Id<T>, &T)> {
        self.items.iter().map(|(id, t)| (*id, t))
    }

    /// Each item with its id, as stored.
    pub fn entries(&self) -> impl Iterator<Item = &(Id<T>, T)> {
        self.items.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Id<T>, &mut T)> {
        self.items.iter_mut().map(|(id, t)| (*id, t))
    }

    pub fn ids(&self) -> impl Iterator<Item = Id<T>> + '_ {
        self.items.iter().map(|(id, _)| *id)
    }

    /// Keeps what `keep` accepts, in order.
    pub fn retain(&mut self, mut keep: impl FnMut(Id<T>, &T) -> bool) {
        self.items.retain(|(id, t)| keep(*id, t));
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kept_in_id_order_and_found_by_id() {
        let mut r = Registry::default();
        for n in [3, 7, 9] {
            r.insert(Id::from_raw(n), n * 10);
        }
        r.retain(|id, _| id.raw() != 7);
        assert_eq!(r.ids().map(Id::raw).collect::<Vec<_>>(), [3, 9]);
        assert_eq!((r.get(Id::from_raw(9)), r.get(Id::from_raw(7))), (Some(&90), None));
        *r.get_mut(Id::from_raw(3)).unwrap() += 1;
        let copy = r.clone();
        assert_eq!(copy.iter().collect::<Vec<_>>(), r.iter().collect::<Vec<_>>());
        assert_eq!(copy, r);
    }

    #[test]
    #[should_panic(expected = "ids only grow")]
    fn an_older_id_is_refused() {
        let mut r = Registry::default();
        r.insert(Id::<()>::from_raw(5), ());
        r.insert(Id::from_raw(5), ());
    }
}
