//! Implementation of Verify, Verifier and Validator for schemas.

use crate::{
    span::{Span, SpanExt},
    Validate, ValidateMap, ValidateSeq, Validator, Verifier,
};
use schemars_crate::{
    schema::{InstanceType, RootSchema, Schema, SchemaObject, SingleOrVec},
    Set,
};
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
};

use super::errors::{Error, ErrorValue, Errors, ErrorsInner, InvalidSchema, UnsupportedValue};

impl<S: Span> Verifier<S> for RootSchema {
    type Error = Errors<S>;

    fn verify_value<V: ?Sized + Validate<Span = S>>(&self, value: &V) -> Result<(), Self::Error> {
        self.schema.verify_value(value)
    }

    fn verify_value_with_span<V: ?Sized + Validate<Span = S>>(
        &self,
        value: &V,
        span: Option<V::Span>,
    ) -> Result<(), Self::Error> {
        self.schema.verify_value_with_span(value, span)
    }
}

impl<S: Span> Verifier<S> for Schema {
    type Error = Errors<S>;

    fn verify_value<V: ?Sized + Validate<Span = S>>(&self, value: &V) -> Result<(), Self::Error> {
        let r = SchemaRef::from(self);
        let s = not_bool_schema!(r, &value.span());
        s.verify_value(value)
    }

    fn verify_value_with_span<V: ?Sized + Validate<Span = S>>(
        &self,
        value: &V,
        span: Option<V::Span>,
    ) -> Result<(), Self::Error> {
        let r = SchemaRef::from(self);
        let s = not_bool_schema!(r, &value.span());
        s.verify_value_with_span(value, span)
    }
}

impl<S: Span> Verifier<S> for SchemaObject {
    type Error = Errors<S>;

    fn verify_value<V: ?Sized + Validate<Span = S>>(&self, value: &V) -> Result<(), Self::Error> {
        self.verify_value_with_span(value, None)
    }

    fn verify_value_with_span<V: ?Sized + Validate<Span = S>>(
        &self,
        value: &V,
        span: Option<V::Span>,
    ) -> Result<(), Self::Error> {
        let mut errors = validate_subschemas(self, span.clone(), value).err();

        if let Err(e) = value.validate(
            SchemaValidator::from_ref(SchemaRef::Object(self))
                .with_parent_span(span)
                .with_span(value.span()),
        ) {
            match &mut errors {
                Some(errs) => {
                    *errs += e;
                }
                None => errors = Some(e),
            }
        }

        match errors {
            None => Ok(()),
            Some(e) => Err(e),
        }
    }
}

/// Validate all the allOf anyOf, etc. schemas for a given value.
fn validate_subschemas<V: ?Sized + Validate>(
    schema: &SchemaObject,
    parent_span: Option<V::Span>,
    value: &V,
) -> Result<(), Errors<V::Span>> {
    if let Some(sub) = &schema.subschemas {
        let mut errors = ErrorsInner::new();

        if let Some(all_of) = &sub.all_of {
            for s in all_of {
                if let Err(e) = s.verify_value_with_span(value, parent_span.clone()) {
                    errors.extend(e.0.into_iter());
                }
            }
        }

        if let Some(any_of) = &sub.any_of {
            let mut validated = Vec::with_capacity(any_of.len());
            let mut inner_errors: Vec<Errors<_>> = Vec::with_capacity(any_of.len());
            for s in any_of {
                match s.verify_value_with_span(value, parent_span.clone()) {
                    Ok(_) => match s {
                        Schema::Object(o) => {
                            validated.push(o.metadata.clone());
                        }
                        _ => {
                            validated.push(None);
                        }
                    },
                    Err(e) => {
                        inner_errors.push(e);
                    }
                }
            }
            if validated.is_empty() {
                errors.push(Error::new(
                    schema.metadata.clone(),
                    value.span(),
                    ErrorValue::NoneValid {
                        errors: inner_errors,
                    },
                ));
            } else if validated.len() > 1 {
                errors.push(Error::new(
                    schema.metadata.clone(),
                    value.span(),
                    ErrorValue::MoreThanOneValid { matched: validated },
                ));
            }
        }

        if let Some(one_of) = &sub.one_of {
            let mut validated = Vec::with_capacity(one_of.len());
            let mut inner_errors: Vec<Errors<_>> = Vec::with_capacity(one_of.len());
            for s in one_of {
                match s.verify_value_with_span(value, parent_span.clone()) {
                    Ok(_) => match s {
                        Schema::Object(o) => {
                            validated.push(o.metadata.clone());
                        }
                        _ => {
                            validated.push(None);
                        }
                    },
                    Err(e) => {
                        inner_errors.push(e);
                    }
                }
            }
            if validated.is_empty() {
                errors.push(Error::new(
                    schema.metadata.clone(),
                    value.span(),
                    ErrorValue::NoneValid {
                        errors: inner_errors,
                    },
                ));
            } else if validated.len() > 1 {
                errors.push(Error::new(
                    schema.metadata.clone(),
                    value.span(),
                    ErrorValue::MoreThanOneValid { matched: validated },
                ));
            }
        }

        if let (Some(sub_if), Some(sub_then)) = (&sub.if_schema, &sub.then_schema) {
            if sub_if
                .verify_value_with_span(value, parent_span.clone())
                .is_ok()
            {
                if let Err(e) = sub_then.verify_value_with_span(value, parent_span.clone()) {
                    errors.extend(e.0.into_iter());
                }
            } else if let Some(sub_else) = &sub.else_schema {
                if let Err(e) = sub_else.verify_value_with_span(value, parent_span.clone()) {
                    errors.extend(e.0.into_iter());
                }
            }
        }

        if let Some(not) = &sub.not {
            if not
                .verify_value_with_span(value, parent_span.clone())
                .is_ok()
            {
                errors.push(Error::new(
                    schema.metadata.clone(),
                    value.span(),
                    ErrorValue::ValidNot {
                        matched: match &**not {
                            Schema::Object(o) => o.metadata.clone(),
                            _ => None,
                        },
                    },
                ));
            }
        }

        return if errors.is_empty() {
            Ok(())
        } else {
            Err(Errors(errors))
        };
    }

    Ok(())
}

/// This is technically not needed anymore,
/// but should do no harm to leave it as is.
enum SchemaRef<'s> {
    Bool(bool),
    Object(&'s SchemaObject),
}

impl<'s> From<&'s Schema> for SchemaRef<'s> {
    fn from(s: &'s Schema) -> Self {
        match s {
            Schema::Bool(b) => SchemaRef::Bool(*b),
            Schema::Object(o) => SchemaRef::Object(o),
        }
    }
}

impl<'s> From<&'s RootSchema> for SchemaRef<'s> {
    fn from(s: &'s RootSchema) -> Self {
        SchemaRef::Object(&s.schema)
    }
}

/// A validator that validates a given schema.
///
/// This is not exposed directly because a value must be validated
/// against multiple schemas in some cases. So the `Schema::verify` methods
/// must be used instead, it will validate the value against subschemas.
struct SchemaValidator<'a, S: Span> {
    schema: SchemaRef<'a>,

    // If a schema was not found for an external tag,
    // everything should be allowed.
    tagged_allow: bool,

    parent_span: Option<S>,
    span: Option<S>,

    // to avoid some unnecessary clones
    combined_span: Option<S>,

    // Array tracking
    arr_item_count: usize,
    // For unique checks
    arr_hashes: HashMap<u64, Option<S>>,
    arr_contains: Option<&'a Schema>,

    // Object tracking
    obj_required: Set<String>,
    obj_prop_count: usize,
    obj_last_key: Option<String>,
}

impl<'a, S: Span> SchemaValidator<'a, S> {
    fn from_ref(schema: SchemaRef<'a>) -> Self {
        Self {
            schema,
            parent_span: None,
            span: None,
            combined_span: None,
            tagged_allow: false,
            arr_item_count: 0,
            arr_hashes: HashMap::new(),
            arr_contains: None,
            obj_required: Set::new(),
            obj_prop_count: 0,
            obj_last_key: None,
        }
    }

    fn with_parent_span(mut self, span: Option<S>) -> Self {
        self.parent_span = span;

        self
    }

    fn combine_spans(&mut self) {
        if self.combined_span.is_none() {
            self.combined_span = self.parent_span.clone();
            self.parent_span.combine(self.span.clone());
        }
    }
}

impl<'a, S: Span> Validator<S> for SchemaValidator<'a, S> {
    type Error = Errors<S>;

    type ValidateSeq = Self;
    type ValidateMap = Self;

    fn with_span(mut self, span: Option<S>) -> Self {
        match span {
            Some(s) => {
                self.span = s.into();
            }
            None => {
                self.parent_span = None;
                self.span = None;
            }
        }
        self.combined_span = None;

        self
    }

    fn validate_bool(mut self, v: bool) -> Result<(), Self::Error> {
        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Boolean, s, &self.combined_span)?;
        check_enum!(bool, v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_i8(mut self, v: i8) -> Result<(), Self::Error> {
        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Integer, s, &self.combined_span)?;
        check_enum!(int, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_i16(mut self, v: i16) -> Result<(), Self::Error> {
        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Integer, s, &self.combined_span)?;
        check_enum!(int, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_i32(mut self, v: i32) -> Result<(), Self::Error> {
        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Integer, s, &self.combined_span)?;
        check_enum!(int, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_i64(mut self, v: i64) -> Result<(), Self::Error> {
        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Integer, s, &self.combined_span)?;
        check_enum!(int, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_i128(mut self, v: i128) -> Result<(), Self::Error> {
        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Integer, s, &self.combined_span)?;
        check_enum!(int, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_u8(mut self, v: u8) -> Result<(), Self::Error> {
        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Integer, s, &self.combined_span)?;
        check_enum!(int, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_u16(mut self, v: u16) -> Result<(), Self::Error> {
        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Integer, s, &self.combined_span)?;
        check_enum!(int, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_u32(mut self, v: u32) -> Result<(), Self::Error> {
        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Integer, s, &self.combined_span)?;
        check_enum!(int, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_u64(mut self, v: u64) -> Result<(), Self::Error> {
        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Integer, s, &self.combined_span)?;
        check_enum!(int, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_u128(mut self, v: u128) -> Result<(), Self::Error> {
        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Integer, s, &self.combined_span)?;
        check_enum!(int, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_f32(mut self, v: f32) -> Result<(), Self::Error> {
        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Number, s, &self.combined_span)?;
        check_enum!(float, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_f64(mut self, v: f64) -> Result<(), Self::Error> {
        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Number, s, &self.combined_span)?;
        check_enum!(float, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_char(self, v: char) -> Result<(), Self::Error> {
        self.validate_str(&v.to_string())
    }

    fn validate_str(mut self, v: &str) -> Result<(), Self::Error> {
        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(String, s, &self.combined_span)?;
        check_enum!(str, v, s, &self.combined_span)?;
        check_string!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_bytes(mut self, _v: &[u8]) -> Result<(), Self::Error> {
        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(String, s, &self.combined_span)?;
        // TODO anything else to check here?
        Ok(())
    }

    fn validate_none(mut self) -> Result<(), Self::Error> {
        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Null, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_some<T>(self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Validate<Span = S>,
    {
        value.validate(self)
    }

    fn validate_unit(mut self) -> Result<(), Self::Error> {
        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Null, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_unit_struct(mut self, _name: &'static str) -> Result<(), Self::Error> {
        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Null, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<(), Self::Error> {
        self.validate_str(variant)
    }

    fn validate_seq(mut self, len: Option<usize>) -> Result<Self::ValidateSeq, Self::Error> {
        self.combine_spans();
        let s = not_bool_schema!(self, &self.schema, &self.combined_span);

        check_type!(Array, s, &self.combined_span)?;

        if let Some(arr) = &s.array {
            if let Some(s) = &arr.contains {
                self.arr_contains = Some(s);
            }
        }
        if let Some(l) = len {
            self.arr_hashes.reserve(l);
        }
        Ok(self)
    }

    fn validate_map(mut self, _len: Option<usize>) -> Result<Self::ValidateMap, Self::Error> {
        self.combine_spans();
        let s = not_bool_schema!(self, &self.schema, &self.combined_span);

        check_type!(Object, s, &self.combined_span)?;

        if let Some(obj) = &s.object {
            self.obj_required = obj.required.clone();
        }

        Ok(self)
    }

    fn validate_tag<V>(&mut self, tag: &V) -> Result<(), Self::Error>
    where
        V: ?Sized + Validate<Span = S> + ToString,
    {
        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Object, s, &self.combined_span)?;

        let key = tag.to_string();

        // Look for the property that has the name of the tag,
        // and continue validation with that schema.
        if let Some(obj) = &s.object {
            if let Some(prop_schema) = obj.properties.get(&key) {
                self.schema = SchemaRef::from(prop_schema);
                return Ok(());
            }

            for (k, v) in obj.pattern_properties.iter() {
                let key_re = regex::Regex::new(k).map_err(|error| {
                    Errors::one(Error::new(
                        s.metadata.clone(),
                        tag.span(),
                        ErrorValue::InvalidSchema(InvalidSchema::InvalidPattern {
                            pattern: k.clone(),
                            error,
                        }),
                    ))
                })?;

                if key_re.is_match(&key) {
                    self.schema = SchemaRef::from(v);
                    return Ok(());
                }
            }

            if let Some(add_prop_schema) = &obj.additional_properties {
                self.schema = SchemaRef::from(&**add_prop_schema);
                return Ok(());
            }
        }

        self.tagged_allow = true;
        Ok(())
    }
}

impl<'a, S: Span> ValidateSeq<S> for SchemaValidator<'a, S> {
    type Error = Errors<S>;

    fn with_span(&mut self, span: Option<S>) -> &mut Self {
        match span {
            Some(s) => {
                self.span = s.into();
            }
            None => {
                self.parent_span = None;
                self.span = None;
            }
        }
        self.combined_span = None;

        self
    }

    fn validate_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Validate<Span = S> + Hash,
    {
        if self.tagged_allow {
            return Ok(());
        }

        self.arr_item_count += 1;

        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        let mut errors = Errors::new();

        let mut value_span = self.combined_span.clone();
        value_span.combine(value.span());

        if let Some(arr) = &s.array {
            if let Some(c) = self.arr_contains {
                if c.verify_value_with_span(value, value_span.clone()).is_ok() {
                    self.arr_contains = None;
                }
            }

            if let Some(items) = &arr.items {
                match items {
                    SingleOrVec::Single(single_schema) => {
                        if let Err(e) =
                            single_schema.verify_value_with_span(value, value_span.clone())
                        {
                            errors.0.extend(e.0.into_iter());
                        }
                    }
                    SingleOrVec::Vec(schemas) => {
                        if let Some(s) = schemas.get(self.arr_item_count - 1) {
                            if let Err(e) = s.verify_value_with_span(value, value_span.clone()) {
                                errors.0.extend(e.0.into_iter());
                            }
                        } else if let Some(s) = &arr.additional_items {
                            if let Err(e) = s.verify_value_with_span(value, value_span.clone()) {
                                errors.0.extend(e.0.into_iter());
                            }
                        }
                    }
                }
            }

            if let Some(true) = arr.unique_items {
                let mut hasher = DefaultHasher::new();
                value.hash(&mut hasher);
                let h = hasher.finish();

                let existing = self.arr_hashes.insert(h, value.span());

                if let Some(existing_val) = existing {
                    errors.0.push(Error::new(
                        s.metadata.clone(),
                        value.span(),
                        ErrorValue::NotUnique {
                            first: existing_val,
                            duplicate: value.span(),
                        },
                    ));
                }
            }
        }

        if !errors.0.is_empty() {
            Err(errors)
        } else {
            Ok(())
        }
    }

    fn end(mut self) -> Result<(), Self::Error> {
        if self.tagged_allow {
            return Ok(());
        }

        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);
        let mut errors = Errors::new();

        if let Some(c) = self.arr_contains {
            errors.0.push(Error::new(
                s.metadata.clone(),
                self.span.clone(),
                ErrorValue::MustContain {
                    schema: match c {
                        Schema::Bool(_) => None,
                        Schema::Object(o) => o.metadata.clone(),
                    },
                },
            ));
        }

        if let Some(arr) = &s.array {
            if let Some(min) = arr.min_items {
                if self.arr_item_count < min as usize {
                    errors.0.push(Error::new(
                        s.metadata.clone(),
                        self.span.clone(),
                        ErrorValue::NotEnoughItems { min: min as usize },
                    ));
                }
            }

            if let Some(max) = arr.max_items {
                if self.arr_item_count > max as usize {
                    errors.0.push(Error::new(
                        s.metadata.clone(),
                        self.span.clone(),
                        ErrorValue::TooManyItems { max: max as usize },
                    ));
                }
            }
        }

        if !errors.0.is_empty() {
            Err(errors)
        } else {
            Ok(())
        }
    }
}

impl<'a, S: Span> ValidateMap<S> for SchemaValidator<'a, S> {
    type Error = Errors<S>;

    fn with_span(&mut self, span: Option<S>) -> &mut Self {
        match span {
            Some(s) => {
                self.span = s.into();
            }
            None => {
                self.parent_span = None;
                self.span = None;
            }
        }
        self.combined_span = None;

        self
    }

    fn validate_key<T>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Validate<Span = S>,
    {
        if self.tagged_allow {
            return Ok(());
        }

        let meta = match &self.schema {
            SchemaRef::Bool(_) => None,
            SchemaRef::Object(o) => o.metadata.as_ref(),
        };

        Err(Errors::one(Error::new(
            meta.cloned(),
            key.span(),
            ErrorValue::UnsupportedValue(UnsupportedValue::KeyNotString),
        )))
    }

    fn validate_string_key<T>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Validate<Span = S> + ToString,
    {
        if self.tagged_allow {
            return Ok(());
        }

        self.obj_prop_count += 1;

        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        let mut key_span = self.combined_span.clone();
        key_span.combine(key.span());

        let key_string = key.to_string();

        self.obj_required.remove(&key_string);

        self.obj_last_key = Some(key_string);

        if let Some(obj) = &s.object {
            if let Some(name_schema) = &obj.property_names {
                if let Err(e) = name_schema.verify_value_with_span(key, key_span) {
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    fn validate_value<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + Validate<Span = S>,
    {
        if self.tagged_allow {
            return Ok(());
        }

        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);
        let key = self.obj_last_key.take().expect("no key before value");

        let mut value_span = self.combined_span.clone();
        value_span.combine(value.span());

        if let Some(obj) = &s.object {
            if let Some(prop_schema) = obj.properties.get(&key) {
                match prop_schema.verify_value_with_span(value, value_span.clone()) {
                    Ok(_) => {
                        return Ok(());
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }

            for (k, v) in obj.pattern_properties.iter() {
                let key_re = regex::Regex::new(k).map_err(|error| {
                    Errors::one(Error::new(
                        s.metadata.clone(),
                        value.span(),
                        ErrorValue::InvalidSchema(InvalidSchema::InvalidPattern {
                            pattern: k.clone(),
                            error,
                        }),
                    ))
                })?;

                if key_re.is_match(&key) {
                    match v.verify_value_with_span(value, value_span.clone()) {
                        Ok(_) => {
                            return Ok(());
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    }
                }
            }

            if let Some(add_prop_schema) = &obj.additional_properties {
                if let Err(e) = add_prop_schema.verify_value_with_span(value, value_span) {
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    fn end(mut self) -> Result<(), Self::Error> {
        if self.tagged_allow {
            return Ok(());
        }

        self.combine_spans();
        let s = not_bool_schema!(&self.schema, &self.combined_span);
        let mut errors = Errors::new();

        if let Some(obj) = &s.object {
            if let Some(max) = obj.max_properties {
                if self.obj_prop_count > max as usize {
                    errors.0.push(Error::new(
                        s.metadata.clone(),
                        self.span.clone(),
                        ErrorValue::TooManyProperties { max: max as usize },
                    ))
                }
            }

            if let Some(min) = obj.min_properties {
                if self.obj_prop_count < min as usize {
                    errors.0.push(Error::new(
                        s.metadata.clone(),
                        self.span.clone(),
                        ErrorValue::NotEnoughProperties { min: min as usize },
                    ))
                }
            }
        }

        for p in self.obj_required {
            errors.0.push(Error::new(
                s.metadata.clone(),
                self.span.clone(),
                ErrorValue::RequiredProperty { name: p },
            ))
        }

        if !errors.0.is_empty() {
            Err(errors)
        } else {
            Ok(())
        }
    }

    fn string_key_required(&self) -> bool {
        if self.tagged_allow {
            // It is accepted either way.
            false
        } else {
            true
        }
    }
}
