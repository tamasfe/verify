//! Implementation of Verify, Verifier and Validator for schemas.

use crate::{
    span::{Span, SpanExt},
    Validate, ValidateMap, ValidateSeq, Validator, Verifier,
};
use schemars_crate::{
    schema::{InstanceType, RootSchema, Schema, SchemaObject, SingleOrVec},
    Map, Set,
};
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
};

use super::errors::{Error, ErrorValue, Errors, ErrorsInner, InvalidSchema, UnsupportedValue};

impl<S: Span> Verifier<S> for RootSchema {
    type Error = Errors<S>;

    fn verify_value<V: ?Sized + Validate<Span = S>>(&self, value: &V) -> Result<(), Self::Error> {
        self.verify_value_with_span(value, None)
    }

    fn verify_value_with_span<V: ?Sized + Validate<Span = S>>(
        &self,
        value: &V,
        span: Option<V::Span>,
    ) -> Result<(), Self::Error> {
        SchemaValidator::new(&self.definitions, (&self.schema).into())
            .with_parent_span(span)
            .validate_inner(value)
    }
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

impl<'s> From<&'s SchemaObject> for SchemaRef<'s> {
    fn from(s: &'s SchemaObject) -> Self {
        SchemaRef::Object(s)
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
    defs: &'a Map<String, Schema>,

    // If a schema was not found for an external tag,
    // everything should be allowed.
    tagged_allow: bool,

    parent_span: Option<S>,
    span: Option<S>,

    // to avoid some unnecessary clones
    combined_span: Option<S>,

    // Array tracking
    arr_item_count: usize,
    // For uniqueness checks
    arr_hashes: HashMap<u64, Option<S>>,
    arr_contains: Option<&'a Schema>,

    // Object tracking
    obj_required: Set<String>,
    obj_prop_count: usize,
    obj_last_key: Option<String>,
    obj_last_key_span: Option<S>,
}

impl<'a, S: Span> SchemaValidator<'a, S> {
    fn new(defs: &'a Map<String, Schema>, schema: SchemaRef<'a>) -> Self {
        Self {
            schema,
            defs,
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
            obj_last_key_span: None,
        }
    }

    fn validate_inner<V: ?Sized + Validate<Span = S>>(
        &mut self,
        value: &V,
    ) -> Result<(), Errors<S>> {
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        let mut value_span = self.combined_span.clone();
        value_span.combine(value.span());

        if let Some(r) = &s.reference {
            match local_definition(r) {
                Some(local) => match self.defs.get(local) {
                    Some(s) => {
                        return SchemaValidator::new(self.defs, s.into())
                            .with_spans(self.parent_span.clone(), value.span())
                            .validate_inner(value)
                    }
                    None => {
                        return Err(Errors::one(Error::new(
                            s.metadata.clone(),
                            value_span.clone(),
                            ErrorValue::InvalidSchema(InvalidSchema::MissingDefinition(
                                local.to_string(),
                            )),
                        )));
                    }
                },
                None => {
                    return Err(Errors::one(Error::new(
                        s.metadata.clone(),
                        value_span.clone(),
                        ErrorValue::InvalidSchema(InvalidSchema::ExternalReference(r.clone())),
                    )));
                }
            }
        }

        let mut errors = self.validate_subschemas(s, value).err();

        if s.instance_type.is_none() {
            return match errors {
                None => Ok(()),
                Some(e) => Err(e),
            };
        }

        if let Err(e) = value.validate(
            SchemaValidator::new(&self.defs, SchemaRef::from(*s))
                .with_spans(self.parent_span.clone(), value.span()),
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

    /// Validate all the allOf anyOf, etc. schemas for a given value.
    fn validate_subschemas<V: ?Sized + Validate<Span = S>>(
        &self,
        schema: &SchemaObject,
        value: &V,
    ) -> Result<(), Errors<S>> {
        if let Some(sub) = &schema.subschemas {
            let mut errors = ErrorsInner::new();

            if let Some(all_of) = &sub.all_of {
                for s in all_of {
                    if let Err(e) = SchemaValidator::new(self.defs, s.into())
                        .with_spans(self.parent_span.clone(), self.span.clone())
                        .validate_inner(value)
                    {
                        errors.extend(e.0.into_iter());
                    }
                }
            }

            if let Some(any_of) = &sub.any_of {
                let mut validated = Vec::with_capacity(any_of.len());
                let mut inner_errors: Vec<Errors<_>> = Vec::with_capacity(any_of.len());
                for s in any_of {
                    match SchemaValidator::new(self.defs, s.into())
                        .with_spans(self.parent_span.clone(), self.span.clone())
                        .validate_inner(value)
                    {
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
                            exclusive: false,
                            schemas: any_of
                                .iter()
                                .map(|s| match s {
                                    Schema::Bool(_) => None,
                                    Schema::Object(o) => o.metadata.clone(),
                                })
                                .collect(),
                            errors: inner_errors,
                        },
                    ));
                }
            }

            if let Some(one_of) = &sub.one_of {
                let mut validated = Vec::with_capacity(one_of.len());
                let mut inner_errors: Vec<Errors<_>> = Vec::with_capacity(one_of.len());
                for s in one_of {
                    match SchemaValidator::new(self.defs, s.into())
                        .with_spans(self.parent_span.clone(), self.span.clone())
                        .validate_inner(value)
                    {
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
                            exclusive: true,
                            schemas: one_of
                                .iter()
                                .map(|s| match s {
                                    Schema::Bool(_) => None,
                                    Schema::Object(o) => o.metadata.clone(),
                                })
                                .collect(),
                            errors: inner_errors,
                        },
                    ));
                } else if validated.len() > 1 {
                    errors.push(Error::new(
                        schema.metadata.clone(),
                        value.span(),
                        ErrorValue::MoreThanOneValid {
                            schemas: one_of
                                .iter()
                                .map(|s| match s {
                                    Schema::Bool(_) => None,
                                    Schema::Object(o) => o.metadata.clone(),
                                })
                                .collect(),
                            matched: validated,
                        },
                    ));
                }
            }

            if let (Some(sub_if), Some(sub_then)) = (&sub.if_schema, &sub.then_schema) {
                if SchemaValidator::new(self.defs, (&**sub_if).into())
                    .with_spans(self.parent_span.clone(), self.span.clone())
                    .validate_inner(value)
                    .is_ok()
                {
                    if let Err(e) = SchemaValidator::new(self.defs, (&**sub_then).into())
                        .with_spans(self.parent_span.clone(), self.span.clone())
                        .validate_inner(value)
                    {
                        errors.extend(e.0.into_iter());
                    }
                } else if let Some(sub_else) = &sub.else_schema {
                    if let Err(e) = SchemaValidator::new(self.defs, (&**sub_else).into())
                        .with_spans(self.parent_span.clone(), self.span.clone())
                        .validate_inner(value)
                    {
                        errors.extend(e.0.into_iter());
                    }
                }
            }

            if let Some(not) = &sub.not {
                if SchemaValidator::new(self.defs, (&**not).into())
                    .with_spans(self.parent_span.clone(), self.span.clone())
                    .validate_inner(value)
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

    fn with_spans(mut self, parent: Option<S>, span: Option<S>) -> Self {
        self.parent_span = parent;
        self.span = span;

        self.combine_spans();

        self
    }

    fn with_parent_span(mut self, span: Option<S>) -> Self {
        self.parent_span = span;
        self.combine_spans();

        self
    }

    fn combine_spans(&mut self) {
        self.combined_span = self.parent_span.clone();
        if self.span.is_some() {
            self.combined_span.combine(self.span.clone());
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
                self.span = Some(s);
            }
            None => {
                self.parent_span = None;
                self.span = None;
            }
        }
        self.combine_spans();

        self
    }

    fn validate_bool(self, v: bool) -> Result<(), Self::Error> {
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Boolean, s, &self.combined_span)?;
        check_enum!(bool, v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_i8(self, v: i8) -> Result<(), Self::Error> {
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Integer, s, &self.combined_span)?;
        check_enum!(int, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_i16(self, v: i16) -> Result<(), Self::Error> {
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Integer, s, &self.combined_span)?;
        check_enum!(int, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_i32(self, v: i32) -> Result<(), Self::Error> {
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Integer, s, &self.combined_span)?;
        check_enum!(int, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_i64(self, v: i64) -> Result<(), Self::Error> {
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Integer, s, &self.combined_span)?;
        check_enum!(int, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_i128(self, v: i128) -> Result<(), Self::Error> {
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Integer, s, &self.combined_span)?;
        check_enum!(int, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_u8(self, v: u8) -> Result<(), Self::Error> {
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Integer, s, &self.combined_span)?;
        check_enum!(int, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_u16(self, v: u16) -> Result<(), Self::Error> {
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Integer, s, &self.combined_span)?;
        check_enum!(int, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_u32(self, v: u32) -> Result<(), Self::Error> {
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Integer, s, &self.combined_span)?;
        check_enum!(int, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_u64(self, v: u64) -> Result<(), Self::Error> {
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Integer, s, &self.combined_span)?;
        check_enum!(int, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_u128(self, v: u128) -> Result<(), Self::Error> {
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Integer, s, &self.combined_span)?;
        check_enum!(int, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_f32(self, v: f32) -> Result<(), Self::Error> {
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Number, s, &self.combined_span)?;
        check_enum!(float, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_f64(self, v: f64) -> Result<(), Self::Error> {
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Number, s, &self.combined_span)?;
        check_enum!(float, v, s, &self.combined_span)?;
        check_number!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_char(self, v: char) -> Result<(), Self::Error> {
        self.validate_str(&v.to_string())
    }

    fn validate_str(self, v: &str) -> Result<(), Self::Error> {
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(String, s, &self.combined_span)?;
        check_enum!(str, v, s, &self.combined_span)?;
        check_string!(v, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_bytes(self, _v: &[u8]) -> Result<(), Self::Error> {
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(String, s, &self.combined_span)?;
        // TODO anything else to check here?
        Ok(())
    }

    fn validate_none(self) -> Result<(), Self::Error> {
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

    fn validate_unit(self) -> Result<(), Self::Error> {
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(Null, s, &self.combined_span)?;

        Ok(())
    }

    fn validate_unit_struct(self, _name: &'static str) -> Result<(), Self::Error> {
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

        self.parent_span = self.combined_span.clone();

        Ok(self)
    }

    fn validate_map(mut self, _len: Option<usize>) -> Result<Self::ValidateMap, Self::Error> {
        let s = not_bool_schema!(self, &self.schema, &self.combined_span);

        check_type!(s, &self.combined_span)?;

        if let Some(obj) = &s.object {
            self.obj_required = obj.required.clone();
        }

        self.parent_span = self.combined_span.clone();

        Ok(self)
    }

    fn validate_tag<V>(&mut self, tag: &V) -> Result<(), Self::Error>
    where
        V: ?Sized + Validate<Span = S> + ToString,
    {
        let s = not_bool_schema!(&self.schema, &self.combined_span);

        check_type!(s, &self.combined_span)?;

        let key = tag.to_string();

        let tag_span = self.parent_span.combined(tag.span());

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
                        tag_span.clone(),
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
                self.span = Some(s);
            }
            None => {
                self.parent_span = None;
                self.span = None;
            }
        }
        self.combine_spans();

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

        let s = not_bool_schema!(&self.schema, &self.combined_span);

        let mut errors = Errors::new();

        let value_span = self.parent_span.combined(value.span());

        if let Some(arr) = &s.array {
            if let Some(c) = self.arr_contains {
                if SchemaValidator::new(&self.defs, c.into())
                    .with_parent_span(self.parent_span.clone())
                    .validate_inner(value)
                    .is_ok()
                {
                    self.arr_contains = None;
                }
            }

            if let Some(items) = &arr.items {
                match items {
                    SingleOrVec::Single(single_schema) => {
                        if let Err(e) = SchemaValidator::new(&self.defs, (&**single_schema).into())
                            .with_parent_span(self.parent_span.clone())
                            .validate_inner(value)
                        {
                            errors.0.extend(e.0.into_iter());
                        }
                    }
                    SingleOrVec::Vec(schemas) => {
                        if let Some(s) = schemas.get(self.arr_item_count - 1) {
                            if let Err(e) = SchemaValidator::new(&self.defs, s.into())
                                .with_parent_span(self.parent_span.clone())
                                .validate_inner(value)
                            {
                                errors.0.extend(e.0.into_iter());
                            }
                        } else if let Some(s) = &arr.additional_items {
                            if let Err(e) = SchemaValidator::new(&self.defs, (&**s).into())
                                .with_parent_span(self.parent_span.clone())
                                .validate_inner(value)
                            {
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

                let existing = self.arr_hashes.insert(h, value_span.clone());

                if let Some(existing_val) = existing {
                    errors.0.push(Error::new(
                        s.metadata.clone(),
                        value_span.clone(),
                        ErrorValue::NotUnique {
                            first: existing_val,
                            duplicate: value_span.clone(),
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

    fn end(self) -> Result<(), Self::Error> {
        if self.tagged_allow {
            return Ok(());
        }

        let s = not_bool_schema!(&self.schema, &self.combined_span);
        let mut errors = Errors::new();

        if let Some(c) = self.arr_contains {
            errors.0.push(Error::new(
                s.metadata.clone(),
                self.combined_span.clone(),
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
                        self.combined_span.clone(),
                        ErrorValue::NotEnoughItems { min: min as usize },
                    ));
                }
            }

            if let Some(max) = arr.max_items {
                if self.arr_item_count > max as usize {
                    errors.0.push(Error::new(
                        s.metadata.clone(),
                        self.combined_span.clone(),
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
                self.span = Some(s);
            }
            None => {
                self.parent_span = None;
                self.span = None;
            }
        }
        self.combine_spans();

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

        let s = not_bool_schema!(&self.schema, &self.combined_span);

        let key_span = self.parent_span.combined(key.span());

        let key_string = key.to_string();

        self.obj_required.remove(&key_string);
        self.obj_last_key = Some(key_string);
        self.obj_last_key_span = key.span();

        if let Some(obj) = &s.object {
            if let Some(name_schema) = &obj.property_names {
                if let Err(e) = SchemaValidator::new(&self.defs, (&**name_schema).into())
                    .with_spans(self.parent_span.clone(), key_span)
                    .validate_inner(key)
                {
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

        let s = not_bool_schema!(&self.schema, &self.combined_span);
        let key = self.obj_last_key.take().expect("no key before value");

        if let Some(obj) = &s.object {
            if let Some(prop_schema) = obj.properties.get(&key) {
                match SchemaValidator::new(&self.defs, prop_schema.into())
                    .with_parent_span(self.parent_span.clone())
                    .validate_inner(value)
                {
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
                    match SchemaValidator::new(&self.defs, v.into())
                        .with_parent_span(self.parent_span.clone())
                        .validate_inner(value)
                    {
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
                if let Err(e) = SchemaValidator::new(&self.defs, (&**add_prop_schema).into())
                    .with_parent_span(self.parent_span.clone())
                    .validate_inner(value)
                {
                    if let ErrorValue::Never = &e.0.get(0).unwrap().value {
                        return Err(Errors::one(Error::new(
                            s.metadata.clone(),
                            self.obj_last_key_span.take(),
                            ErrorValue::UnknownProperty,
                        )));
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Ok(())
    }

    fn end(self) -> Result<(), Self::Error> {
        if self.tagged_allow {
            return Ok(());
        }

        let s = not_bool_schema!(&self.schema, &self.combined_span);
        let mut errors = Errors::new();

        if let Some(obj) = &s.object {
            if let Some(max) = obj.max_properties {
                if self.obj_prop_count > max as usize {
                    errors.0.push(Error::new(
                        s.metadata.clone(),
                        self.combined_span.clone(),
                        ErrorValue::TooManyProperties { max: max as usize },
                    ))
                }
            }

            if let Some(min) = obj.min_properties {
                if self.obj_prop_count < min as usize {
                    errors.0.push(Error::new(
                        s.metadata.clone(),
                        self.combined_span.clone(),
                        ErrorValue::NotEnoughProperties { min: min as usize },
                    ))
                }
            }
        }

        for p in self.obj_required {
            errors.0.push(Error::new(
                s.metadata.clone(),
                self.combined_span.clone(),
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

fn local_definition(path: &str) -> Option<&str> {
    if !path.starts_with("#/definitions/") {
        return None;
    }

    Some(path.trim_start_matches("#/definitions/"))
}
