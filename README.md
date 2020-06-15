- [Verify](#verify)
  - [Basic Usage](#basic-usage)
  - [Cargo Features](#cargo-features)
    - [serde](#serde)
    - [smallvec](#smallvec)
    - [schemars](#schemars)

# Verify

[![Latest Version](https://img.shields.io/crates/v/verify.svg)](https://crates.io/crates/verify)
[![Documentation](https://docs.rs/verify/badge.svg)](https://docs.rs/verify)

Verify is yet another validation library for Rust. Its main concept consists of validators that can validate values of any kind of structure. The idea is based on [Serde](https://github.com/serde-rs/serde)'s model, and there is even an optional wrapper for Serde-compatible types. 

## Basic Usage

The library itself without features doesn't do much, it only provides trait definitions and common types.

In order to use it you need to write or find a validator, or enable one of the implementation features of the library.
There is official support only for [Schemars](https://github.com/GREsau/schemars) at the moment.

This very basic example shows how to create a self-validating type with Verify and Schemars:

(_Schemars doesn't yet provide a way to add additional rules during derive, but will in the future._)

```rust
#[derive(Default, Verify, Serialize, JsonSchema)]
#[verify(schemars, serde)]
struct ExampleStruct {
    example_value: i32,
}

fn main() {
    let example = ExampleStruct::default();
    
    // This will always return Ok(())
    assert!(example.verify().is_ok());
}

```

There are quite a few things happening behind the scenes. For more details, visit the [documentation](https://docs.rs/verify).

## Cargo Features

By default only the `smallvec` feature is enabled.

### serde

[Serde](https://github.com/serde-rs/serde) integration, it enables validating any value that implements [Serialize](https://docs.serde.rs/serde/trait.Serialize.html).

### smallvec

Use [smallvec](https://github.com/servo/rust-smallvec) for some types instead of `Vec` in the library.

### schemars

Enable [Schemars](https://github.com/GREsau/schemars) integration by implementing `Validator`, `Verifier` and `Verify` for its schema types.
