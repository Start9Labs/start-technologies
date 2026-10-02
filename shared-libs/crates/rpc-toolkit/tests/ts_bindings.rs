#![cfg(feature = "ts")]

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use rpc_toolkit::ts::{handler_bindings, Direction, TSVisitor, TS};
use rpc_toolkit::{from_fn, impl_ts_shape, Context, Empty, HandlerExt, ParentHandler, Server};
use serde::{Deserialize, Serialize};
use serde_json::json;
use visit_rs::SerdeShape;
use yajrc::RpcError;

#[derive(Clone)]
struct Ctx;
impl Context for Ctx {}

#[derive(Serialize, Deserialize, SerdeShape)]
#[serde(rename_all(serialize = "camelCase", deserialize = "SCREAMING_SNAKE_CASE"))]
struct Params {
    required_value: u64,
    nullable: Option<String>,
    #[serde(default)]
    with_default: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    omitted: Option<String>,
    #[serde(rename(serialize = "out", deserialize = "in"), alias = "legacy")]
    renamed: String,
    #[serde(skip_serializing)]
    input_only: String,
    #[serde(skip_deserializing)]
    output_only: u32,
}
impl_ts_shape!(Params { define: "Params" });

#[derive(Serialize, Deserialize, SerdeShape)]
struct Node {
    value: String,
    next: Option<Box<Node>>,
}
impl_ts_shape!(Node { define: "Node" });

#[derive(Serialize, Deserialize, SerdeShape)]
struct Flat {
    own: String,
    #[serde(flatten)]
    inner: Option<Inner>,
}
impl_ts_shape!(Flat);

#[derive(Serialize, Deserialize, SerdeShape)]
struct Inner {
    inner: u32,
}
impl_ts_shape!(Inner);

#[derive(Serialize, Deserialize, SerdeShape)]
#[serde(transparent)]
struct Transparent {
    data: String,
    #[serde(skip)]
    skipped: bool,
}
impl_ts_shape!(Transparent);

#[derive(Serialize, Deserialize, SerdeShape)]
struct Unit;
impl_ts_shape!(Unit);
#[derive(Serialize, Deserialize, SerdeShape)]
struct OneTuple(String);
impl_ts_shape!(OneTuple);
#[derive(Serialize, Deserialize, SerdeShape)]
struct EmptyTuple();
impl_ts_shape!(EmptyTuple);

#[derive(Serialize, Deserialize, SerdeShape)]
#[serde(rename_all = "snake_case", rename_all_fields = "camelCase")]
enum External {
    IOError,
    One(String),
    Pair(u32, bool),
    Object {
        some_value: String,
    },
    Empty {},
    EmptyTuple(),
    #[serde(skip)]
    Hidden,
}
impl_ts_shape!(External);

#[derive(Serialize, Deserialize, SerdeShape)]
#[serde(tag = "kind")]
enum Internal {
    Unit,
    Data {
        value: u32,
    },
    Newtype(Inner),
    Nullable(Option<Inner>),
    Null(()),
    #[serde(untagged)]
    Untagged(Inner),
}
impl_ts_shape!(Internal);
#[derive(Serialize, Deserialize, SerdeShape)]
#[serde(tag = "kind", content = "payload")]
enum Adjacent {
    Unit,
    Data(u32),
    Pair(String, u32),
}
impl_ts_shape!(Adjacent);
#[derive(Serialize, Deserialize, SerdeShape)]
#[serde(untagged)]
enum Untagged {
    Unit,
    Number(u32),
    Data { value: String },
}
impl_ts_shape!(Untagged);
#[derive(Serialize, Deserialize, SerdeShape)]
enum Generic<T> {
    Data(T),
}
impl_ts_shape!(Generic<u32>);

fn module<T: TS>(direction: Direction) -> String {
    let mut visitor = TSVisitor::new();
    visitor.with_direction(direction, |v| v.append_type::<T>());
    visitor.into_module("TestType").unwrap()
}

fn typecheck(module: &str, assertions: &str) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("fixture.ts");
    std::fs::write(&file, format!("{module}\n{assertions}")).unwrap();
    let tsc =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../node_modules/typescript/bin/tsc");
    assert!(
        tsc.exists(),
        "Run npm ci at the monorepo root before testing the ts feature"
    );
    let output = Command::new("node")
        .current_dir(dir.path())
        .arg(tsc)
        .args([
            "--strict",
            "--exactOptionalPropertyTypes",
            "--noEmit",
            "--skipLibCheck",
            "--target",
            "ES2020",
        ])
        .arg(file)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn check_serialized<T: TS + Serialize>(values: &[T], rejected: &str) {
    let mut assertions = String::new();
    for (i, value) in values.iter().enumerate() {
        assertions.push_str(&format!(
            "const value{i}: TestType = {};\n",
            serde_json::to_string(value).unwrap()
        ));
    }
    assertions.push_str(&format!(
        "// @ts-expect-error\nconst rejected: TestType = {rejected};\n"
    ));
    typecheck(&module::<T>(Direction::Output), &assertions);
}

#[test]
fn serde_struct_shapes_typecheck() {
    let params = Params {
        required_value: 1,
        nullable: None,
        with_default: false,
        omitted: None,
        renamed: "name".into(),
        input_only: "input".into(),
        output_only: 42,
    };
    assert_eq!(params.input_only, "input");
    assert!(
        !Transparent {
            data: "x".into(),
            skipped: false
        }
        .skipped
    );
    assert!(serde_json::to_value(External::Hidden).is_err());
    check_serialized(&[params], "{requiredValue:1,out:'name',outputOnly:42}");
    let inputs = [
        json!({"REQUIRED_VALUE":1,"in":"name","INPUT_ONLY":"input"}),
        json!({"REQUIRED_VALUE":1,"legacy":"name","INPUT_ONLY":"input","NULLABLE":null}),
    ];
    let mut assertions = String::new();
    for (i, input) in inputs.iter().enumerate() {
        serde_json::from_value::<Params>(input.clone()).unwrap();
        assertions.push_str(&format!("const input{i}: TestType = {input};\n"));
    }
    assertions.push_str("// @ts-expect-error\nconst missing: TestType = {REQUIRED_VALUE:1,INPUT_ONLY:'x'};\n// @ts-expect-error\nconst wrong: TestType = {REQUIRED_VALUE:'one',in:'x',INPUT_ONLY:'x'};");
    typecheck(&module::<Params>(Direction::Input), &assertions);
    check_serialized(
        &[
            Flat {
                own: "a".into(),
                inner: None,
            },
            Flat {
                own: "b".into(),
                inner: Some(Inner { inner: 1 }),
            },
        ],
        "{own:1}",
    );
    check_serialized(
        &[Transparent {
            data: "data".into(),
            skipped: false,
        }],
        "{data:'data'}",
    );
    check_serialized(&[Unit], "[]");
    check_serialized(&[OneTuple("value".into())], "['value']");
    check_serialized(&[EmptyTuple()], "null");
}

#[test]
fn serde_enum_shapes_typecheck() {
    check_serialized(
        &[
            External::IOError,
            External::One("x".into()),
            External::Pair(1, true),
            External::Object {
                some_value: "x".into(),
            },
            External::Empty {},
            External::EmptyTuple(),
        ],
        "'Hidden'",
    );
    assert!(serde_json::to_value(Internal::Nullable(None)).is_err());
    check_serialized(
        &[
            Internal::Unit,
            Internal::Data { value: 1 },
            Internal::Newtype(Inner { inner: 1 }),
            Internal::Null(()),
            Internal::Untagged(Inner { inner: 1 }),
        ],
        "{kind:'Data',value:'one'}",
    );
    check_serialized(
        &[
            Adjacent::Unit,
            Adjacent::Data(1),
            Adjacent::Pair("x".into(), 1),
        ],
        "{kind:'Data',payload:'one'}",
    );
    check_serialized(
        &[
            Untagged::Unit,
            Untagged::Number(1),
            Untagged::Data { value: "x".into() },
        ],
        "true",
    );
    check_serialized(&[Generic::Data(1u32)], "{Data:'one'}");
}

#[test]
fn recursive_and_composed_types_typecheck() {
    check_serialized(
        &[Node {
            value: "a".into(),
            next: Some(Box::new(Node {
                value: "b".into(),
                next: None,
            })),
        }],
        "{value:1,next:null}",
    );
    check_serialized(&[BTreeMap::from([(1u64, "value".to_owned())])], "{'1':1}");
    typecheck(&module::<rpc_toolkit::util::Flat<Inner, Flat>>(Direction::Output), "const ok: TestType = {own:'a',inner:1};\n// @ts-expect-error\nconst bad: TestType = {own:1,inner:1};");
}

#[derive(Serialize, Deserialize, SerdeShape)]
struct ParentParams {
    token: String,
}
impl_ts_shape!(ParentParams);

fn leaf(
    _: Ctx,
    p: Params,
    _: rpc_toolkit::util::Flat<ParentParams, Empty>,
) -> Result<Node, RpcError> {
    Ok(Node {
        value: p.renamed,
        next: None,
    })
}
fn callable(_: Ctx, p: ParentParams) -> Result<String, RpcError> {
    Ok(p.token)
}

#[tokio::test]
async fn handler_tree_matches_dispatch_and_inference() {
    let parent = ParentHandler::<Ctx, ParentParams, Empty>::new()
        .root_handler(from_fn(callable).no_cli())
        .subcommand("leaf", from_fn(leaf).no_cli())
        .subcommand(
            "opaque",
            from_fn(|_: Ctx, _: Empty| Ok::<_, RpcError>(()))
                .no_cli()
                .no_ts(),
        );
    let root = ParentHandler::<Ctx, Empty, Empty>::new().subcommand("jobs", parent.no_cli());
    let module = handler_bindings(&root, "Api").unwrap().unwrap();
    typecheck(
        &module,
        r#"
const rootParams: RpcParamType<Api,'jobs'> = {token:'t'};
const params: RpcParamType<Api,'jobs.leaf'> = {token:'t',REQUIRED_VALUE:1,in:'x',INPUT_ONLY:'x'};
const result: RpcReturnType<Api,'jobs.leaf'> = {value:'x',next:null};
const parentResult: RpcReturnType<Api,'jobs'> = 't';
const opaqueResult: RpcReturnType<Api,'jobs.opaque'> = true;
// @ts-expect-error
const invalid: RpcParamType<Api,'jobs.leaf'> = {REQUIRED_VALUE:1,in:'x',INPUT_ONLY:'x'};
// @ts-expect-error
const invalidNested: RpcReturnType<Api,'missing.leaf'> = true;
// @ts-expect-error
const namespace: RpcParamType<Api,''> = {};
// @ts-expect-error
const invalidReturn: RpcReturnType<Api,'jobs.leaf'> = {value:1,next:null};
"#,
    );
    let server = Server::new(|| async { Ok(Ctx) }, root);
    let result = server
        .handle_command(
            "jobs.leaf",
            imbl_value::to_value(
                &json!({"token":"t","REQUIRED_VALUE":1,"in":"x","INPUT_ONLY":"x"}),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        imbl_value::from_value::<serde_json::Value>(result).unwrap(),
        json!({"value":"x","next":null})
    );
    let result = server
        .handle_command("jobs", imbl_value::to_value(&json!({"token":"t"})).unwrap())
        .await
        .unwrap();
    assert_eq!(imbl_value::from_value::<String>(result).unwrap(), "t");
}

#[test]
fn conflicting_and_recursive_inline_types_fail_generation() {
    struct First;
    struct Second;
    impl TS for First {
        const DEFINE: Option<&str> = Some("Same");
        fn visit_ts(v: &mut TSVisitor) {
            v.ts.push_str("string");
        }
    }
    impl TS for Second {
        const DEFINE: Option<&str> = Some("Same");
        fn visit_ts(v: &mut TSVisitor) {
            v.ts.push_str("number");
        }
    }
    let mut v = TSVisitor::new();
    v.append_type::<First>();
    v.append_type::<Second>();
    assert!(v
        .into_module("Root")
        .unwrap_err()
        .to_string()
        .contains("Conflicting"));
    struct Recursive;
    impl TS for Recursive {
        fn visit_ts(v: &mut TSVisitor) {
            v.append_type::<Recursive>();
        }
    }
    let mut v = TSVisitor::new();
    v.append_type::<Recursive>();
    assert!(v
        .into_module("Root")
        .unwrap_err()
        .to_string()
        .contains("requires DEFINE"));
    for name in [
        "lowercase",
        "Partial",
        "Exclude",
        "RpcHandler",
        "Invalid-Name",
    ] {
        assert!(TSVisitor::new().into_module(name).is_err());
    }
}

#[derive(Default, Serialize, Deserialize, SerdeShape)]
#[serde(default)]
struct Defaults {
    required: String,
    count: u32,
}
impl_ts_shape!(Defaults);
#[derive(Serialize, Deserialize, SerdeShape)]
struct TupleOption(Option<String>, u32);
impl_ts_shape!(TupleOption);
#[derive(Serialize, Deserialize, SerdeShape)]
struct SkippedNewtype(#[serde(skip)] bool);
impl_ts_shape!(SkippedNewtype);

#[derive(Serialize, Deserialize, SerdeShape, Clone)]
#[serde(from = "String", into = "String")]
struct Converted {
    raw: String,
}
impl From<String> for Converted {
    fn from(raw: String) -> Self {
        Self { raw }
    }
}
impl From<Converted> for String {
    fn from(value: Converted) -> Self {
        value.raw
    }
}
impl_ts_shape!(Converted);

#[test]
fn containers_sequences_and_conversions_match_serde() {
    serde_json::from_value::<Defaults>(json!({})).unwrap();
    typecheck(&module::<Defaults>(Direction::Input), "const omitted: TestType = {};\n// @ts-expect-error\nconst wrong: TestType = {count:'one'};");
    assert!(serde_json::from_value::<TupleOption>(json!([null])).is_err());
    typecheck(
        &module::<TupleOption>(Direction::Input),
        "const valid: TestType = [null,1];\n// @ts-expect-error\nconst short: TestType = [null];",
    );
    check_serialized(&[(Some("x".to_owned()), true)], "['x']");
    check_serialized(&[[Some(1u32), None]], "[1]");
    check_serialized(&[f64::NAN, f64::INFINITY, 1.5], "'1.5'");
    let skipped = SkippedNewtype(true);
    assert!(skipped.0);
    check_serialized(&[skipped], "null");
    serde_json::from_value::<SkippedNewtype>(json!(true)).unwrap();
    check_serialized(&[Converted { raw: "x".into() }], "{raw:'x'}");
    serde_json::from_value::<Converted>(json!("x")).unwrap();
    typecheck(
        &module::<Converted>(Direction::Input),
        "const valid: TestType = 'x';\n// @ts-expect-error\nconst invalid: TestType = {raw:'x'};",
    );
}

fn serialize_number<S: serde::Serializer>(value: &u32, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&value.to_string())
}
#[derive(Serialize, Deserialize, SerdeShape)]
struct Custom {
    #[serde(serialize_with = "serialize_number")]
    value: u32,
}
impl_ts_shape!(Custom);

#[test]
fn overrides_and_opt_out_compose_with_other_adapters() {
    let f = from_fn(|_: Ctx, _: Empty| Ok::<_, RpcError>(Custom { value: 1 }));
    assert!(handler_bindings(&f.no_cli(), "Api")
        .unwrap_err()
        .to_string()
        .contains("Custom serde field"));
    let f = from_fn(|_: Ctx, _: Empty| Ok::<_, RpcError>(Custom { value: 1 }));
    let bound = f
        .no_cli()
        .override_return_ts_as::<String>()
        .no_display()
        .with_about("about");
    let module = handler_bindings(&bound, "Api").unwrap().unwrap();
    typecheck(&module, "const ok: RpcReturnType<Api,''> = '1';\n// @ts-expect-error\nconst bad: RpcReturnType<Api,''> = {value:1};");
    let f = from_fn(|_: Ctx, _: Empty| Ok::<_, RpcError>(()));
    assert!(handler_bindings(
        &f.no_ts()
            .no_display()
            .no_cli()
            .override_params_ts_as::<String>(),
        "Api"
    )
    .unwrap()
    .is_none());
    let f = from_fn(|_: Ctx, _: Empty| Ok::<_, RpcError>(()));
    let parent = ParentHandler::<Ctx, Empty, Empty>::new().root_handler(f.no_cli().no_ts());
    typecheck(
        &handler_bindings(&parent, "Api").unwrap().unwrap(),
        "const opaque: RpcReturnType<Api,''> = true;",
    );
}
