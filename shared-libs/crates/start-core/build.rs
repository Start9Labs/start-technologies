use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../../..");

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

    // StartOS versions can have four segments.
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
