# jsonpath_lib

[![Build Status](https://travis-ci.org/freestrings/jsonpath.svg?branch=master)](https://travis-ci.org/freestrings/jsonpath)
![crates.io](https://img.shields.io/crates/v/jsonpath_lib.svg)
![Codecov](https://img.shields.io/codecov/c/github/freestrings/jsonpath.svg?token=92c41b4e7cf04a9cbebc08f68c5da615)

A [JsonPath](https://goessner.net/articles/JsonPath/) engine written in `Rust`.

## Place in the monorepo

This is a **first-party crate** in the start-technologies monorepo. It originated as a fork of
[freestrings/jsonpath](https://github.com/freestrings/jsonpath) and has since fully diverged — it is
maintained here as first-party code, with no intent to sync back upstream.

- **Path:** `shared-libs/crates/jsonpath`
- **Package name:** `jsonpath_lib` (differs from the directory name `jsonpath`; build/test with
  `cargo … -p jsonpath_lib`).
- **Crate type:** `rlib` + `cdylib` (the cdylib exports the deprecated C FFI in `src/ffi`).
- **Consumers:** `start-core` uses `jsonpath_lib::Compiled` in `src/config/hook.rs`.
- **Dependencies:** the sibling [`imbl-value`](../imbl-value) crate (supplies the `Value`/`Vector`
  types this engine queries), plus `serde` / `serde_json` and `log`. It is a direct path
  dependency — no `[patch]`, no crates.io pull.

## Rust API

<details><summary><b>jsonpath_lib crate</b></summary>

Use the in-tree `jsonpath_lib` path dependency; the upstream crates.io release is a different implementation.

```rust
extern crate jsonpath_lib as jsonpath;
```

</details>

<details><summary><b>Rust - jsonpath::Selector struct</b></summary>

```rust
#[derive(Deserialize, PartialEq, Debug)]
struct Friend {
    name: String,
    age: Option<u8>,
}

let json_obj = json!({
    "school": {
        "friends": [
            {"name": "친구1", "age": 20},
            {"name": "친구2", "age": 20}
        ]
    },
    "friends": [
        {"name": "친구3", "age": 30},
        {"name": "친구4"}
]});

let mut selector = Selector::new();

let result = selector
    .path("$..[?(@.age >= 30)]").unwrap()
    .value(&json_obj)
    .select().unwrap();

assert_eq!(vector![&json!({"name": "친구3", "age": 30})], result);

let result = selector.select_as_str().unwrap();
assert_eq!(r#"[{"name":"친구3","age":30}]"#, result);

let result = selector.select_as::<Friend>().unwrap();
assert_eq!(vector![Friend { name: "친구3".to_string(), age: Some(30) }], result);
```

</details>

<details><summary><b>Rust - jsonpath::SelectorMut struct</b></summary>

```rust
let json_obj = json!({
    "school": {
        "friends": [
            {"name": "친구1", "age": 20},
            {"name": "친구2", "age": 20}
        ]
    },
    "friends": [
        {"name": "친구3", "age": 30},
        {"name": "친구4"}
]});

let mut selector_mut = SelectorMut::new();

let result = selector_mut
    .str_path("$..[?(@.age == 20)].age").unwrap()
    .value(json_obj)
    .replace_with(&mut |v| {
        let age = if let Value::Number(n) = v {
            n.as_u64().unwrap() * 2
        } else {
            0
        };

        Some(json!(age))
    }).unwrap()
    .take().unwrap();

assert_eq!(result, json!({
    "school": {
        "friends": [
            {"name": "친구1", "age": 40},
            {"name": "친구2", "age": 40}
        ]
    },
    "friends": [
        {"name": "친구3", "age": 30},
        {"name": "친구4"}
]}));
```

</details>

<details><summary><b>Rust - jsonpath::select(json: &imbl_value::value::Value, jsonpath: &str)</b></summary>

```rust
let json_obj = json!({
    "school": {
        "friends": [
            {"name": "친구1", "age": 20},
            {"name": "친구2", "age": 20}
        ]
    },
    "friends": [
        {"name": "친구3", "age": 30},
        {"name": "친구4"}
]});

let json = jsonpath::select(&json_obj, "$..friends[0]").unwrap();

assert_eq!(json, vector![
    &json!({"name": "친구3", "age": 30}),
    &json!({"name": "친구1", "age": 20})
]);
```

</details>

<details><summary><b>Rust - jsonpath::select_as_str(json_str: &str, jsonpath: &str)</b></summary>

```rust
let ret = jsonpath::select_as_str(r#"
{
    "school": {
        "friends": [
                {"name": "친구1", "age": 20},
                {"name": "친구2", "age": 20}
            ]
    },
    "friends": [
        {"name": "친구3", "age": 30},
        {"name": "친구4"}
    ]
}
"#, "$..friends[0]").unwrap();

assert_eq!(ret, r#"[{"name":"친구3","age":30},{"name":"친구1","age":20}]"#);
```

</details>

<details><summary><b>Rust - jsonpath::select_as&lt;T: `serde::de::DeserializeOwned`&gt;(json_str: &str, jsonpath: &str)</b></summary>

```rust
#[derive(Deserialize, PartialEq, Debug)]
struct Person {
    name: String,
    age: u8,
    phones: Vector<String>,
}

let ret: Vector<Person> = jsonpath::select_as(r#"
{
    "person":
        {
            "name": "Doe John",
            "age": 44,
            "phones": [
                "+44 1234567",
                "+44 2345678"
            ]
        }
}
"#, "$.person").unwrap();

let person = Person {
    name: "Doe John".to_string(),
    age: 44,
    phones: vector!["+44 1234567".to_string(), "+44 2345678".to_string()],
};

assert_eq!(ret[0], person);
```

</details>

<details><summary><b>Rust - jsonpath::PathCompiled::compile(jsonpath: &str)</b></summary>

```rust
let template = jsonpath::PathCompiled::compile("$..friends[0]").unwrap();

let json_obj = json!({
    "school": {
        "friends": [
            {"name": "친구1", "age": 20},
            {"name": "친구2", "age": 20}
        ]
    },
    "friends": [
        {"name": "친구3", "age": 30},
        {"name": "친구4"}
]});

let json = template.select(&json_obj).unwrap();

assert_eq!(json, vector![
    &json!({"name": "친구3", "age": 30}),
    &json!({"name": "친구1", "age": 20})
]);
```

</details>

<details><summary><b>Rust - jsonpath::selector(json: &imbl_value::value::Value)</b></summary>

```rust
let json_obj = json!({
    "school": {
        "friends": [
            {"name": "친구1", "age": 20},
            {"name": "친구2", "age": 20}
        ]
    },
    "friends": [
        {"name": "친구3", "age": 30},
        {"name": "친구4"}
]});

let mut selector = jsonpath::selector(&json_obj);

let json = selector("$..friends[0]").unwrap();

assert_eq!(json, vector![
    &json!({"name": "친구3", "age": 30}),
    &json!({"name": "친구1", "age": 20})
]);

let json = selector("$..friends[1]").unwrap();

assert_eq!(json, vector![
    &json!({"name": "친구4"}),
    &json!({"name": "친구2", "age": 20})
]);
```

</details>

<details><summary><b>Rust - jsonpath::selector_as&lt;T: serde::de::DeserializeOwned&gt;(json: &imbl_value::value::Value)</b></summary>

```rust
let json_obj = json!({
    "school": {
       "friends": [
            {"name": "친구1", "age": 20},
            {"name": "친구2", "age": 20}
        ]
    },
    "friends": [
        {"name": "친구3", "age": 30},
        {"name": "친구4"}
]});

#[derive(Deserialize, PartialEq, Debug)]
struct Friend {
    name: String,
    age: Option<u8>,
}

let mut selector = jsonpath::selector_as::<Friend>(&json_obj);

let json = selector("$..friends[0]").unwrap();

let ret = vector!(
    Friend { name: "친구3".to_string(), age: Some(30) },
    Friend { name: "친구1".to_string(), age: Some(20) }
);
assert_eq!(json, ret);

let json = selector("$..friends[1]").unwrap();

let ret = vector!(
    Friend { name: "친구4".to_string(), age: None },
    Friend { name: "친구2".to_string(), age: Some(20) }
);

assert_eq!(json, ret);
```

</details>

<details><summary><b>Rust - jsonpath::delete(value: &Value, path: &str)</b></summary>

```rust
let json_obj = json!({
    "school": {
        "friends": [
            {"name": "친구1", "age": 20},
            {"name": "친구2", "age": 20}
        ]
    },
    "friends": [
        {"name": "친구3", "age": 30},
        {"name": "친구4"}
]});

let ret = jsonpath::delete(json_obj, "$..[?(20 == @.age)]").unwrap();

assert_eq!(ret, json!({
    "school": {
        "friends": [
            null,
            null
        ]
    },
    "friends": [
        {"name": "친구3", "age": 30},
        {"name": "친구4"}
]}));
```

</details>

<details><summary><b>Rust - jsonpath::replace_with&lt;F: FnMut(&Value) -> Value&gt;(value: &Value, path: &str, fun: &mut F)</b></summary>

```rust
let json_obj = json!({
    "school": {
        "friends": [
            {"name": "친구1", "age": 20},
            {"name": "친구2", "age": 20}
        ]
    },
    "friends": [
        {"name": "친구3", "age": 30},
        {"name": "친구4"}
]});

let ret = jsonpath::replace_with(json_obj, "$..[?(@.age == 20)].age", &mut |v| {
    let age = if let Value::Number(n) = v {
        n.as_u64().unwrap() * 2
    } else {
        0
    };

    Some(json!(age))
}).unwrap();

assert_eq!(ret, json!({
    "school": {
        "friends": [
            {"name": "친구1", "age": 40},
            {"name": "친구2", "age": 40}
        ]
    },
    "friends": [
        {"name": "친구3", "age": 30},
        {"name": "친구4"}
]}));
```

</details>

[Rust - Other Examples](https://github.com/freestrings/jsonpath/wiki/rust-examples)

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) — how it's built (parse → walk flow, public API, place in the
  monorepo).
- [AGENTS.md](AGENTS.md) — toolchain, build/test/format, conventions.

Licensed under MIT — see [LICENSE](LICENSE). Originally written by Changseok Han as
[freestrings/jsonpath](https://github.com/freestrings/jsonpath), Copyright (c) 2019 Changseok Han.
