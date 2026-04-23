//! Tiny shared utilities for server systems.

use std::cmp::Ordering;

/// Compare two `f32`s, treating NaN as `Equal`. Use for sort keys / `min_by` /
/// `max_by` where `partial_cmp` returning `None` would otherwise need an
/// `unwrap_or` at every call site.
#[inline]
pub fn cmp_f32(a: f32, b: f32) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}
