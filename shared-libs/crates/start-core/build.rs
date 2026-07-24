use std::path::Path;

// start-core hosts the start-cli applet, which versions independently of start-core. Bake the
// start-cli crate version in so the applet can report it (see bins::cli_version); the sibling
// manifest is the single source of truth.
fn main() {
    let manifest =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../projects/start-cli/Cargo.toml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    let contents = std::fs::read_to_string(&manifest)
        .unwrap_or_else(|e| panic!("read {}: {e}", manifest.display()));
    let parsed: serde_toml::Table = serde_toml::from_str(&contents)
        .unwrap_or_else(|e| panic!("parse {}: {e}", manifest.display()));
    let version = parsed["package"]["version"]
        .as_str()
        .unwrap_or_else(|| panic!("no [package] version string in {}", manifest.display()));
    println!("cargo:rustc-env=START_CLI_VERSION={version}");
}
