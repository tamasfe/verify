//! Error definitions used during Schema-related validation.

use crate::span::Span;
use schemars_crate::schema::{InstanceType, Metadata, SingleOrVec};
use std::ops::AddAssign;

/// A validation error.
///
/// It contains an optional span of the invalid value and optional information about the schema that caused the error.
#[derive(Debug, Clone, PartialEq)]
pub struct Error<S: Span> {
    /// Information about the schema that caused the validation
    /// error.
    pub meta: Option<Box<Metadata>>,

    /// The span of the invalid value.
    pub span: Option<S>,

    /// The actual error details.
    pub value: ErrorValue<S>,
}

impl<S: Span> Error<S> {
    pub(crate) fn new(meta: Option<Box<Metadata>>, span: Option<S>, value: ErrorValue<S>) -> Self {
        Self { meta, span, value }
    }
}

impl<S: Span> core::fmt::Display for Error<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut start_paren = false;

        if let Some(span) = &self.span {
            write!(f, "({:?}", span)?;
            start_paren = true;
        }

        if f.alternate() {
            if let Some(meta) = &self.meta {
                if let Some(title) = &meta.title {
                    if start_paren {
                        write!(f, r#", schema: "{}""#, title)?;
                    } else {
                        write!(f, r#"(schema: "{}""#, title)?;
                    }
                }
            }
        }

        if start_paren {
            write!(f, ") ")?;
        }

        write!(f, "{}", self.value)
    }
}

/// All the validation errors that can occur.
#[derive(Debug, Clone, PartialEq)]
// TODO maybe prefix or group them by type?
pub enum ErrorValue<S: Span> {
    /// Indicates that the schema will never match any value.
    NotAllowed,

    /// Indicates that the schema itself is invalid.
    InvalidSchema(InvalidSchema),

    /// Indicates incompatible value that cannot be validated
    /// by a schema.
    UnsupportedValue(UnsupportedValue),

    /// Indicates invalid type.
    InvalidType { expected: SingleOrVec<InstanceType> },

    /// Indicates invalid enum value.
    InvalidEnumValue { expected: Vec<serde_json::Value> },

    /// Indicates that the number is not multiple of the given value.
    NotMultipleOf { multiple_of: f64 },

    /// Indicates that the number is less than the given minimum value.
    LessThanExpected { min: f64, exclusive: bool },

    /// Indicates that the number is more than the given maximum value.
    MoreThanExpected { max: f64, exclusive: bool },

    /// Indicates that the string doesn't match the given pattern.
    NoPatternMatch { pattern: String },

    /// Indicates that the string is too long.
    TooLong { max_length: u32 },

    /// Indicates that the string is too short.
    TooShort { min_length: u32 },

    /// Indicates that none of the subschemas matched.
    NoneValid { errors: Vec<Errors<S>> },

    /// Indicates that more than one of the subschemas matched.
    MoreThanOneValid { matched: Vec<Option<Box<Metadata>>> },

    /// Indicates that a not schema matched.
    ValidNot { matched: Option<Box<Metadata>> },

    /// Indicates that the items in the array are not unique.
    NotUnique {
        first: Option<S>,
        duplicate: Option<S>,
    },

    /// Indicates that the array doesn't contain the value of a given schema.
    MustContain { schema: Option<Box<Metadata>> },

    /// Indicates that the array doesn't have enough items.
    NotEnoughItems { min: usize },

    /// Indicates that the array has too many items.
    TooManyItems { max: usize },

    /// Indicates that the object has too few properties.
    NotEnoughProperties { min: usize },

    /// Indicates that the object has too many properties.
    TooManyProperties { max: usize },

    /// Indicates that a required property is missing.
    RequiredProperty { name: String },

    /// Any error that does not originate from the validator.
    Custom(String),
}

/// Error that occurs when a value cannot be validated
/// by a schema.
#[derive(Debug, Clone, PartialEq)]
pub enum UnsupportedValue {
    /// Indicates that key of a map is not a string.
    KeyNotString,
}

impl core::fmt::Display for UnsupportedValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnsupportedValue::KeyNotString => write!(f, "map key must be a string"),
        }
    }
}

/// All errors related to the schema being invalid.
///
/// A schema must be valid in order to validate anything with it.
/// This error occurs if that is not the case.
///
/// It is also returned by calling [verify](crate::Verify::verify) on a schema.
#[derive(Debug, Clone, PartialEq)]
pub enum InvalidSchema {
    /// Indicates a missing local definition.
    MissingDefinition(String),

    /// Indicates an invalid regex pattern in the schema.
    InvalidPattern {
        pattern: String,
        error: regex::Error,
    },

    /// Indicates an unresolved external reference in the schema.
    ExternalReference(String),
}

impl core::fmt::Display for InvalidSchema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InvalidSchema::MissingDefinition(s) => write!(f, r#"missing local definition "{}""#, s),
            InvalidSchema::InvalidPattern { pattern, error } => {
                write!(f, r#"invalid regex pattern "{}": {}"#, pattern, error)
            }
            InvalidSchema::ExternalReference(r) => write!(
                f,
                r#"the schema contains unresolved external reference: "{}""#,
                r
            ),
        }
    }
}

impl<S: Span> core::fmt::Display for ErrorValue<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorValue::NotAllowed => write!(f, "value is not allowed here"),
            ErrorValue::InvalidSchema(err) => write!(f, "invalid schema: {}", err),
            ErrorValue::UnsupportedValue(err) => write!(f, "unsupported value: {}", err),
            ErrorValue::InvalidType { expected } => write!(
                f,
                "invalid type, expected {}",
                match expected {
                    SingleOrVec::Single(s) => {
                        format!(r#""{:?}""#, s)
                    }
                    SingleOrVec::Vec(v) => {
                        let mut s = "one of {".into();

                        for (i, t) in v.iter().enumerate() {
                            s += format!(r#""{:?}""#, t).as_str();
                            if i != v.len() - 1 {
                                s += ", "
                            }
                        }
                        s += "}";

                        s
                    }
                }
            ),
            ErrorValue::InvalidEnumValue { expected } => {
                let enum_vals: Vec<String> = expected.iter().map(|v| v.to_string()).collect();
                write!(
                    f,
                    "invalid enum value, expected to be one of {{{}}}",
                    enum_vals.join(", ")
                )
            }
            ErrorValue::NotMultipleOf { multiple_of } => {
                write!(f, "the value is expected to be multiple of {}", multiple_of)
            }
            ErrorValue::LessThanExpected { min, exclusive } => {
                if *exclusive {
                    write!(f, "the value is expected to be more than {}", min)
                } else {
                    write!(f, "the value is expected to be at least {}", min)
                }
            }
            ErrorValue::MoreThanExpected { max, exclusive } => {
                if *exclusive {
                    write!(f, "the value is expected to be less than {}", max)
                } else {
                    write!(f, "the value is expected to be at most {}", max)
                }
            }
            ErrorValue::NoPatternMatch { pattern } => {
                write!(f, r#"the string must match the pattern "{}""#, pattern)
            }
            ErrorValue::TooLong { max_length } => write!(
                f,
                r#"the string must not be longer than {} characters"#,
                max_length
            ),
            ErrorValue::TooShort { min_length } => write!(
                f,
                r#"the string must must be at least {} characters long"#,
                min_length
            ),
            ErrorValue::NoneValid { errors } => {
                writeln!(f, r#"no subschema matched the value:"#)?;

                for (i, e) in errors.iter().enumerate() {
                    write!(f, "{}", e)?;

                    if i != errors.len() - 1 {
                        writeln!(f, "\n")?;
                    }
                }

                Ok(())
            }
            ErrorValue::MoreThanOneValid { matched } => writeln!(
                f,
                r#"expected exactly one schema to match, but {} schemas matched"#,
                matched.len()
            ),
            ErrorValue::ValidNot { matched } => {
                if let Some(meta) = matched {
                    if let Some(title) = &meta.title {
                        return writeln!(f, r#"the value must not be a "{}""#, title);
                    }
                }

                writeln!(f, r#"the value is disallowed by a "not" schema"#)
            }
            ErrorValue::NotUnique { first, duplicate } => {
                if let (Some(first), Some(dup)) = (first, duplicate) {
                    writeln!(
                        f,
                        r#"all items in the array must be unique, but "{:?}" and "{:?}" are the same"#,
                        first, dup
                    )
                } else {
                    writeln!(f, r#"all items in the array must be unique"#)
                }
            }
            ErrorValue::MustContain { schema } => {
                if let Some(meta) = schema {
                    if let Some(title) = &meta.title {
                        return writeln!(
                            f,
                            r#"at least one of the items in the array must be "{}""#,
                            title
                        );
                    }
                }

                writeln!(
                    f,
                    r#"at least one of the items in the array must match the given schema"#
                )
            }
            ErrorValue::NotEnoughItems { min } => {
                write!(f, "the array must have at least {} items", min)
            }
            ErrorValue::TooManyItems { max } => {
                write!(f, "the array cannot have more than {} items", max)
            }
            ErrorValue::NotEnoughProperties { min } => {
                write!(f, "the object must have at least {} properties", min)
            }
            ErrorValue::TooManyProperties { max } => {
                write!(f, "the object cannot have more than {} properties", max)
            }
            ErrorValue::RequiredProperty { name } => {
                write!(f, r#"the required property "{}" is missing"#, name)
            }
            ErrorValue::Custom(err) => err.fmt(f),
        }
    }
}

#[cfg(feature = "smallvec")]
type SmallVecArray<S> = [Error<S>; 10];

#[cfg(feature = "smallvec")]
/// In a lot of cases there are only 1 or 2 errors
/// so using smallvec probably helps a bit by removing unnecessary allocations.
pub(super) type ErrorsInner<S> = smallvec_crate::SmallVec<SmallVecArray<S>>;

#[cfg(not(feature = "smallvec"))]
pub(super) type ErrorsInner<S> = Vec<Error<S>>;

/// A collection of [Errors](Error), this type is returned from validation.
#[derive(Debug, Clone, PartialEq)]
#[repr(transparent)]
pub struct Errors<S: Span>(pub(crate) ErrorsInner<S>);

impl<S: Span> Errors<S> {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Error<S>> {
        self.0.iter()
    }

    pub(super) fn new() -> Self {
        Errors(ErrorsInner::new())
    }

    pub(super) fn one(error: Error<S>) -> Self {
        let mut v = ErrorsInner::new();
        v.push(error);
        Errors(v)
    }
}

impl<S: Span> IntoIterator for Errors<S> {
    type Item = <ErrorsInner<S> as IntoIterator>::Item;

    #[cfg(feature = "smallvec")]
    type IntoIter = smallvec_crate::IntoIter<SmallVecArray<S>>;

    #[cfg(not(feature = "smallvec"))]
    type IntoIter = std::vec::IntoIter<Error<S>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<S: Span> core::fmt::Display for Errors<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for e in &self.0 {
            writeln!(f, "{}", e)?;
        }
        Ok(())
    }
}

impl<S: Span> std::error::Error for Errors<S> {}
impl<S: Span> crate::Error for Errors<S> {
    fn custom<T: core::fmt::Display>(error: T) -> Self {
        Self::one(Error::new(None, None, ErrorValue::Custom(error.to_string())))
    }
}

impl<S: Span> AddAssign for Errors<S> {
    fn add_assign(&mut self, rhs: Self) {
        self.0.extend(rhs.0.into_iter());
    }
}
