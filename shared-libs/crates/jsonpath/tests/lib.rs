extern crate jsonpath_lib as jsonpath;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate imbl_value;

use common::{compare_result, read_contents, read_json, setup};
use imbl_value::imbl::{vector, Vector};
use imbl_value::Value;
use jsonpath::JsonPathError;
use serde::Deserialize;

mod common;

#[test]
fn compile() {
    let compile_object = |path| {
        let template = jsonpath::PathCompiled::compile(path).unwrap();
        let json_obj = read_json("./benchmark/data_obj.json");
        let json = template.select(&json_obj).unwrap();
        let ret = json!([
            {"id": 2,"name": "Gray Berry"},
            {"id": 2,"name": "Gray Berry"}
        ]);
        compare_result(json, ret);
    };

    let compile_array = |path| {
        let template = jsonpath::PathCompiled::compile(path).unwrap();
        let json_obj = read_json("./benchmark/data_array.json");
        let json = template.select(&json_obj).unwrap();
        let ret = json!([
            {"id": 2,"name": "Gray Berry"},
            {"id": 2,"name": "Rosetta Erickson"}
        ]);
        compare_result(json, ret);
    };

    fn compile_error() {
        #[allow(deprecated)]
        let template = jsonpath::Compiled::compile("$[");
        assert!(template.is_err());
    }

    setup();

    compile_object("$..friends[2]");
    compile_array("$..friends[2]");
    compile_error();
}

#[test]
fn selector() {
    setup();

    fn select<'a, F>(selector: &mut F, path: &'a str, target: Value)
    where
        F: FnMut(&'a str) -> Result<Vector<&Value>, JsonPathError>,
    {
        let json = selector(path).unwrap();
        compare_result(json, target);
    }

    let json_obj = read_json("./benchmark/data_obj.json");
    let mut selector = jsonpath::selector(&json_obj);

    select(
        &mut selector,
        "$..friends[2]",
        json!([
            {"id": 2,"name": "Gray Berry"},
            {"id": 2,"name": "Gray Berry"}
        ]),
    );
    select(
        &mut selector,
        "$..friends[0]",
        json!([
            {"id": 0},
            {"id": 0,"name": "Millicent Norman"}
        ]),
    );
}

#[test]
fn selector_as() {
    #[derive(Clone, Deserialize, PartialEq, Debug)]
    struct Friend {
        id: u8,
        name: Option<String>,
    }

    fn select<'a, F>(selector: &mut F, path: &'a str, target: Vec<Friend>)
    where
        F: FnMut(&'a str) -> Result<Vec<Friend>, JsonPathError>,
    {
        let json = selector(path).unwrap();
        assert_eq!(json, target);
    }

    let json_obj = read_json("./benchmark/data_obj.json");
    let mut selector = jsonpath::selector_as::<Friend>(&json_obj);

    select(
        &mut selector,
        "$..friends[2]",
        vec![
            Friend {
                id: 2,
                name: Some("Gray Berry".to_string()),
            },
            Friend {
                id: 2,
                name: Some("Gray Berry".to_string()),
            },
        ],
    );

    select(
        &mut selector,
        "$..friends[0]",
        vec![
            Friend { id: 0, name: None },
            Friend {
                id: 0,
                name: Some("Millicent Norman".to_string()),
            },
        ],
    );
}

#[test]
fn select() {
    let json_obj = read_json("./benchmark/example.json");
    let json = jsonpath::select(&json_obj, "$..book[2]").unwrap();
    let ret = json!([{
        "category" : "fiction",
        "author" : "Herman Melville",
        "title" : "Moby Dick",
        "isbn" : "0-553-21311-3",
        "price" : 8.99
    }]);
    compare_result(json, ret);
}

#[test]
fn select_str() {
    let json_str = read_contents("./benchmark/example.json");
    let result_str = jsonpath::select_as_str(&json_str, "$..book[2]").unwrap();
    let ret = json!([{
        "category" : "fiction",
        "author" : "Herman Melville",
        "title" : "Moby Dick",
        "isbn" : "0-553-21311-3",
        "price" : 8.99
    }]);
    let json: Value = serde_json::from_str(&result_str).unwrap();
    assert_eq!(json, ret);
}

#[test]
fn test_to_struct() {
    #[derive(Deserialize, PartialEq, Debug)]
    struct Person {
        name: String,
        age: u8,
        phones: Vector<String>,
    }

    let ret: Vec<Person> = jsonpath::select_as(
        r#"
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
    "#,
        "$.person",
    )
    .unwrap();

    let person = Person {
        name: "Doe John".to_string(),
        age: 44,
        phones: vector!["+44 1234567".to_string(), "+44 2345678".to_string()],
    };

    assert_eq!(vec![person], ret);
}
