#![cfg_attr(feature = "docs", feature(doc_cfg))]

/*!

# Overview

The main idea is based on [Serde](::serde)'s [Serialize](serde::Serialize) and [Serializer](serde::ser::Serializer) traits.

Similarly, Verify has a [Validate](Validate) trait that types can implement and decide how they should be validated,
and a [Validator](Validator) trait for types that do the actual validation.

[Spans](span::Span) contain information about each value during validation, with their help
any deeply nested value can be identified after validation without additional lookups.

There are also two higher level traits: [Verify](Verify) and [Verifier](Verifier).

[Verifiers](Verifier) validate values that implement [Validate](Validate) *somehow* internally, it might involve multiple validations and even validators or none at all.

[Verify](Verify) is implemented by types that can verify themselves *somehow*, it might or might not involve a [Validator](Validator),
nothing is known about the validation process apart from the output.

Note that validation and verification mean pretty much the same thing **in the context of this library**,
the distinction is for naming purposes only.

Thanks to the similarity with [Serde](::serde), validating serializable values is rather straightforward, read more about it [here](crate::serde).

# Basic Usage

The library itself without features doesn't do much, it only provides definitions and common traits.

In order to use it you need to write or find a validator, or enable one of the implementation features of the library.
There is official support only for [Schemars](https://github.com/GREsau/schemars) at the moment.

This very basic example shows how to create a self-validating type with Verify and Schemars:

```edition2018
# use schemars_crate::{self as schemars, JsonSchema};
# use serde::Serialize;
# use verify::Verify;
#[derive(Default, Verify, Serialize, JsonSchema)]
#[verify(schemars, serde)]
struct ExampleStruct {
    example_value: i32,
}

let example = ExampleStruct::default();
assert!(example.verify().is_ok());
```

*/

pub mod span;

#[cfg(feature = "serde")]
#[doc(cfg(feature = "serde"))]
pub mod serde;

// Optional implementations for various crates.
mod impls;

// "impls" is only for code structure, it is removed
// for the public API.
pub use impls::*;

/**

Macro for deriving [Verify](Verify).

# Attributes

All options are set by the `verify` attribute.

## Container Attributes

### serde

Use [Serde](::serde) for validation.

**Options:**

- spans (optional): The name of the type that provides spans, it must implement [Spans](crate::serde::Spans).
By default [KeySpans](crate::serde::KeySpans) is used.

**Example:**

```ignore
#[verify(serde(spans = "KeySpans"))]
pub struct Example { ... }
```

### schemars

Use [Schemars](::schemars_crate) schema for validation.
Works only if the type implements [Serialize](serde::Serialize) and [JsonSchema](schemars_crate::JsonSchema).
It also needs the `serde` to be enabled.

**Example:**

```ignore
#[verify(schemars, serde)]
pub struct Example { ... }
```

### verifier

Provide what verifier to use.

**Options:**

- name: The name of the verifier type.
- create (optional): How the verifier should be constructed, [Default](Default) is used if not set.
- error (optional): The error type of the verifier, it might be needed when there are ambiguous complex generics
that cannot be guessed by the macro.

**Example:**

With name only:

```ignore
#[verify(verifier = "ExampleVerifier")]
pub struct Example { ... }
```

Or with options:

```ignore
#[verify(
    verifier(
        name = "verifiers::ExampleVerifier",
        create = "verifiers::get_a_new_verifier(self.config())"
    )
)]
pub struct Example { ... }
```

## Field Attributes

Field attributes are ignored for now.

*/
pub use verify_macros::Verify;

/// The errors returned by validators must implement this trait.
///
/// The [AddAssign](core::ops::AddAssign) bound is required in order to support
/// validators that return multiple errors.
pub trait Error: Sized + std::error::Error + core::ops::AddAssign {
    /// Values that are being validated can report errors that
    /// are not related to the validators.
    fn custom<T: core::fmt::Display>(error: T) -> Self;
}

/// Convenience trait for interacting with errors.
pub trait ErrorExt: Sized {
    /// Combine two error-like types. It is useful for
    /// spans wrapped in [Options](Option).
    fn combine(&mut self, span: Self);
}

impl<T: Error> ErrorExt for T {
    fn combine(&mut self, span: Self) {
        *self += span;
    }
}

impl<T: Error> ErrorExt for Option<T> {
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


/// Validate is implemented by values that can be validate themselves
/// against a given validator.
pub trait Validate: span::Spanned {
    /// Validate self against a given validator.
    fn validate<V: Validator<Self::Span>>(&self, validator: V) -> Result<(), V::Error>;
}

/// This trait is implemented by types that validate a value internally.
///
/// It is useful when the actual [Validator](Validator) is not exposed, or there
/// might be more than one validations taking place for the same value.
pub trait Verifier<S: span::Span> {
    /// The error returned by the validator.
    type Error: Error;

    /// Validate a value internally.
    fn verify_value<V: ?Sized + Validate<Span = S>>(&self, value: &V) -> Result<(), Self::Error>;

    /// Validators that support hierarchical spans might require a starting parent span.
    fn verify_value_with_span<V: ?Sized + Validate<Span = S>>(
        &self,
        value: &V,
        _span: Option<V::Span>,
    ) -> Result<(), Self::Error> {
        self.verify_value(value)
    }
}

/// This trait is implemented by types that can validate themselves.
pub trait Verify {
    /// The error returned by the validator.
    type Error: Error;

    /// Validate self internally.
    fn verify(&self) -> Result<(), Self::Error>;
}

/// Values that implement [Validate](Validate) can validate themselves against
/// types that implement this trait.
///
/// It is modelled after Serde [Serializer](serde::ser::Serializer), and works in a very similar fashion.
pub trait Validator<S: span::Span>: Sized {
    /// The error returned by the validator.
    type Error: Error;

    /// The type returned for validating sequences.
    ///
    /// The span type and error must match the Validator's.
    type ValidateSeq: ValidateSeq<S, Error = Self::Error>;

    /// The type returned for validating maps.
    ///
    /// The span type and error must match the Validator's.
    type ValidateMap: ValidateMap<S, Error = Self::Error>;

    /// Set the span for the current value that is being validated.
    ///
    /// In some cases this is needed to ensure that the validator returns
    /// the correct span in its errors.
    fn with_span(self, span: Option<S>) -> Self;

    /// Validate a bool value.
    fn validate_bool(self, v: bool) -> Result<(), Self::Error>;

    /// Validate an i8 value.
    fn validate_i8(self, v: i8) -> Result<(), Self::Error>;

    /// Validate an i16 value.
    fn validate_i16(self, v: i16) -> Result<(), Self::Error>;

    /// Validate an i32 value.
    fn validate_i32(self, v: i32) -> Result<(), Self::Error>;

    /// Validate an i64 value.
    fn validate_i64(self, v: i64) -> Result<(), Self::Error>;

    /// Validate an i128 value.
    fn validate_i128(self, v: i128) -> Result<(), Self::Error>;

    /// Validate an u8 value.
    fn validate_u8(self, v: u8) -> Result<(), Self::Error>;

    /// Validate an u16 value.
    fn validate_u16(self, v: u16) -> Result<(), Self::Error>;

    /// Validate an u32 value.
    fn validate_u32(self, v: u32) -> Result<(), Self::Error>;

    /// Validate an u64 value.
    fn validate_u64(self, v: u64) -> Result<(), Self::Error>;

    /// Validate an u128 value.
    fn validate_u128(self, v: u128) -> Result<(), Self::Error>;

    /// Validate an f32 value.
    fn validate_f32(self, v: f32) -> Result<(), Self::Error>;

    /// Validate an f64 value.
    fn validate_f64(self, v: f64) -> Result<(), Self::Error>;

    /// Validate a single char value.
    fn validate_char(self, v: char) -> Result<(), Self::Error>;

    /// Validate a string value.
    fn validate_str(self, v: &str) -> Result<(), Self::Error>;

    /// Validate slice of bytes.
    fn validate_bytes(self, v: &[u8]) -> Result<(), Self::Error>;

    /// Validate a [None](Option::None) value.
    fn validate_none(self) -> Result<(), Self::Error>;

    /// Validate a [Some](Option::Some) value.
    fn validate_some<V>(self, value: &V) -> Result<(), Self::Error>
    where
        V: ?Sized + Validate<Span = S>;

    /// Validate an empty tuple `()`.
    fn validate_unit(self) -> Result<(), Self::Error>;

    /// Validate a zero-sized struct.
    fn validate_unit_struct(self, name: &'static str) -> Result<(), Self::Error>;

    /// Validate a unit enum variant.
    fn validate_unit_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
    ) -> Result<(), Self::Error>;

    /// Validate a sequence.
    fn validate_seq(self, len: Option<usize>) -> Result<Self::ValidateSeq, Self::Error>;

    /// Validate a map.
    fn validate_map(self, len: Option<usize>) -> Result<Self::ValidateMap, Self::Error>;

    /// Validate a tag for a map value.
    ///
    /// This exists in order to support Serde's "externally tagged" types,
    /// for example `{ "TAG": ... }`.
    fn validate_tag<V>(&mut self, tag: &V) -> Result<(), Self::Error>
    where
        V: ?Sized + Validate<Span = S> + ToString;
}

/// Type returned by [validate_seq](Validator::validate_seq).
pub trait ValidateSeq<S: span::Span> {
    /// The error returned by the validator.
    type Error: std::error::Error;

    /// Set the span for the current value that is being validated.
    ///
    /// In some cases this is needed to ensure that the validator returns
    /// the correct span in its errors.
    fn with_span(&mut self, span: Option<S>) -> &mut Self;

    /// Validate an element in the sequence.
    fn validate_element<V>(&mut self, value: &V) -> Result<(), Self::Error>
    where
        V: ?Sized + Validate<Span = S> + core::hash::Hash;

    /// End the sequence.
    fn end(self) -> Result<(), Self::Error>;
}

/// Type returned by [validate_map](Validator::validate_map).
pub trait ValidateMap<S: span::Span> {
    /// The error returned by the validator.
    type Error: std::error::Error;

    /// Set the span for the current value that is being validated.
    ///
    /// In some cases this is needed to ensure that the validator returns
    /// the correct span in its errors.
    fn with_span(&mut self, span: Option<S>) -> &mut Self;

    /// Validate a key in the map.
    fn validate_key<V>(&mut self, key: &V) -> Result<(), Self::Error>
    where
        V: ?Sized + Validate<Span = S>;

    /// Validate a key in the map.
    ///
    /// This method guarantees that the key is a string
    /// or has a string representation.
    fn validate_string_key<V>(&mut self, key: &V) -> Result<(), Self::Error>
    where
        V: ?Sized + Validate<Span = S> + ToString;

    /// Validate a map entry.
    fn validate_value<V: ?Sized>(&mut self, value: &V) -> Result<(), Self::Error>
    where
        V: Validate<Span = S>;

    /// Validate an entry (key and value).
    fn validate_entry<K: ?Sized, V: ?Sized>(
        &mut self,
        key: &K,
        value: &V,
    ) -> Result<(), Self::Error>
    where
        K: Validate<Span = S>,
        V: Validate<Span = S>,
    {
        self.validate_key(key)?;
        self.validate_value(value)
    }

    /// Validate an entry (key and value).
    ///
    /// This method guarantees that the key is a string
    /// or has a string representation.
    fn validate_string_entry<K, V>(&mut self, key: &K, value: &V) -> Result<(), Self::Error>
    where
        K: ?Sized + Validate<Span = S> + ToString,
        V: ?Sized + Validate<Span = S>,
    {
        self.validate_string_key(key)?;
        self.validate_value(value)
    }

    /// Some [Validate](Validate) implementors can convert map keys to strings.
    /// With this method redundant conversions can be avoided.
    fn string_key_required(&self) -> bool {
        false
    }

    /// End the map.
    fn end(self) -> Result<(), Self::Error>;
}
