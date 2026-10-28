#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use rpc_toolkit::ts::{Bindings, TSVisitor, TS};

pub const SCHEMAS: &str = "./schemas";

/// Declares one type as `TestType` beside its dependencies.
pub fn module<T: TS>() -> Bindings {
    let mut visitor = TSVisitor::new();
    visitor.declare::<T>("TestType");
    visitor.into_bindings(None, SCHEMAS).unwrap()
}

/// Registers one type and its dependencies under their own names.
pub fn declarations<T: TS>() -> Bindings {
    let mut visitor = TSVisitor::new();
    visitor.append_type::<T>();
    visitor.into_bindings(None, SCHEMAS).unwrap()
}

pub fn rejection<T: TS>() -> String {
    let mut visitor = TSVisitor::new();
    visitor.append_type::<T>();
    visitor
        .into_bindings(None, SCHEMAS)
        .unwrap_err()
        .to_string()
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

pub fn node_module(path: &str) -> PathBuf {
    let module = repo_root().join("node_modules").join(path);
    assert!(
        module.exists(),
        "Run npm ci at the monorepo root before testing the ts feature"
    );
    module
}

/// Writes the module pair where `zod` resolves and type-checks it with the
/// pinned compiler, the assertions appended to the type module.
pub fn typecheck(bindings: &Bindings, assertions: &str) {
    let fixtures = repo_root().join("target/ts-fixtures");
    std::fs::create_dir_all(&fixtures).unwrap();
    let dir = tempfile::tempdir_in(&fixtures).unwrap();
    std::fs::write(dir.path().join("schemas.ts"), &bindings.schemas).unwrap();
    std::fs::write(
        dir.path().join("types.ts"),
        format!("{}\n{assertions}", bindings.types),
    )
    .unwrap();
    let output = Command::new("node")
        .current_dir(dir.path())
        .arg(node_module("typescript/bin/tsc"))
        .args([
            "--strict",
            "--exactOptionalPropertyTypes",
            "--noPropertyAccessFromIndexSignature",
            "--noUncheckedIndexedAccess",
            "--noEmit",
            "--skipLibCheck",
            "--ignoreConfig",
            "--target",
            "ES2022",
            "--module",
            "esnext",
            "--moduleResolution",
            "bundler",
            "schemas.ts",
            "types.ts",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}\n--- schemas.ts\n{}\n--- types.ts\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        bindings.schemas,
        bindings.types
    );
}

/// The `.d.ts` TypeScript emits for the schema module.
pub fn declaration_file(bindings: &Bindings) -> String {
    let fixtures = repo_root().join("target/ts-fixtures");
    std::fs::create_dir_all(&fixtures).unwrap();
    let dir = tempfile::tempdir_in(&fixtures).unwrap();
    std::fs::write(dir.path().join("schemas.ts"), &bindings.schemas).unwrap();
    let output = Command::new("node")
        .current_dir(dir.path())
        .arg(node_module("typescript/bin/tsc"))
        .args([
            "--strict",
            "--declaration",
            "--emitDeclarationOnly",
            "--skipLibCheck",
            "--ignoreConfig",
            "--target",
            "ES2022",
            "--module",
            "esnext",
            "--moduleResolution",
            "bundler",
            "schemas.ts",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::read_to_string(dir.path().join("schemas.d.ts")).unwrap()
}

/// Evaluates a script against the compiled schema module; `schemas` is bound
/// to its exports and the assertion text is on stdin.
pub fn evaluate(bindings: &Bindings, script: &str) -> String {
    let fixtures = repo_root().join("target/ts-fixtures");
    std::fs::create_dir_all(&fixtures).unwrap();
    let dir = tempfile::tempdir_in(&fixtures).unwrap();
    std::fs::write(dir.path().join("schemas.ts"), &bindings.schemas).unwrap();
    let compile = Command::new("node")
        .current_dir(dir.path())
        .arg(node_module("typescript/bin/tsc"))
        .args([
            "--target",
            "ES2022",
            "--module",
            "commonjs",
            "--moduleResolution",
            "bundler",
            "--skipLibCheck",
            "--ignoreConfig",
            "schemas.ts",
        ])
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr)
    );
    let output = Command::new("node")
        .current_dir(dir.path())
        .arg("-e")
        .arg(format!(
            "const {{ z }} = require({zod});\nconst schemas = require('./schemas.js');\n{script}",
            zod = serde_json::to_string(&node_module("zod")).unwrap()
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// Bundles an entry module beside the schema module with esbuild, tree shaking
/// enabled and `zod` external, returning the output text.
pub fn bundle(bindings: &Bindings, entry: &str) -> String {
    let fixtures = repo_root().join("target/ts-fixtures");
    std::fs::create_dir_all(&fixtures).unwrap();
    let dir = tempfile::tempdir_in(&fixtures).unwrap();
    std::fs::write(dir.path().join("schemas.ts"), &bindings.schemas).unwrap();
    std::fs::write(dir.path().join("entry.ts"), entry).unwrap();
    let output = Command::new("node")
        .current_dir(dir.path())
        .arg("-e")
        .arg(format!(
            "const esbuild = require({esbuild});\nconst result = esbuild.buildSync({{ entryPoints: ['entry.ts'], bundle: true, write: false, format: 'esm', treeShaking: true, external: ['zod'] }});\nprocess.stdout.write(result.outputFiles[0].text);",
            esbuild = serde_json::to_string(&node_module("esbuild")).unwrap()
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
