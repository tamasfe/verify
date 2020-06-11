// TODO: proper tests
use schemars_crate::{self as schemars, schema::RootSchema, JsonSchema};
use serde::Serialize;
use serde_json::json;
use verify::{
    serde::{KeySpans, Spanned},
    Verifier, Verify,
};

#[derive(Default, Verify, Serialize, JsonSchema)]
#[verify(schemars, serde)]
struct SomeStruct {
    some_value: i32,
}

#[derive(Default, Verify, Serialize, JsonSchema)]
#[verify(
    serde(spans = "verify::serde::KeySpans"),
    verifier(
        name = "schemars::schema::RootSchema",
        create = "schemars::schema_for!(SomeStructExplicit)"
    )
)]
struct SomeStructExplicit {
    some_value: i32,
}

#[test]
fn test_derive() {
    let some_struct = SomeStruct::default();
    assert!(some_struct.verify().is_ok());

    let some_struct_explicit = SomeStructExplicit::default();
    assert!(some_struct_explicit.verify().is_ok());
}


#[test]
fn test_verify() {
    let schema_value = json! {
        {
            "$schema": "http://json-schema.org/draft-07/schema#",
            "title": "SomeStruct",
            "type": "object",
            "required": [
              "some_inner",
              "some_int"
            ],
            "additionalProperties": false,
            "properties": {
              "some_str": {
                "type": "string"
              },
              "some_inner": {
                "type": "object",
                "required": [
                  "inner_values",
                  "inner_value"
                ],
                "properties": {
                  "inner_values": {
                    "type": "array",
                    "maxItems": 2,
                    "items": {
                        "type": "string"
                    }
                  },
                  "inner_value": {
                    "type": ["number", "string"],
                    "enum": [1, "value"]
                  }
                }
              },
              "some_int": {
                "type": "integer",
                "format": "int32"
              }
            }
          }
    };

    let value = json! {
        {
            "some_str": false,
            "some_inner": {
              "inner_value": 1.0,
              "inner_values": ["value", 2]
            },
            "unexpected_property": 2
        }
    };

    let schema = serde_json::from_value::<RootSchema>(schema_value).unwrap();
    let validation_result = schema.verify_value(&Spanned::new(&value, KeySpans::default()));

    if let Err(errors) = validation_result {
        for error in errors {
            println!(
                "({span}) {err}",
                span = error.span.map(|s| s.dotted()).unwrap_or_default(),
                err = error.value
            )
        }
    }
}


#[test]
fn test_self_verify() {
    let schema_value = json! {
        {
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
              "invalid_string": {
                "type": "string",
                "pattern": "[[[[\\"
              },
              "missing_local": {
                "$ref": "#/definitions/Missing"
              },
              "external_ref": {
                "$ref": "http://example.com/schema.json#/definitions/Something"
              }
            },
          }
    };

    let schema = serde_json::from_value::<RootSchema>(schema_value).unwrap();
    let validation_result = schema.verify();

    if let Err(errors) = validation_result {
        for error in errors {
            println!(
                "({span}) {err}",
                span = error.span.map(|s| s.dotted()).unwrap_or_default(),
                err = error.value
            )
        }
    }
}
