#![cfg(feature = "ts")]
#![allow(dead_code)]

mod common;

use std::io::Write;
use std::process::{Command, Stdio};

use common::{declarations, node_module, repo_root, SCHEMAS};
use rpc_toolkit::ts::{export_namespace, Bindings, TSVisitor, TS};

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
    #[visit(opaque, ts(skip))]
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
    /// Inline field documentation. It runs past prettier's print width so the
    /// member chain holding its description must format to a fixed point.
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
#[visit(ts(type = "z.string()"))]
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
    #[visit(ts(type = "z.boolean()"))]
    literal: bool,
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

const CONTAINER: &str =
    "/**\n * Container documentation.\n *\n * Second paragraph.\n * Closing *\\/ stays text.\n */\n";

#[test]
fn docs_follow_directional_names_and_visible_members() {
    let bindings = export_namespace("jsdoc", "docs", SCHEMAS).unwrap();
    assert!(
        bindings
            .schemas
            .contains(&format!("{CONTAINER}export const Documented: z.ZodCodec<")),
        "{}",
        bindings.schemas
    );
    assert!(bindings.schemas.contains(
        ".meta({\nid: \"Documented\", description: \"Container documentation.\\n\\nSecond paragraph.\\nClosing */ stays text.\" });"
    ));
    for name in ["Documented", "DocumentedRequest"] {
        assert!(
            bindings
                .types
                .contains(&format!("{CONTAINER}export type {name} =")),
            "{}",
            bindings.types
        );
    }
    for name in ["received", "legacy", "sent"] {
        assert!(
            bindings
                .schemas
                .contains(&format!("/**\n * Field documentation.\n */\n\"{name}\"")),
            "{}",
            bindings.schemas
        );
    }
    assert!(bindings.schemas.contains("Input-only"));
    assert!(bindings.schemas.contains("Output-only"));
    assert!(!bindings.schemas.contains("skipped documentation"));
}

#[test]
fn markdown_indentation_and_line_breaks_survive() {
    #[doc = " Code:\n\n     indented();\n       nested();\n Hard break  \nNext line."]
    #[derive(visit_rs::VisitFields)]
    struct Markdown;
    rpc_toolkit::reflect_ts!(Markdown);

    let bindings = declarations::<Markdown>();
    let comment =
        " * Code:\n *\n *     indented();\n *       nested();\n * Hard break  \n * Next line.";
    assert!(bindings.schemas.contains(comment), "{}", bindings.schemas);
    assert!(bindings.types.contains(comment), "{}", bindings.types);
    assert!(bindings.schemas.contains(
        ".meta({\nid: \"Markdown\", description: \"Code:\\n\\n    indented();\\n      nested();\\nHard break  \\nNext line.\" })"
    ), "{}", bindings.schemas);
}

#[test]
fn shared_lowering_and_inline_shapes_retain_docs() {
    let shared = declarations::<SharedShape>();
    assert!(shared
        .schemas
        .contains("Shared shape documentation.\n */\nexport const SharedShape"));
    assert!(shared
        .schemas
        .contains("Shared field documentation.\n */\n\"value\": z.string().meta({\ndescription: \"Shared field documentation.\" })"));
    let mut visitor = TSVisitor::new();
    visitor.declare::<Flattened<String>>("InlineRoot");
    let inline = visitor.into_bindings(None, SCHEMAS).unwrap().schemas;
    for text in [
        "Flattened payload documentation.",
        "Inline container documentation.",
        "Inline field documentation.",
    ] {
        assert!(inline.contains(text), "{}", inline);
    }
    assert!(!inline.contains("\"payload\""));
}

fn assert_enum<T: TS>() {
    let schemas = declarations::<T>().schemas;
    for text in [
        "Enum documentation.",
        "Unit documentation.",
        "Variant documentation.",
        "Variant field documentation.",
        "Input variant documentation.",
    ] {
        assert!(schemas.contains(text), "{}", schemas);
    }
    assert!(!schemas.contains("Skipped"));
}

#[test]
fn enum_docs_survive_all_serde_tag_layouts() {
    assert_enum::<External>();
    assert_enum::<Internal>();
    assert_enum::<Adjacent>();
    assert_enum::<Untagged>();
    let external = declarations::<External>().schemas;
    assert!(external.contains("Variant documentation.\n */\n\"sent\""));
    assert!(external.contains("Variant documentation.\n */\n\"received\""));
    let internal = declarations::<Internal>().schemas;
    assert!(internal.contains("Variant documentation.\n */\n\"kind\": z.literal(\"sent\")"));
    assert!(internal
        .contains("Variant documentation.\n */\n\"kind\": z.literal([\"legacy\", \"received\"])"));
}

#[test]
fn overrides_keep_owner_docs_without_claiming_replaced_members() {
    let transparent = declarations::<Transparent>().schemas;
    assert!(transparent.contains("Transparent documentation."));
    assert!(transparent.contains("export const Transparent: z.ZodString = /* @__PURE__ */ z.string().meta({\ndescription: \"Transparent field documentation.\" })"));
    assert!(!transparent.contains("Transparent hidden"));
    let literal = declarations::<Literal>().schemas;
    assert!(literal.contains("Literal container documentation."));
    assert!(!literal.contains("Replaced field"));
    let wire = declarations::<Wire>().schemas;
    assert!(wire.contains("Wire container documentation."));
    assert!(!wire.contains("Replaced wire field"));
    assert!(wire.contains("export const Wire: z.ZodCodec<z.ZodObject<{ \n/**\n * Inline field documentation. It runs past prettier's print width so the\n * member chain holding its description must format to a fixed point.\n */\n\"value\": z.ZodString }>, z.ZodString> = /* @__PURE__ */ wire(object({ \n/**\n * Inline field documentation. It runs past prettier's print width so the\n * member chain holding its description must format to a fixed point.\n */\n\"value\": z.string().meta({\ndescription: \"Inline field documentation. It runs past prettier's print width so the\\nmember chain holding its description must format to a fixed point.\" }) }).meta({\ndescription: \"Inline container documentation.\" }), z.string())"), "{}", wire);
    let converted = declarations::<Converted>().schemas;
    assert!(converted.contains("Conversion documentation."));
    assert!(converted.contains(
        "export const Converted: z.ZodCodec<z.ZodString, z.ZodInt> = /* @__PURE__ */ wire(z.string(), z.int())"
    ));
    let field = declarations::<WireField>().schemas;
    assert!(field.contains("Custom field documentation.\n */\n\"value\": z.string()"));
    assert!(field.contains("Custom field documentation.\n */\n\"value\": z.int()"));
    assert!(field.contains("Literal field documentation.\n */\n\"literal\": literal_0"));
    let tuple = declarations::<Tuple>().schemas;
    assert!(tuple.contains("export const Tuple: z.ZodTuple<[z.ZodString, z.ZodInt], null> = /* @__PURE__ */ z.tuple([z.string().meta({\ndescription: \"Tuple field documentation.\" }), z.int()])"), "{}", tuple);
}

#[test]
fn member_docs_survive_declaration_emit() {
    let mut visitor = TSVisitor::new();
    visitor.append_type::<Documented>();
    visitor.append_type::<Internal>();
    visitor.append_type::<SharedShape>();
    let bindings = visitor.into_bindings(None, SCHEMAS).unwrap();
    let declarations = common::declaration_file(&bindings);
    for text in [
        "Container documentation.",
        "Field documentation.",
        "Input-only documentation.",
        "Output-only documentation.",
        "Enum documentation.",
        "Variant documentation.",
        "Variant field documentation.",
        "Shared field documentation.",
    ] {
        assert!(declarations.contains(text), "{}\n{}", text, declarations);
    }
    assert!(declarations.contains("export declare const SharedShape: z.ZodObject<{"));
}

#[test]
fn declaration_caching_does_not_duplicate_docs_or_hide_collisions() {
    let mut visitor = TSVisitor::new();
    visitor.append_type::<Documented>();
    visitor.append_type::<Documented>();
    let bindings = visitor.into_bindings(None, SCHEMAS).unwrap();
    assert_eq!(
        bindings.schemas.matches("export const Documented").count(),
        1
    );
    assert_eq!(
        bindings.schemas.matches("Container documentation.").count(),
        2
    );
    let mut visitor = TSVisitor::new();
    visitor.declare::<Documented>("Collision");
    visitor.declare::<Literal>("Collision");
    assert!(visitor
        .into_bindings(None, SCHEMAS)
        .unwrap_err()
        .to_string()
        .contains("Conflicting"));
}

/// Runs a node script in a directory holding the module pair, with the
/// TypeScript compiler path as its first argument.
fn with_modules(bindings: &Bindings, assertions: &str, script: &str) {
    let fixtures = repo_root().join("target/ts-fixtures");
    std::fs::create_dir_all(&fixtures).unwrap();
    let dir = tempfile::tempdir_in(&fixtures).unwrap();
    std::fs::write(dir.path().join("schemas.ts"), &bindings.schemas).unwrap();
    std::fs::write(
        dir.path().join("types.ts"),
        format!("{}\n{assertions}", bindings.types),
    )
    .unwrap();
    let mut child = Command::new("node")
        .current_dir(dir.path())
        .args(["-e", script])
        .arg(repo_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}\n--- schemas.ts\n{}\n--- types.ts\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bindings.schemas,
        bindings.types
    );
}

const PROGRAM: &str = r#"
const root = process.argv[1];
const ts = require(root + '/node_modules/typescript');
const assert = require('node:assert/strict');
const options = { strict: true, noEmit: true, exactOptionalPropertyTypes: true, skipLibCheck: true, target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext, moduleResolution: ts.ModuleResolutionKind.Bundler };
const program = ts.createProgram(['types.ts'], options);
const diagnostics = ts.getPreEmitDiagnostics(program);
assert.equal(diagnostics.length, 0, ts.formatDiagnosticsWithColorAndContext(diagnostics, ts.createCompilerHost(options)));
const checker = program.getTypeChecker();
const source = program.getSourceFile('types.ts');
function alias(name) { return source.statements.find(s => ts.isTypeAliasDeclaration(s) && s.name.text === name); }
function docs(symbol) { return ts.displayPartsToString(symbol.getDocumentationComment(checker)); }
function property(type, name) {
  const symbol = checker.getPropertyOfType(type, name);
  assert.ok(symbol, name);
  return [symbol, checker.getTypeOfSymbol(symbol)];
}
"#;

#[test]
fn inline_rpc_docs_are_stable_after_one_prettier_pass() {
    /// RPC parameter container documentation.
    #[derive(visit_rs::VisitFields)]
    #[serde(transparent)]
    struct Params<T>(T);
    rpc_toolkit::reflect_ts!(impl [T] for Params<T> where [T: TS]);

    let mut visitor = TSVisitor::new();
    visitor.types_prelude(rpc_toolkit::ts::type_helpers());
    visitor
        .ts
        .push_str("{ _PARAMS: z.object({}), _CHILDREN: { \"inline\": { _PARAMS: ");
    visitor.append_type::<Params<Inline<String>>>();
    visitor
        .ts
        .push_str(", _RETURN: z.null() }, \"named\": { _PARAMS: ");
    visitor.append_type::<Params<Documented>>();
    visitor
        .ts
        .push_str(", _RETURN: z.null() }, \"variant\": { _PARAMS: ");
    visitor.append_type::<Params<External>>();
    visitor.ts.push_str(", _RETURN: z.null() } } }");
    let bindings = visitor.into_bindings(Some("RPC"), SCHEMAS).unwrap();
    assert_eq!(
        bindings
            .schemas
            .matches("RPC parameter container documentation.")
            .count(),
        3
    );
    assert!(bindings.schemas.contains("Inline container documentation."));
    let script = format!(
        r#"{PROGRAM}
const fs = require('node:fs');
const prettier = require(root + '/node_modules/prettier');
(async () => {{
  assert.equal(prettier.version, '3.8.3');
  const options = {{...JSON.parse(fs.readFileSync(root + '/.prettierrc.json')), parser: 'typescript'}};
  for (const file of ['schemas.ts', 'types.ts']) {{
    const rendered = fs.readFileSync(file, 'utf8');
    const once = await prettier.format(rendered, options);
    assert.ok(await prettier.check(once, options), file + ': one formatting pass must satisfy prettier.check');
    assert.equal(await prettier.format(once, options), once, file + ': second formatting pass must be byte-identical');
  }}
  const rpc = property(checker.getTypeAtLocation(alias('RPC')), '_CHILDREN')[1];
  const inline = property(property(rpc, 'inline')[1], '_PARAMS')[1];
  assert.match(docs(property(inline, 'value')[0]), /Inline field documentation\./);
  const named = property(property(rpc, 'named')[1], '_PARAMS')[1];
  assert.match(docs(property(named, 'received')[0]), /Field documentation\./);
  const variant = property(property(rpc, 'variant')[1], '_PARAMS')[1];
  assert.ok(variant.types.some(t => {{
    const symbol = checker.getPropertyOfType(t, 'received');
    return symbol && docs(symbol).includes('Variant documentation.');
  }}), 'variant property hover');
}})().catch(error => {{ console.error(error); process.exitCode = 1; }});
"#
    );
    with_modules(&bindings, "", &script);
}

#[test]
fn typescript_editor_docs_and_wire_types_remain_valid() {
    let mut visitor = TSVisitor::new();
    visitor.append_type::<Documented>();
    visitor.append_type::<External>();
    visitor.append_type::<Internal>();
    visitor.append_type::<Adjacent>();
    visitor.append_type::<Untagged>();
    visitor.append_type::<Transparent>();
    visitor.append_type::<Wire>();
    visitor.append_type::<WireField>();
    visitor.append_type::<Tuple>();
    visitor.append_type::<SharedShape>();
    visitor.append_type::<Literal>();
    visitor.append_type::<Converted>();
    visitor.declare::<Flattened<String>>("FlattenedRoot");
    let bindings = visitor.into_bindings(None, SCHEMAS).unwrap();
    let script = format!(
        r#"{PROGRAM}
for (const [name, member] of [['Documented', 'sent'], ['DocumentedRequest', 'received'], ['DocumentedRequest', 'legacy']]) {{
  const declaration = alias(name);
  assert.match(docs(checker.getSymbolAtLocation(declaration.name)), /Container documentation\./);
  const type = checker.getTypeAtLocation(declaration);
  assert.match(docs(property(type, member)[0]), /Field documentation\./, name + '.' + member);
}}
for (const [name, member] of [['External', 'sent'], ['Internal', 'kind'], ['Adjacent', 'kind']]) {{
  const type = checker.getTypeAtLocation(alias(name));
  const branch = type.types.find(t => checker.getPropertyOfType(t, member) && docs(checker.getPropertyOfType(t, member)).includes('Variant documentation.'));
  assert.ok(branch, name + ' variant hover');
}}
"#
    );
    with_modules(
        &bindings,
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
const flattened: FlattenedRoot = {value: 'text'};
const converted: ConvertedInput = 'text';
// @ts-expect-error
const convertedOutput: Converted = 'text';
"#,
        &script,
    );
    node_module("prettier");
}
