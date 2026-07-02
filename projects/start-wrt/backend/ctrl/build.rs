use std::process::Command;

fn main() {
    // include_dir! in embedded_web.rs panics at macro expansion if the web dist
    // dir is missing — which it is on a fresh clone or in any build that never
    // runs the Angular build (cargo check/test, CI's `make test`). Ensure it
    // exists; empty is fine for those builds, and `make startwrt` always
    // populates it first via the $(STARTWRT_WEB_DIST) prerequisite.
    std::fs::create_dir_all("../../web/dist/startwrt/browser")
        .expect("failed to create web dist placeholder dir");

    // Prefer STARTWRT_GIT_HASH from the environment (set by build-rust.sh on
    // the host before entering the Docker container). Fall back to running git
    // directly for local dev builds.
    let hash = std::env::var("STARTWRT_GIT_HASH")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            let short = Command::new("git")
                .args(["rev-parse", "--short", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|| "unknown".into());

            let dirty = Command::new("git")
                .args(["diff-index", "--quiet", "HEAD", "--"])
                .status()
                .map(|s| !s.success())
                .unwrap_or(false);

            if dirty {
                format!("{short}-dirty")
            } else {
                short
            }
        });

    println!("cargo:rustc-env=STARTWRT_GIT_HASH={hash}");
    println!("cargo:rerun-if-env-changed=STARTWRT_GIT_HASH");

    // Re-embed the UI when the built web bundle changes. embedded_web.rs bakes in
    // web/dist via include_dir!, but include_dir does not register the embedded
    // files as cargo deps, so without this cargo skips recompiling this crate on a
    // web-only change and the binary ships a stale UI. Path is relative to this
    // build script's cwd (CARGO_MANIFEST_DIR = backend/ctrl), mirroring the
    // include_dir! path. Angular emits content-hashed filenames, so any content
    // change alters the directory listing and is reliably picked up.
    println!("cargo:rerun-if-changed=../../web/dist/startwrt/browser");
}
