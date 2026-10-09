#![cfg(feature = "ts")]
#![allow(dead_code)]

use rpc_toolkit::ts::{export_namespace, Direction, TSVisitor, TS};

/// Container documentation.
///
#[doc = " Second paragraph.\nClosing */ stays text."]
#[derive(visit_rs::VisitFields)]
#[visit(ts(input_rename = "DocumentedRequest"))]
struct Documented {
    /// Field documentation.
    #[serde(rename(serialize = "sent", deserialize = "received"), alias = "legacy")]
    value: String,
    /// Input-only documentation.
    #[serde(skip_serializing)]
    input: u32,
    /// Output-only documentation.
    #[serde(skip_deserializing)]
    output: u32,
    /// Serde skipped documentation.
    #[serde(skip)]
    #[visit(opaque)]
    hidden: Unbound,
    /// TS skipped documentation.
    #[visit(type_attributes(visit::wire))]
    #[visit(ts(skip), wire = "rpc_toolkit::ts::Unknown")]
    #[visit(opaque)]
    injected: Unbound,
}
struct Unbound;
rpc_toolkit::reflect_ts!(Documented);
rpc_toolkit::ts_export!(Documented, namespaces = ["docs"]);
rpc_toolkit::reflect_ts!(impl [T] for Inline<T> where [T: TS]);
rpc_toolkit::reflect_ts!(impl [T] for Flattened<T> where [T: TS]);
rpc_toolkit::reflect_ts!(Transparent);
rpc_toolkit::reflect_ts!(Literal);
rpc_toolkit::reflect_ts!(Wire);
rpc_toolkit::reflect_ts!(WireField);
rpc_toolkit::reflect_ts!(Converted);
rpc_toolkit::reflect_ts!(Tuple);

/// Inline container documentation.
#[derive(visit_rs::VisitFields)]
struct Inline<T> {
    /// Inline field documentation.
    value: T,
}

/// Shared shape documentation.
#[derive(visit_rs::VisitFields)]
struct SharedShape {
    /// Shared field documentation.
    value: String,
}
rpc_toolkit::reflect_ts!(SharedShape);

macro_rules! documented_enum {
    ($name:ident $(, $attr:meta)?) => {
        /// Enum documentation.
        #[derive(visit_rs::VisitVariants)]
        $(#[$attr])?
        enum $name {
            /// Unit documentation.
            Unit,
            /// Variant documentation.
            #[serde(rename(serialize = "sent", deserialize = "received"), alias = "legacy")]
            Named {
                /// Variant field documentation.
                value: String,
                /// Skipped variant field documentation.
                #[serde(skip)]
                #[visit(opaque)]
                hidden: Unbound,
            },
            /// Input variant documentation.
            #[serde(skip_serializing)]
            Input,
            /// Skipped variant documentation.
            #[serde(skip)]
            #[visit(opaque)]
            Hidden(Unbound),
        }
        rpc_toolkit::reflect_ts!($name);
    };
}
documented_enum!(External);
documented_enum!(Internal, serde(tag = "kind"));
documented_enum!(Adjacent, serde(tag = "kind", content = "data"));
documented_enum!(Untagged, serde(untagged));

/// Transparent documentation.
#[derive(visit_rs::VisitFields)]
#[serde(transparent)]
struct Transparent {
    /// Transparent field documentation.
    value: String,
    /// Transparent hidden documentation.
    #[serde(skip)]
    #[visit(opaque)]
    hidden: Unbound,
}

#[derive(visit_rs::VisitFields)]
struct Flattened<T> {
    /// Flattened payload documentation.
    #[serde(flatten)]
    payload: Inline<T>,
}

/// Literal container documentation.
#[derive(visit_rs::VisitFields)]
#[visit(type_attributes(visit::wire))]
#[visit(ts(type = "string"), wire = "rpc_toolkit::ts::Unknown")]
struct Literal {
    /// Replaced field documentation.
    #[visit(opaque)]
    hidden: Unbound,
}

/// Wire container documentation.
#[derive(visit_rs::VisitFields)]
#[visit(type_attributes(visit::input_wire, visit::output_wire))]
#[visit(input_wire = "Inline<String>", output_wire = "String")]
struct Wire {
    /// Replaced wire field documentation.
    #[visit(opaque)]
    hidden: Unbound,
}

#[derive(visit_rs::VisitFields)]
struct WireField {
    /// Custom field documentation.
    #[serde(deserialize_with = "custom")]
    #[visit(type_attributes(visit::input_wire, visit::output_wire))]
    #[visit(input_wire = "String", output_wire = "u32")]
    #[visit(opaque)]
    value: Unbound,
    /// Literal field documentation.
    #[visit(type_attributes(visit::wire))]
    #[visit(ts(type = "boolean"), wire = "rpc_toolkit::ts::Unknown")]
    #[visit(opaque)]
    literal: Unbound,
}

/// Conversion documentation.
#[derive(visit_rs::VisitFields)]
#[visit(type_attributes(serde::from, serde::into))]
#[serde(from = "String", into = "u32")]
#[visit(opaque)]
struct Converted(Unbound);

/// Tuple documentation.
#[derive(visit_rs::VisitFields)]
struct Tuple(#[doc = "Tuple field documentation."] String, u32);

fn declarations<T: TS>(direction: Direction) -> String {
    let mut visitor = TSVisitor::new();
    visitor.with_direction(direction, |visitor| visitor.append_type::<T>());
    visitor.into_declarations().unwrap()
}

#[test]
fn docs_follow_directional_names_and_visible_members() {
    let module = export_namespace("jsdoc", "docs").unwrap();
    let container = "/**\n * Container documentation.\n *\n * Second paragraph.\n * Closing *\\/ stays text.\n */\n";
    for name in ["Documented", "DocumentedRequest"] {
        assert!(
            module.contains(&format!("{container}export type {name} =")),
            "{}",
            module
        );
    }
    for (direction, names, present, absent) in [
        (
            Direction::Input,
            vec!["received", "legacy"],
            "Input-only",
            "Output-only",
        ),
        (Direction::Output, vec!["sent"], "Output-only", "Input-only"),
    ] {
        let output = declarations::<Documented>(direction);
        for name in names {
            assert!(
                output.contains(&format!("/**\n * Field documentation.\n */\n\"{name}\"")),
                "{}",
                output
            );
        }
        assert!(output.contains(present));
        assert!(!output.contains(absent));
        assert!(!output.contains("skipped documentation"));
    }
}

#[test]
fn markdown_indentation_and_line_breaks_survive() {
    #[doc = " Code:\n\n     indented();\n       nested();\n Hard break  \nNext line."]
    #[derive(visit_rs::VisitFields)]
    struct Markdown;
    rpc_toolkit::reflect_ts!(Markdown);

    let output = declarations::<Markdown>(Direction::Output);
    assert!(
        output.contains(
            " * Code:\n *\n *     indented();\n *       nested();\n * Hard break  \n * Next line."
        ),
        "{}",
        output
    );
}

#[test]
fn shared_lowering_and_inline_shapes_retain_docs() {
    for direction in [Direction::Input, Direction::Output] {
        let shared = declarations::<SharedShape>(direction);
        assert!(shared.contains("Shared shape documentation.\n */\nexport type"));
        assert!(shared.contains("Shared field documentation.\n */\n\"value\""));
        let mut visitor = TSVisitor::new();
        visitor.with_direction(direction, |v| v.append_type::<Flattened<String>>());
        let inline = visitor.into_module("InlineRoot").unwrap();
        for text in [
            "Flattened payload documentation.",
            "Inline container documentation.",
            "Inline field documentation.",
        ] {
            assert!(inline.contains(text), "{}", inline);
        }
        assert!(!inline.contains("\"payload\""));
    }
}

fn assert_enum<T: TS>(direction: Direction) {
    let output = declarations::<T>(direction);
    for text in [
        "Enum documentation.",
        "Unit documentation.",
        "Variant documentation.",
        "Variant field documentation.",
    ] {
        assert!(output.contains(text), "{}", output);
    }
    assert!(!output.contains("Skipped"));
    assert_eq!(
        output.contains("Input variant documentation."),
        direction == Direction::Input
    );
}

#[test]
fn enum_docs_survive_all_serde_tag_layouts() {
    for direction in [Direction::Input, Direction::Output] {
        assert_enum::<External>(direction);
        assert_enum::<Internal>(direction);
        assert_enum::<Adjacent>(direction);
        assert_enum::<Untagged>(direction);
    }
    let external = declarations::<External>(Direction::Output);
    assert!(external.contains("Variant documentation.\n */\n\"sent\""));
    let internal = declarations::<Internal>(Direction::Output);
    assert!(internal.contains("Variant documentation.\n */\n\"kind\""));
}

#[test]
fn overrides_keep_owner_docs_without_claiming_replaced_members() {
    for direction in [Direction::Input, Direction::Output] {
        let transparent = declarations::<Transparent>(direction);
        assert!(transparent.contains("Transparent documentation."));
        assert!(transparent.contains("Transparent field documentation."));
        assert!(!transparent.contains("Transparent hidden"));
        let literal = declarations::<Literal>(direction);
        assert!(literal.contains("Literal container documentation."));
        assert!(!literal.contains("Replaced field"));
        let wire = declarations::<Wire>(direction);
        assert!(wire.contains("Wire container documentation."));
        assert!(!wire.contains("Replaced wire field"));
        assert_eq!(
            wire.contains("Inline field documentation."),
            direction == Direction::Input
        );
        let converted = declarations::<Converted>(direction);
        assert!(converted.contains("Conversion documentation."));
        assert!(converted.contains(if direction == Direction::Input {
            "= string;"
        } else {
            "= number;"
        }));
        let field = declarations::<WireField>(direction);
        assert!(field.contains("Custom field documentation.\n */\n\"value\""));
        assert!(field.contains(if direction == Direction::Input {
            "\"value\":(string)"
        } else {
            "\"value\":(number)"
        }));
        assert!(field.contains("Literal field documentation.\n */\n\"literal\":(boolean)"));
        let tuple = declarations::<Tuple>(direction);
        assert!(tuple.contains("Tuple field documentation.\n */\nstring,number]"));
    }
}

#[test]
fn declaration_caching_does_not_duplicate_docs_or_hide_collisions() {
    let mut visitor = TSVisitor::new();
    visitor.append_type::<Documented>();
    visitor.append_type::<Documented>();
    let output = visitor.into_declarations().unwrap();
    assert_eq!(output.matches("Container documentation.").count(), 1);
    let mut visitor = TSVisitor::new();
    visitor.declare::<Documented>("Collision");
    visitor.declare::<Literal>("Collision");
    assert!(visitor
        .into_declarations()
        .unwrap_err()
        .to_string()
        .contains("Conflicting"));
}

#[test]
fn inline_rpc_docs_are_stable_after_one_prettier_pass() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    /// RPC parameter container documentation.
    #[derive(visit_rs::VisitFields)]
    #[serde(transparent)]
    struct Params<T>(T);
    rpc_toolkit::reflect_ts!(impl [T] for Params<T> where [T: TS]);

    let mut visitor = TSVisitor::new();
    visitor.ts.push_str("{inline:{_PARAMS:");
    visitor.append_type::<Params<Inline<String>>>();
    visitor.ts.push_str(";_RETURN:null};named:{_PARAMS:");
    visitor.append_type::<Params<Documented>>();
    visitor.ts.push_str(";_RETURN:null};variant:{_PARAMS:");
    visitor.append_type::<Params<External>>();
    visitor.ts.push_str(";_RETURN:null}}");
    let module = visitor.into_module("RPC").unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let script = r#"
const fs = require('node:fs');
const assert = require('node:assert/strict');
const root = process.argv[1];
const prettier = require(root + '/node_modules/prettier');
const ts = require(root + '/node_modules/typescript');
(async () => {
  assert.equal(prettier.version, '3.8.3');
  const options = {...JSON.parse(fs.readFileSync(root + '/.prettierrc.json')), parser: 'typescript'};
  const rendered = fs.readFileSync(0, 'utf8');
  const once = await prettier.format(rendered, options);
  assert.ok(await prettier.check(once, options), 'one formatting pass must satisfy prettier.check');
  assert.equal(await prettier.format(once, options), once, 'second formatting pass must be byte-identical');
  for (const text of [rendered, once]) {
    assert.equal(text.split('RPC parameter container documentation.').length - 1, 3);
    assert.ok(text.includes('Inline container documentation.'));
    const file = '/virtual-inline-rpc.ts';
    const compilerOptions = {strict: true, noEmit: true, target: ts.ScriptTarget.ES2020};
    const host = ts.createCompilerHost(compilerOptions);
    const original = host.getSourceFile.bind(host);
    host.getSourceFile = (name, ...args) => name === file ? ts.createSourceFile(file, text, compilerOptions.target, true) : original(name, ...args);
    const program = ts.createProgram([file], compilerOptions, host);
    const diagnostics = ts.getPreEmitDiagnostics(program);
    assert.equal(diagnostics.length, 0, ts.formatDiagnosticsWithColorAndContext(diagnostics, host));
    const checker = program.getTypeChecker();
    const source = program.getSourceFile(file);
    const declaration = source.statements.find(s => ts.isTypeAliasDeclaration(s) && s.name.text === 'RPC');
    const rpc = checker.getTypeAtLocation(declaration);
    function property(type, name) {
      const symbol = checker.getPropertyOfType(type, name);
      assert.ok(symbol, name);
      return [symbol, checker.getTypeOfSymbolAtLocation(symbol, declaration)];
    }
    function docs(symbol) { return ts.displayPartsToString(symbol.getDocumentationComment(checker)); }
    const inline = property(property(rpc, 'inline')[1], '_PARAMS')[1];
    assert.match(docs(property(inline, 'value')[0]), /Inline field documentation\./);
    const named = property(property(rpc, 'named')[1], '_PARAMS')[1];
    assert.match(docs(property(named, 'sent')[0]), /Field documentation\./);
    const variant = property(property(rpc, 'variant')[1], '_PARAMS')[1];
    assert.ok(variant.types.some(t => {
      const symbol = checker.getPropertyOfType(t, 'sent');
      return symbol && docs(symbol).includes('Variant documentation.');
    }), 'variant property hover');
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
"#;
    let mut child = Command::new("node")
        .args(["-e", script])
        .arg(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(module.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn typescript_editor_docs_and_wire_types_remain_valid() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut module = export_namespace("jsdoc", "docs").unwrap();
    for direction in [Direction::Input, Direction::Output] {
        module.push_str(&declarations::<External>(direction));
        module.push_str(&declarations::<Internal>(direction));
        module.push_str(&declarations::<Adjacent>(direction));
        module.push_str(&declarations::<Untagged>(direction));
        module.push_str(&declarations::<Transparent>(direction));
        module.push_str(&declarations::<Wire>(direction));
        module.push_str(&declarations::<WireField>(direction));
        module.push_str(&declarations::<Tuple>(direction));
        module.push_str(&declarations::<SharedShape>(direction));
        module.push_str(&declarations::<Literal>(direction));
        module.push_str(&declarations::<Converted>(direction));
        let mut visitor = TSVisitor::new();
        visitor.with_direction(direction, |v| v.append_type::<Flattened<String>>());
        module.push_str(
            &visitor
                .into_module(if direction == Direction::Input {
                    "FlattenedInput"
                } else {
                    "FlattenedOutput"
                })
                .unwrap(),
        );
    }
    let typescript = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../node_modules/typescript/lib/typescript.js");
    assert!(
        typescript.exists(),
        "Run make rpc-toolkit-test to provision TypeScript"
    );
    let script = r#"
const ts = require(process.argv[1]);
const assert = require('node:assert/strict');
const text = require('node:fs').readFileSync(0, 'utf8');
const file = '/virtual-jsdoc.ts';
const options = { strict: true, noEmit: true, target: ts.ScriptTarget.ES2020 };
const host = ts.createCompilerHost(options);
const original = host.getSourceFile.bind(host);
host.getSourceFile = (name, ...args) => name === file ? ts.createSourceFile(file, text, options.target, true) : original(name, ...args);
const program = ts.createProgram([file], options, host);
const diagnostics = ts.getPreEmitDiagnostics(program);
assert.equal(diagnostics.length, 0, ts.formatDiagnosticsWithColorAndContext(diagnostics, host));
const checker = program.getTypeChecker();
const source = program.getSourceFile(file);
function alias(name) { return source.statements.find(s => ts.isTypeAliasDeclaration(s) && s.name.text === name); }
function docs(symbol) { return ts.displayPartsToString(symbol.getDocumentationComment(checker)); }
for (const [name, property] of [['Documented', 'sent'], ['DocumentedRequest', 'received'], ['DocumentedRequest', 'legacy']]) {
  const declaration = alias(name);
  assert.match(docs(checker.getSymbolAtLocation(declaration.name)), /Container documentation\./);
  const type = checker.getTypeAtLocation(declaration);
  assert.match(docs(checker.getPropertyOfType(type, property)), /Field documentation\./);
}
for (const [name, property] of [['External', 'sent'], ['Internal', 'kind'], ['Adjacent', 'kind']]) {
  const type = checker.getTypeAtLocation(alias(name));
  const branch = type.types.find(t => checker.getPropertyOfType(t, property) && docs(checker.getPropertyOfType(t, property)).includes('Variant documentation.'));
  assert.ok(branch, name + ' variant hover');
}
"#;
    module.push_str(
        r#"
const output: Documented = {sent: 'text', output: 1};
const input: DocumentedRequest = {received: 'text', input: 1};
const alias: DocumentedRequest = {legacy: 'text', input: 1};
// @ts-expect-error
const missingAlias: DocumentedRequest = {input: 1};
// @ts-expect-error
const wrongDirection: Documented = {received: 'text', output: 1};
// @ts-expect-error
const wrongWire: WireField = {value: 'text', literal: true};
const customInput: WireFieldInput = {value: 'text', literal: true};
const flattened: FlattenedOutput = {value: 'text'};
"#,
    );
    let mut child = Command::new("node")
        .args(["-e", script])
        .arg(typescript)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(module.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
