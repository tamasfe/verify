use schemars_crate::{self as schemars, JsonSchema};
use serde::Serialize;
use verify::Verify;

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
fn test_verify() {
    let some_struct = SomeStruct::default();
    assert!(some_struct.verify().is_ok());

    let some_struct_explicit = SomeStructExplicit::default();
    assert!(some_struct_explicit.verify().is_ok());
}
