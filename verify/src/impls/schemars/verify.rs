use crate::{span::Keys, Verify};
use schemars_crate::{
    schema::{RootSchema, Schema, SchemaObject},
    Map,
};

use super::errors::{Error, ErrorValue, Errors, InvalidSchema};

impl Verify for RootSchema {
    type Error = Errors<Keys>;

    fn verify(&self) -> Result<(), Self::Error> {
        let mut errors = Errors::new();

        for (k, s) in &self.definitions {
            if let Err(err) = verify_schema(s, &self.definitions, Keys::new() + "definitions" + k) {
                errors += err;
            }
        }

        if let Err(err) = verify_schema_object(&self.schema, &self.definitions, Keys::new()) {
            errors += err;
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

fn verify_schema(
    schema: &Schema,
    definitions: &Map<String, Schema>,
    span: Keys,
) -> Result<(), Errors<Keys>> {
    match schema {
        Schema::Bool(_) => Ok(()),
        Schema::Object(o) => verify_schema_object(o, definitions, span),
    }
}

fn verify_schema_object(
    schema: &SchemaObject,
    definitions: &Map<String, Schema>,
    span: Keys,
) -> Result<(), Errors<Keys>> {
    let mut errors = Errors::new();

    if let Some(r) = &schema.reference {
        match local_definition(r) {
            Some(s) => {
                if definitions.get(s).is_none() {
                    errors.0.push(Error::new(
                        None,
                        Some(span.clone()),
                        ErrorValue::InvalidSchema(InvalidSchema::MissingDefinition(s.to_string())),
                    ));
                    return Err(errors);
                }
            }
            None => {
                errors.0.push(Error::new(
                    None,
                    Some(span.clone()),
                    ErrorValue::InvalidSchema(InvalidSchema::ExternalReference(r.clone())),
                ));
                return Err(errors);
            }
        }
    }

    if let Some(subs) = &schema.subschemas {
        if let Some(all_ofs) = &subs.all_of {
            for (i, s) in all_ofs.iter().enumerate() {
                if let Err(err) = verify_schema(s, definitions, span.clone() + "allOf" + i) {
                    errors += err;
                }
            }
        }

        if let Some(one_ofs) = &subs.one_of {
            for (i, s) in one_ofs.iter().enumerate() {
                if let Err(err) = verify_schema(s, definitions, span.clone() + "oneOf" + i) {
                    errors += err;
                }
            }
        }

        if let Some(any_ofs) = &subs.any_of {
            for (i, s) in any_ofs.iter().enumerate() {
                if let Err(err) = verify_schema(s, definitions, span.clone() + "anyOf" + i) {
                    errors += err;
                }
            }
        }

        if let Some(s) = &subs.not {
            if let Err(err) = verify_schema(s, definitions, span.clone() + "not") {
                errors += err;
            }
        }

        if let Some(s) = &subs.if_schema {
            if let Err(err) = verify_schema(s, definitions, span.clone() + "if") {
                errors += err;
            }
        }

        if let Some(s) = &subs.then_schema {
            if let Err(err) = verify_schema(s, definitions, span.clone() + "then") {
                errors += err;
            }
        }

        if let Some(s) = &subs.else_schema {
            if let Err(err) = verify_schema(s, definitions, span.clone() + "else") {
                errors += err;
            }
        }
    }

    if let Some(o) = &schema.object {
        for (k, _) in &o.pattern_properties {
            if let Err(error) = regex::Regex::new(k) {
                errors.0.push(Error::new(
                    None,
                    Some(span.clone() + "patternProperties" + k),
                    ErrorValue::InvalidSchema(InvalidSchema::InvalidPattern {
                        pattern: k.clone(),
                        error,
                    }),
                ));
            }
        }

        for (k, s) in &o.properties {
            if let Err(err) = verify_schema(s, definitions, span.clone() + "properties" + k) {
                errors += err;
            }
        }

        if let Some(s) = &o.additional_properties {
            if let Err(err) = verify_schema(s, definitions, span.clone() + "additionalProperties") {
                errors += err;
            }
        }
    }

    if let Some(st) = &schema.string {
        if let Some(p) = &st.pattern {
            if let Err(error) = regex::Regex::new(&p) {
                errors.0.push(Error::new(
                    None,
                    Some(span.clone() + "pattern"),
                    ErrorValue::InvalidSchema(InvalidSchema::InvalidPattern {
                        pattern: p.clone(),
                        error,
                    }),
                ));
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn local_definition(path: &str) -> Option<&str> {
    if !path.starts_with("#/definitions/") {
        return None;
    }

    Some(path.trim_start_matches("#/definitions/"))
}
