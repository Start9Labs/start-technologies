# jsonpath_lib

[![Build Status](https://travis-ci.org/freestrings/jsonpath.svg?branch=master)](https://travis-ci.org/freestrings/jsonpath)
![crates.io](https://img.shields.io/crates/v/jsonpath_lib.svg)
![npm](https://img.shields.io/npm/v/jsonpath-wasm.svg?label=npm%20%60jsonpath-wasm%60)
![Codecov](https://img.shields.io/codecov/c/github/freestrings/jsonpath.svg?token=92c41b4e7cf04a9cbebc08f68c5da615)

`Rust` 버전 [JsonPath](https://goessner.net/articles/JsonPath/) 구현으로
`Webassembly`와 `Javascript`에서도 유사한 API 인터페이스를 제공 한다.

It is JsonPath [JsonPath](https://goessner.net/articles/JsonPath/) engine
written in `Rust`. it provide a similar API interface in `Webassembly`
and`Javascript` too.

- [Webassembly Demo](https://freestrings.github.io/jsonpath/)
- [NPM jsonpath-wasm - webassembly](https://www.npmjs.com/package/jsonpath-wasm)

## Place in the monorepo

This is a **first-party crate** in the start-technologies monorepo. It originated as a fork of
[freestrings/jsonpath](https://github.com/freestrings/jsonpath) and has since fully diverged — it is
maintained here as first-party code, with no intent to sync back upstream.

- **Path:** `shared-libs/crates/jsonpath`
- **Package name:** `jsonpath_lib` (differs from the directory name `jsonpath`; build/test with
  `cargo … -p jsonpath_lib`).
- **Crate type:** `rlib` + `cdylib` (the cdylib backs the `wasm/` bindings).
- **Consumers:** `start-core` uses `jsonpath_lib::Compiled` in `src/config/hook.rs`.
- **Dependencies:** the sibling [`imbl-value`](../imbl-value) crate (supplies the `Value`/`Vector`
  types this engine queries), plus `serde` / `serde_json` and `log`. It is a direct path
  dependency — no `[patch]`, no crates.io pull.

## Rust API

<details><summary><b>jsonpath_lib crate</b></summary>

Go to [`jsonpath_lib` creates.io](https://crates.io/crates/jsonpath_lib)

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

## Javascript API

<details><summary><b>npm package</b></summary>

##### jsonpath-wasm

Goto [`jsonpath-wasm` npmjs.org](https://www.npmjs.com/package/jsonpath-wasm)

```javascript
// browser
import * as jsonpath from 'jsonpath-wasm'
// NodeJs
const jsonpath = require('jsonpath-wasm')
```

##### jsonpath-wasm

`wasm-bindgen` 리턴 타입 제약 때문에 빌더 패턴은 지원하지 않는다.

It does not support `builder-pattern` due to the `return type` restriction of
`wasm-bindgen`.

```javascript
let jsonObj = {
  school: {
    friends: [
      { name: '친구1', age: 20 },
      { name: '친구2', age: 20 },
    ],
  },
  friends: [{ name: '친구3', age: 30 }, { name: '친구4' }],
}

let ret = [
  { name: '친구3', age: 30 },
  { name: '친구1', age: 20 },
]

let selector = new jsonpath.Selector()
selector.path('$..friends[0]')
selector.value(jsonObj)

let retObj = selector.select()

console.log(JSON.stringify(ret) == JSON.stringify(retObj))

// => true
```

빌더 패턴 제약은 `Selector class`와 동일하다.

```javascript
let jsonObj = {
  school: {
    friends: [
      { name: '친구1', age: 20 },
      { name: '친구2', age: 20 },
    ],
  },
  friends: [{ name: '친구3', age: 30 }, { name: '친구4' }],
}

let selector = new jsonpath.SelectorMut()
selector.path('$..[?(@.age == 20)]')

{
  selector.value(jsonObj)
  selector.deleteValue()

  let resultObj = {
    school: { friends: [null, null] },
    friends: [{ name: '친구3', age: 30 }, { name: '친구4' }],
  }
  console.log(JSON.stringify(selector.take()) !== JSON.stringify(resultObj))

  // => true
}

{
  selector.value(jsonObj)
  selector.replaceWith(v => {
    v.age = v.age * 2
    return v
  })

  let resultObj = {
    school: {
      friends: [
        { name: '친구1', age: 40 },
        { name: '친구2', age: 40 },
      ],
    },
    friends: [{ name: '친구3', age: 30 }, { name: '친구4' }],
  }
  console.log(JSON.stringify(selector.take()) !== JSON.stringify(resultObj))

  // => true
}
```

</details>

<details><summary><b>Javascript - jsonpath.select(json: string|object, jsonpath: string)</b></summary>

```javascript
let jsonObj = {
  school: {
    friends: [
      { name: '친구1', age: 20 },
      { name: '친구2', age: 20 },
    ],
  },
  friends: [{ name: '친구3', age: 30 }, { name: '친구4' }],
}

let ret = [
  { name: '친구3', age: 30 },
  { name: '친구1', age: 20 },
]

let selectAsString = jsonpath.select(JSON.stringify(jsonObj), '$..friends[0]')
let selectAsObj = jsonpath.select(jsonObj, '$..friends[0]')

console.log(JSON.stringify(ret) == JSON.stringify(selectAsString), JSON.stringify(ret) == JSON.stringify(selectAsObj))

// => true, true
```

</details>

<details><summary><b>Javascript - jsonpath.compile(jsonpath: string)</b></summary>

```javascript
let error = jsonpath.compile('')
console.log(typeof error, error) //string 'path error'

let template = jsonpath.compile('$..friends[0]')

let jsonObj = {
  school: {
    friends: [
      { name: '친구1', age: 20 },
      { name: '친구2', age: 20 },
    ],
  },
  friends: [{ name: '친구3', age: 30 }, { name: '친구4' }],
}

let ret = [
  { name: '친구3', age: 30 },
  { name: '친구1', age: 20 },
]

let selectAsString = template(JSON.stringify(jsonObj))
let selectAsObj = template(jsonObj)

console.log(JSON.stringify(ret) == JSON.stringify(selectAsString), JSON.stringify(ret) == JSON.stringify(selectAsObj))

// => true, true

let jsonObj2 = {
  school: {
    friends: [{ name: 'Millicent Norman' }, { name: 'Vincent Cannon' }],
  },
  friends: [{ age: 30 }, { age: 40 }],
}

let ret2 = [{ age: 30 }, { name: 'Millicent Norman' }]

let selectAsString2 = template(JSON.stringify(jsonObj2))
let selectAsObj2 = template(jsonObj2)

console.log(JSON.stringify(ret2) == JSON.stringify(selectAsString2), JSON.stringify(ret2) == JSON.stringify(selectAsObj2))

// => true, true
```

</details>

<details><summary><b>Javascript - jsonpath.selector(json: string|object)</b></summary>

```javascript
let jsonObj = {
  school: {
    friends: [
      { name: '친구1', age: 20 },
      { name: '친구2', age: 20 },
    ],
  },
  friends: [{ name: '친구3', age: 30 }, { name: '친구4' }],
}

let ret1 = [
  { name: '친구3', age: 30 },
  { name: '친구1', age: 20 },
]

let ret2 = [{ name: '친구4' }, { name: '친구2', age: 20 }]

let selector = jsonpath.selector(jsonObj)
// or as json string
// let selector = jsonpath.selector(JSON.stringify(jsonObj));

let select1 = selector('$..friends[0]')
let select2 = selector('$..friends[1]')

console.log(JSON.stringify(ret1) == JSON.stringify(select1), JSON.stringify(ret2) == JSON.stringify(select2))

// => true, true
```

</details>

<details><summary><b>Javascript - jsonpath.deleteValue(json: string|object, path: string)</b></summary>

```javascript
let jsonObj = {
  school: {
    friends: [
      { name: '친구1', age: 20 },
      { name: '친구2', age: 20 },
    ],
  },
  friends: [{ name: '친구3', age: 30 }, { name: '친구4' }],
}

let _1 = jsonpath.deleteValue(jsonObj, '$..friends[0]')
let result = jsonpath.deleteValue(_1, '$..friends[1]')

console.log(
  JSON.stringify(result) !==
    JSON.stringify({
      school: { friends: [null, null] },
      friends: [null, null],
    }),
)

// => true
```

</details>

<details><summary><b>Javascript - jsonpath.replaceWith(json: string|object, path: string, fun: function(json: object) => json: object</b></summary>

```javascript
let jsonObj = {
  school: {
    friends: [
      { name: '친구1', age: 20 },
      { name: '친구2', age: 20 },
    ],
  },
  friends: [{ name: '친구3', age: 30 }, { name: '친구4' }],
}

let result = jsonpath.replaceWith(jsonObj, '$..friends[0]', v => {
  v.age = v.age * 2
  return v
})

console.log(
  JSON.stringify(result) ===
    JSON.stringify({
      school: {
        friends: [
          { name: '친구1', age: 40 },
          { name: '친구2', age: 20 },
        ],
      },
      friends: [{ name: '친구3', age: 60 }, { name: '친구4' }],
    }),
)

// => true
```

</details>

[Javascript - Other Examples](https://github.com/freestrings/jsonpath/wiki/Javascript-examples)

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) — how it's built (parse → walk flow, public API, place in the
  monorepo).
- [CONTRIBUTING.md](CONTRIBUTING.md) — toolchain, build/test/format, conventions.
- [AGENTS.md](AGENTS.md) — AI/dev operating rules and layout map (`CLAUDE.md` is a one-line
  `@AGENTS.md` import).

Licensed under MIT — see [LICENSE](LICENSE).
