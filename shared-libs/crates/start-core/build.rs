use std::path::Path;

// start-core hosts the start-cli applet, which versions independently of start-core. Bake the
// start-cli crate version in so the applet can report it (see bins::cli_version); the sibling
// manifest is the single source of truth.
fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");

    let manifest = root.join("projects/start-cli/Cargo.toml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    let contents = std::fs::read_to_string(&manifest)
        .unwrap_or_else(|e| panic!("read {}: {e}", manifest.display()));
    let parsed: serde_toml::Table = serde_toml::from_str(&contents)
        .unwrap_or_else(|e| panic!("parse {}: {e}", manifest.display()));
    let version = parsed["package"]["version"]
        .as_str()
        .unwrap_or_else(|| panic!("no [package] version string in {}", manifest.display()));
    println!("cargo:rustc-env=START_CLI_VERSION={version}");

    // The StartOS release version carries a revision segment no Cargo manifest can hold
    // (0.4.0.1), so root package.json is its source of truth. `version::Current` must agree —
    // asserted by version::tests::current_matches_manifest.
    let pkg_json = root.join("package.json");
    println!("cargo:rerun-if-changed={}", pkg_json.display());
    let contents = std::fs::read_to_string(&pkg_json)
        .unwrap_or_else(|e| panic!("read {}: {e}", pkg_json.display()));
    let parsed: serde_json::Value = serde_json::from_str(&contents)
        .unwrap_or_else(|e| panic!("parse {}: {e}", pkg_json.display()));
    let version = parsed["version"]
        .as_str()
        .unwrap_or_else(|| panic!("no version string in {}", pkg_json.display()));
    println!("cargo:rustc-env=STARTOS_VERSION={version}");
}
