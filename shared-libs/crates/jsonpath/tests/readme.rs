extern crate jsonpath_lib as jsonpath;
extern crate serde;
#[macro_use]
extern crate imbl_value;
extern crate serde_json;

use imbl_value::imbl::{vector, Vector};
use imbl_value::Value;
use jsonpath::{JsonSelector, JsonSelectorMut, PathParser};
use serde::Deserialize;

mod common;

#[test]
fn readme() {
    let json_obj = json!({
        "store": {
            "book": [
                {
                    "category": "reference",
                    "author": "Nigel Rees",
                    "title": "Sayings of the Century",
                    "price": 8.95
                },
                {
                    "category": "fiction",
                    "author": "Evelyn Waugh",
                    "title": "Sword of Honour",
                    "price": 12.99
                },
                {
                    "category": "fiction",
                    "author": "Herman Melville",
                    "title": "Moby Dick",
                    "isbn": "0-553-21311-3",
                    "price": 8.99
                },
                {
                    "category": "fiction",
                    "author": "J. R. R. Tolkien",
                    "title": "The Lord of the Rings",
                    "isbn": "0-395-19395-8",
                    "price": 22.99
                }
            ],
            "bicycle": {
                "color": "red",
                "price": 19.95
            }
        },
        "expensive": 10
    });

    let mut selector = jsonpath::selector(&json_obj);

    assert_eq!(
        selector("$.store.book[*].author").unwrap(),
        vector![
            &json_obj["store"]["book"][0]["author"],
            &json_obj["store"]["book"][1]["author"],
            &json_obj["store"]["book"][2]["author"],
            &json_obj["store"]["book"][3]["author"]
        ]
    );

    assert_eq!(
        selector("$..author").unwrap(),
        vector![
            &json_obj["store"]["book"][0]["author"],
            &json_obj["store"]["book"][1]["author"],
            &json_obj["store"]["book"][2]["author"],
            &json_obj["store"]["book"][3]["author"]
        ]
    );

    assert_eq!(
        selector("$.store.*").unwrap(),
        vector![&json_obj["store"]["book"], &json_obj["store"]["bicycle"]]
    );

    assert_eq!(
        selector("$.store..price").unwrap(),
        vector![
            &json_obj["store"]["book"][0]["price"],
            &json_obj["store"]["book"][1]["price"],
            &json_obj["store"]["book"][2]["price"],
            &json_obj["store"]["book"][3]["price"],
            &json_obj["store"]["bicycle"]["price"]
        ]
    );

    assert_eq!(
        selector("$..book[2]").unwrap(),
        vector![&json_obj["store"]["book"][2]]
    );

    assert_eq!(
        selector("$..book[-2]").unwrap(),
        vector![&json_obj["store"]["book"][2]]
    );

    assert_eq!(
        selector("$..book[0,1]").unwrap(),
        vector![&json_obj["store"]["book"][0], &json_obj["store"]["book"][1]]
    );

    assert_eq!(
        selector("$..book[:2]").unwrap(),
        vector![&json_obj["store"]["book"][0], &json_obj["store"]["book"][1]]
    );

    assert_eq!(
        selector("$..book[?(@.isbn)]").unwrap(),
        vector![&json_obj["store"]["book"][2], &json_obj["store"]["book"][3]]
    );

    assert_eq!(
        selector("$.store.book[?(@.price < 10)]").unwrap(),
        vector![&json_obj["store"]["book"][0], &json_obj["store"]["book"][2],]
    );
}

#[test]
fn readme_selector() {
    #[derive(Clone, Deserialize, PartialEq, Debug)]
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

    let parser = PathParser::compile("$..[?(@.age >= 30)]").unwrap();
    let mut selector = JsonSelector::new(parser);

    let result = selector.value(&json_obj).select().unwrap();

    assert_eq!(vector![&json_obj["friends"][0]], result);

    let result = selector.select_as_str().unwrap();
    assert_eq!(r#"[{"name":"친구3","age":30}]"#, result);

    let result = selector.select_as::<Friend>().unwrap();
    assert_eq!(
        vec![Friend {
            name: "친구3".to_string(),
            age: Some(30),
        }],
        result
    );
}

#[test]
fn readme_selector_mut() {
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

    let parser = PathParser::compile("$..[?(@.age == 20)].age").unwrap();
    let mut selector_mut = JsonSelectorMut::new(parser);

    let result = selector_mut
        .value(json_obj)
        .replace_with(&mut |v| {
            let age = if let Value::Number(n) = v {
                n.as_u64().unwrap() * 2
            } else {
                0
            };

            Some(json!(age))
        })
        .unwrap()
        .take()
        .unwrap();

    assert_eq!(
        result,
        json!({
            "school": {
                "friends": [
                    {"name": "친구1", "age": 40},
                    {"name": "친구2", "age": 40}
                ]
            },
            "friends": [
                {"name": "친구3", "age": 30},
                {"name": "친구4"}
        ]})
    );
}

#[test]
fn readme_select() {
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

    assert_eq!(
        json,
        vector![&json_obj["friends"][0], &json_obj["school"]["friends"][0]]
    );
}

#[test]
fn readme_select_as_str() {
    let ret = jsonpath::select_as_str(
        r#"
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
    "#,
        "$..friends[0]",
    )
    .unwrap();

    assert_eq!(
        ret,
        r#"[{"name":"친구3","age":30},{"name":"친구1","age":20}]"#
    );
}

#[test]
fn readme_select_as() {
    #[derive(Deserialize, PartialEq, Debug)]
    struct Person {
        name: String,
        age: u8,
        phones: Vector<String>,
    }

    let ret: Vec<Person> = jsonpath::select_as(
        r#"{
                    "person":
                        {
                            "name": "Doe John",
                            "age": 44,
                            "phones": [
                                "+44 1234567",
                                "+44 2345678"
                            ]
                        }
                }"#,
        "$.person",
    )
    .unwrap();

    let person = Person {
        name: "Doe John".to_string(),
        age: 44,
        phones: vector!["+44 1234567".to_string(), "+44 2345678".to_string()],
    };

    assert_eq!(ret[0], person);
}

#[test]
fn readme_compile() {
    let first_firend = jsonpath::PathCompiled::compile("$..friends[0]").unwrap();

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

    let json = first_firend.select(&json_obj).unwrap();

    assert_eq!(
        json,
        vector![&json_obj["friends"][0], &json_obj["school"]["friends"][0]]
    );
}

#[test]
fn readme_selector_fn() {
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

    assert_eq!(
        json,
        vector![&json_obj["friends"][0], &json_obj["school"]["friends"][0]]
    );

    let json = selector("$..friends[1]").unwrap();

    assert_eq!(
        json,
        vector![&json_obj["friends"][1], &json_obj["school"]["friends"][1]]
    );
}

#[test]
fn readme_selector_as() {
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

    #[derive(Clone, Deserialize, PartialEq, Debug)]
    struct Friend {
        name: String,
        age: Option<u8>,
    }

    let mut selector = jsonpath::selector_as::<Friend>(&json_obj);

    let json = selector("$..friends[0]").unwrap();

    let ret = vec![
        Friend {
            name: "친구3".to_string(),
            age: Some(30),
        },
        Friend {
            name: "친구1".to_string(),
            age: Some(20),
        },
    ];
    assert_eq!(json, ret);

    let json = selector("$..friends[1]").unwrap();

    let ret = vec![
        Friend {
            name: "친구4".to_string(),
            age: None,
        },
        Friend {
            name: "친구2".to_string(),
            age: Some(20),
        },
    ];

    assert_eq!(json, ret);
}

#[test]
fn readme_delete() {
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

    assert_eq!(
        ret,
        json!({
            "school": {
                "friends": [
                    null,
                    null
                ]
            },
            "friends": [
                {"name": "친구3", "age": 30},
                {"name": "친구4"}
        ]})
    );
}

#[test]
fn readme_delete2() {
    let json_obj = common::read_json("./benchmark/example.json");

    let ret = jsonpath::delete(json_obj, "$.store.book").unwrap();

    assert_eq!(
        ret,
        json!({
            "store": {
                "book": null,
                "bicycle": {
                    "color": "red",
                    "price": 19.95
                }
            },
            "expensive": 10
        })
    );
}

#[test]
fn readme_replace_with() {
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

    let result = jsonpath::replace_with(json_obj, "$..[?(@.age == 20)].age", &mut |v| {
        let age = if let Value::Number(n) = v {
            n.as_u64().unwrap() * 2
        } else {
            0
        };

        Some(json!(age))
    })
    .unwrap();

    assert_eq!(
        result,
        json!({
            "school": {
                "friends": [
                    {"name": "친구1", "age": 40},
                    {"name": "친구2", "age": 40}
                ]
            },
            "friends": [
                {"name": "친구3", "age": 30},
                {"name": "친구4"}
        ]})
    );
}
