use std::collections::HashSet;
use std::hash::Hash;

/// Returns the set difference: elements in `a` that are not in `b`.
/// Note: this code is hot, so is optimized for speed.
pub fn difference<A: Eq + Hash + Clone>(a: &HashSet<A>, b: &HashSet<A>) -> HashSet<A> {
    let mut result = HashSet::new();
    for item in a {
        if !b.contains(item) {
            result.insert(item.clone());
        }
    }
    result
}

/// Returns true if `a` and `b` have at least one element in common.
/// Note: this code is hot, so is optimized for speed.
pub fn intersects<A: Eq + Hash>(a: &HashSet<A>, b: &HashSet<A>) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    for item in a {
        if b.contains(item) {
            return true;
        }
    }
    false
}

/// Returns true if every element in `a` is also in `b`.
/// Note: this code is hot, so is optimized for speed.
pub fn every<A: Eq + Hash>(a: &HashSet<A>, b: &HashSet<A>) -> bool {
    for item in a {
        if !b.contains(item) {
            return false;
        }
    }
    true
}

/// Returns the union of sets `a` and `b`.
/// Note: this code is hot, so is optimized for speed.
pub fn union<A: Eq + Hash + Clone>(a: &HashSet<A>, b: &HashSet<A>) -> HashSet<A> {
    let mut result = HashSet::new();
    for item in a {
        result.insert(item.clone());
    }
    for item in b {
        result.insert(item.clone());
    }
    result
}
