//! This module contains Span-related definitions and common Span types.

use std::ops::{Add, AddAssign};

/// A span that is associated with values during validation.
///
/// Spans can represent a hierarchy for nested values
/// with the help of the [AddAssign](core::ops::AddAssign) trait.
pub trait Span: core::fmt::Debug + Clone + core::ops::AddAssign {}

/// Convenience trait for interacting with spans.
pub trait SpanExt: Sized {
    /// Combine two Span-like types. It is useful for
    /// spans wrapped in [Options](Option).
    fn combine(&mut self, span: Self);
}

impl<T: Span> SpanExt for T {
    fn combine(&mut self, span: Self) {
        *self += span;
    }
}

impl<T: Span> SpanExt for Option<T> {
    fn combine(&mut self, span: Self) {
        match span {
            Some(new_span) => match self {
                Some(s) => *s += new_span,
                None => *self = Some(new_span),
            },
            None => {
                *self = None;
            }
        }
    }
}

/// Spanned values can return an optional [Span](Span) about themselves.
pub trait Spanned {
    /// Span is information about the value.
    ///
    /// It is usually returned with validation errors so that the caller knows
    /// where the error happened in case of nested values such as arrays or maps.
    type Span: Span;

    /// Return the span for the value if any.
    ///
    /// Span hierarchy for nested values is controlled by the [Validators](crate::Validator),
    /// returning [None](Option::None) should reset the hierarchy during validation.
    fn span(&self) -> Option<Self::Span>;
}

#[cfg(feature = "smallvec")]
type KeysSmallVecArray = [String; 10];

#[cfg(feature = "smallvec")]
/// Using smallvec probably helps a bit by removing unnecessary allocations.
type KeysInner = smallvec_crate::SmallVec<KeysSmallVecArray>;

#[cfg(not(feature = "smallvec"))]
type KeysInner = Vec<String>;

/// A span consisting of consecutive string keys.
#[derive(Debug, Default, Clone, Eq, PartialEq)]
#[repr(transparent)]
pub struct Keys(KeysInner);

impl Span for Keys {}

impl Keys {
    /// Create a new instance with no keys.
    pub fn new() -> Self {
        Keys(KeysInner::new())
    }

    /// Create a new empty instance with a capacity.
    pub fn with_capacity(cap: usize) -> Self {
        Keys(KeysInner::with_capacity(cap))
    }

    /// Iterator over the keys.
    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.0.iter()
    }

    /// Mutable Iterator over the keys.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut String> {
        self.0.iter_mut()
    }

    /// Returns the keys joined by dots.
    pub fn dotted(&self) -> String {
        self.0.join(".")
    }

    /// Add a new key.
    pub fn push(&mut self, value: String) {
        self.0.push(value)
    }

    /// Return the inner container.
    pub fn into_inner(self) -> KeysInner {
        self.0
    }
}

impl AddAssign for Keys {
    fn add_assign(&mut self, rhs: Self) {
        self.0.extend(rhs.0.into_iter())
    }
}

impl<S: ToString> Add<S> for Keys {
    type Output = Self;
    fn add(mut self, rhs: S) -> Self::Output {
        self.push(rhs.to_string());
        self
    }
}

impl From<String> for Keys {
    fn from(s: String) -> Self {
        let mut v = Self::new();
        v.push(s);
        v
    }
}

impl IntoIterator for Keys {
    type Item = <KeysInner as IntoIterator>::Item;

    #[cfg(feature = "smallvec")]
    type IntoIter = smallvec_crate::IntoIter<KeysSmallVecArray>;

    #[cfg(not(feature = "smallvec"))]
    type IntoIter = std::vec::IntoIter<String>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
