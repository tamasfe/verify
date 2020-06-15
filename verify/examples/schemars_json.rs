/*!
This example validates a value parsed from JSON against a schema
that was also parsed from JSON.
*/

use schemars_crate::schema::RootSchema;
use serde_json::json;
use verify::{serde::KeySpans, serde::Spanned, Verifier};

fn main() {
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
                    "type": "integer",
                    "enum": [1, 3]
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
            "some_inner": {
              "inner_value": 2,
              "inner_values": ["value", 2]
            },
            "unexpected_property": 2
        }
    };

    let schema = serde_json::from_value::<RootSchema>(schema_value).unwrap();
    let valid = schema.verify_value(&Spanned::new(&value, KeySpans::default()));

    if let Err(errors) = valid {
        for error in errors {
            println!(
                "({span}) {err}",
                span = error.span.map(|s| s.dotted()).unwrap_or_default(),
                err = error.value
            )
        }
    }
    // (some_inner.inner_value) invalid enum value, expected to be one of {1, 3}
    // (some_inner.inner_values.1) invalid type, expected "String"
    // (unexpected_property) value is not allowed here
    // () the required property "some_int" is missing
}
