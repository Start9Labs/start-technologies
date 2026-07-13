use std::path::{Path, PathBuf};
use std::str::FromStr;

use clap::Parser;
use ed25519_dalek::SigningKey;
use futures::future::{BoxFuture, FutureExt};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::PackageId;
use crate::context::CliContext;
use crate::developer::write_developer_key;
use crate::prelude::*;
use crate::util::Invoke;
use crate::util::serde::IoFormat;

/// Per-workspace state directory, created at the workspace root. Its presence is
/// the marker that both `init-package` and start-cli walk up from cwd to find;
/// `schema` (in config.yaml) lets a future change migrate the contract.
pub const STARTOS_DIR: &str = ".startos";
/// Workspace-scoped ed25519 signing key (PKCS#8 PEM). Generated once and never
/// overwritten — it signs the packages built in this workspace.
pub const BUILD_KEY_FILE: &str = "build-key";
/// Workspace config: named host and registry targets the packager switches
/// between. Scaffolded with defaults; the `host` entries are placeholders to
/// point at your own StartOS boxes.
const CONFIG_FILE: &str = "config.yaml";
const WORKSPACE_CONFIG_CONTENTS: &str = r#"schema: 1
host:
  default: https://dev-vm.local
  prod: https://prodbox.local
registry:
  default: https://alpha-registry-x.start9.com
  beta: https://beta-registry.start9.com
  prod: https://registry.start9.com
"#;

/// Published packaging guide, surfaced when init-workspace refuses to scaffold inside
/// a package repo.
const DOCS_URL: &str = "https://docs.start9.com/packaging/environment-setup.html";

/// Default source for the packaging guide (which also carries the package template).
/// The guide lives in the start-technologies monorepo, and the clone takes the whole
/// tree: past the guide, packagers get the SDK and StartOS source to answer questions
/// the guide can't, and a repo they can open a fix PR from. `--filter=blob:none` keeps
/// it cheap (blobs fetch on demand) while leaving full history, so `log`/`blame`/rebase
/// all work. Re-point the `start-technologies` remote afterward to track a fork; the
/// session-start sync follows whatever remote is configured.
const MONOREPO_URL: &str = "https://github.com/Start9Labs/start-technologies.git";
/// Workspace-relative path to the monorepo checkout that carries the guide.
const MONOREPO_DIR: &str = "start-technologies";
/// Symlink target for the workspace `AGENTS.md` — the guide's canonical copy, so
/// a sync keeps the workspace context current with no extra step. It is also a page
/// of the published guide, so packagers can read it without scaffolding a workspace.
const AGENTS_SYMLINK_TARGET: &str =
    "start-technologies/projects/start-sdk/docs/src/agent-context.md";
/// Where the workspace `AGENTS.md` pointed before the context became a guide page. Nothing
/// lives there now, so a workspace scaffolded by an older start-cli has a dangling link;
/// a re-run repoints it.
const LEGACY_AGENTS_SYMLINK_TARGET: &str = "start-technologies/projects/start-sdk/docs/AGENTS.md";
/// Path to the package template inside the cloned guide (joined onto MONOREPO_DIR).
const TEMPLATE_SUBPATH: &str = "projects/start-sdk/docs/package-template";

/// Claude Code does not auto-read `AGENTS.md`, so the workspace `CLAUDE.md`
/// imports both it and the user's local prefs.
const CLAUDE_MD_CONTENTS: &str = "@AGENTS.md\n@AGENTS.local.md\n";

/// Created once and never overwritten by a sync — the user's own context.
const AGENTS_LOCAL_STUB: &str = include_str!("AGENTS.local.md.template");

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
pub struct InitWorkspaceParams {
    #[arg(help = "help.arg.workspace-path")]
    path: Option<PathBuf>,
}

/// Prepare a directory so any AI assistant working in it has the latest packaging
/// context in scope. Clones the guide, links the context files, and provisions
/// `.startos/` (a workspace signing key + target config). Idempotent: a re-run only
/// fills in what's missing; updates to the guide happen via the session-start sync
/// documented in `AGENTS.md`.
pub async fn init_workspace(
    _: CliContext,
    InitWorkspaceParams { path }: InitWorkspaceParams,
) -> Result<(), Error> {
    let root = if let Some(path) = path {
        path
    } else {
        std::env::current_dir().with_kind(ErrorKind::Filesystem)?
    };
    tokio::fs::create_dir_all(&root)
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, root.display().to_string()))?;
    let root = tokio::fs::canonicalize(&root)
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, root.display().to_string()))?;

    // Refuse to turn a package repo into a workspace. A workspace is the *parent*
    // directory that holds package repos; a `*-startos` package is not one. Point at
    // the parent, which is where the workspace belongs.
    if let Some(repo) = find_enclosing_package_repo(&root) {
        let parent = repo.parent().unwrap_or(repo.as_path());
        return Err(Error::new(
            eyre!(
                "{}",
                t!(
                    "s9pk.init.in-package-repo",
                    path = repo.display().to_string(),
                    parent = parent.display().to_string(),
                    docs = DOCS_URL
                )
            ),
            ErrorKind::InvalidRequest,
        ));
    }

    // Nesting is allowed: a workspace inside a workspace is fine. When building,
    // signing, or reading config, start-cli walks up from the cwd and uses the nearest
    // `.startos/`, so a nested workspace transparently overrides an outer one — no need
    // to look at, or refuse, an enclosing workspace here.

    // Provision the guide. Clone only when absent — refreshing is the session-start sync's
    // job, not this command's — so an existing checkout is left untouched. `exists()`
    // follows symlinks, so pointing `start-technologies` at a checkout you already maintain
    // skips the clone entirely.
    let docs = root.join(MONOREPO_DIR);
    if !docs.exists() {
        eprintln!("{}", t!("s9pk.init.cloning-guide"));
        Command::new("git")
            .arg("clone")
            .arg("--filter=blob:none")
            .arg("--branch")
            .arg("master")
            .arg(MONOREPO_URL)
            .arg(&docs)
            .capture(false)
            .invoke(ErrorKind::Git)
            .await?;
    }

    // Symlink (not a copy) so a guide sync keeps the workspace AGENTS.md current.
    // symlink_metadata treats a broken link as present, so a re-run never clobbers.
    let agents = root.join("AGENTS.md");
    if tokio::fs::read_link(&agents)
        .await
        .is_ok_and(|target| target == Path::new(LEGACY_AGENTS_SYMLINK_TARGET))
    {
        tokio::fs::remove_file(&agents)
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, agents.display().to_string()))?;
    }
    if tokio::fs::symlink_metadata(&agents).await.is_err() {
        tokio::fs::symlink(AGENTS_SYMLINK_TARGET, &agents)
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, agents.display().to_string()))?;
    }
    write_if_absent(&root.join("AGENTS.local.md"), AGENTS_LOCAL_STUB).await?;
    write_if_absent(&root.join("CLAUDE.md"), CLAUDE_MD_CONTENTS).await?;
    // .startos/ marks the workspace and holds its signing key + target config. Written
    // last, so only a fully provisioned directory counts as a workspace.
    let startos = root.join(STARTOS_DIR);
    tokio::fs::create_dir_all(&startos)
        .await
        .with_ctx(|_| (ErrorKind::Filesystem, startos.display().to_string()))?;
    write_if_absent(&startos.join(CONFIG_FILE), WORKSPACE_CONFIG_CONTENTS).await?;
    // Generated once and never regenerated — overwriting the build-key would change
    // the workspace's signing identity.
    let build_key = startos.join(BUILD_KEY_FILE);
    if tokio::fs::symlink_metadata(&build_key).await.is_err() {
        // bind before the await so the !Send ThreadRng isn't held across it
        let key = SigningKey::generate(&mut crate::util::crypto::os_rng());
        write_developer_key(&key, &build_key).await?;
    }

    println!(
        "{}",
        t!(
            "s9pk.init.workspace-ready",
            path = root.display().to_string()
        )
    );
    Ok(())
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
pub struct InitPackageParams {
    #[arg(help = "help.arg.package-display-name")]
    name: String,
}

/// Scaffold a new package from the workspace's bundled template, interpolating the
/// display name and a normalized ID, then `npm install` the result. Leaves a
/// `TODO.md` worklist that drives the package from clone to release-ready.
pub async fn init_package(
    _: CliContext,
    InitPackageParams { name }: InitPackageParams,
) -> Result<(), Error> {
    let cwd = std::env::current_dir().with_kind(ErrorKind::Filesystem)?;
    let root = find_workspace_root(&cwd)?.ok_or_else(|| {
        Error::new(
            eyre!("{}", t!("s9pk.init.not-in-workspace")),
            ErrorKind::InvalidRequest,
        )
    })?;

    // Normalize to a candidate ID, then validate it through the manifest's own
    // rules rather than re-implementing them.
    let id = normalize_id(&name);
    PackageId::from_str(&id).map_err(|_| {
        Error::new(
            eyre!(
                "{}",
                t!("s9pk.init.invalid-package-name", name = name.as_str())
            ),
            ErrorKind::InvalidId,
        )
    })?;

    let dst = cwd.join(format!("{id}-startos"));
    if dst.exists() {
        return Err(Error::new(
            eyre!(
                "{}",
                t!("s9pk.init.package-exists", path = dst.display().to_string())
            ),
            ErrorKind::InvalidRequest,
        ));
    }

    let template = root.join(MONOREPO_DIR).join(TEMPLATE_SUBPATH);
    if !template.exists() {
        return Err(Error::new(
            eyre!(
                "{}",
                t!(
                    "s9pk.init.template-missing",
                    path = template.display().to_string()
                )
            ),
            ErrorKind::NotFound,
        ));
    }

    copy_template_interpolated(&template, &dst, &id, &name).await?;

    // A package is its own git repo — initialize one so version-hash stamping and the
    // shipped CI workflows work. No commit is made; the first commit is the packager's.
    Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(&dst)
        .invoke(ErrorKind::Git)
        .await?;

    eprintln!("{}", t!("s9pk.init.installing-deps"));
    Command::new("npm")
        .arg("install")
        .current_dir(&dst)
        .capture(false)
        .invoke(ErrorKind::Network)
        .await?;

    println!(
        "{}",
        t!(
            "s9pk.init.package-created",
            id = id,
            path = dst.display().to_string()
        )
    );
    Ok(())
}

/// Walk up from `start` (inclusive) for the nearest workspace — a directory whose
/// `.startos` is a provisioned marker (`build-key` or a schema config). `init-package`
/// scaffolds into whatever this returns, so with nested workspaces it targets the
/// innermost one. A bare/legacy `.startos` (no build-key, no schema) isn't a marker.
fn find_workspace_root(start: &Path) -> Result<Option<PathBuf>, Error> {
    let mut dir = start.to_path_buf();
    loop {
        let startos = dir.join(STARTOS_DIR);
        match std::fs::metadata(&startos) {
            Ok(meta) if meta.is_dir() && startos_dir_is_marker(&startos) => {
                return Ok(Some(dir));
            }
            // Present but not a provisioned marker (e.g. the legacy global config) —
            // keep walking up.
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            // EACCES (or any other IO error) on an ancestor — treat as "no
            // accessible workspace here" and stop walking.
            Err(_) => return Ok(None),
        }
        if !dir.pop() {
            return Ok(None);
        }
    }
}

/// Minimal probe: a provisioned workspace `config.yaml` is schema-tagged; a bare or
/// legacy flat config is not, so it fails to deserialize here.
#[derive(Deserialize)]
struct SchemaProbe {
    #[allow(dead_code)]
    schema: u64,
}

/// A `.startos` dir counts as a provisioned workspace once it holds a build-key or a
/// schema-tagged config.
fn startos_dir_is_marker(startos: &Path) -> bool {
    if startos.join(BUILD_KEY_FILE).exists() {
        return true;
    }
    std::fs::File::open(startos.join(CONFIG_FILE))
        .ok()
        .and_then(|f| IoFormat::Yaml.from_reader::<_, SchemaProbe>(f).ok())
        .is_some()
}

/// Heuristic for a StartOS package repo: a `package.json` that depends on the
/// packaging SDK, or the scaffolded `startos/` source layout.
fn is_package_repo(dir: &Path) -> bool {
    if let Ok(pkg) = std::fs::read_to_string(dir.join("package.json")) {
        if pkg.contains("@start9labs/start-sdk") {
            return true;
        }
    }
    let startos = dir.join("startos");
    startos.join("manifest").is_dir() || startos.join("index.ts").is_file()
}

/// Walk up from `start` (inclusive) for the nearest enclosing package repo, so a
/// workspace is never initialized inside one. Stops at a workspace marker: an enclosing
/// workspace is fine (nesting is allowed) and bounds the walk.
fn find_enclosing_package_repo(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        if is_package_repo(&dir) {
            return Some(dir);
        }
        let startos = dir.join(STARTOS_DIR);
        if std::fs::metadata(&startos)
            .map(|m| m.is_dir())
            .unwrap_or(false)
            && startos_dir_is_marker(&startos)
        {
            return None;
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// The error to surface when an s9pk build/sign needs a workspace signing key but none
/// exists above the cwd. We want packagers to run `init-workspace` (it also brings the
/// guide + AGENTS.md/CLAUDE.md), so this points the way rather than falling back to any
/// key. If the cwd is inside a package repo, it names the parent — where the workspace
/// belongs — so an existing package repo is one `init-workspace` away from building.
pub(crate) fn no_workspace_error() -> Error {
    no_workspace_error_at(std::env::current_dir().ok().as_deref())
}

fn no_workspace_error_at(cwd: Option<&Path>) -> Error {
    let msg = match cwd.and_then(find_enclosing_package_repo) {
        Some(repo) => {
            let parent = repo.parent().unwrap_or(repo.as_path());
            t!(
                "s9pk.init.no-workspace-in-package-repo",
                repo = repo.display().to_string(),
                parent = parent.display().to_string(),
                docs = DOCS_URL
            )
            .to_string()
        }
        None => t!("s9pk.init.no-workspace", docs = DOCS_URL).to_string(),
    };
    Error::new(eyre!("{msg}"), ErrorKind::Uninitialized)
}

/// Normalize a human display name to a candidate package ID: lowercase ASCII
/// alphanumerics, runs of whitespace/underscore/hyphen collapsed to a single
/// hyphen, every other character dropped, no leading or trailing hyphen. Validity
/// (non-empty, matches the manifest's ID rules) is the caller's check.
fn normalize_id(name: &str) -> String {
    let mut out = String::new();
    let mut pending_sep = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_sep && !out.is_empty() {
                out.push('-');
            }
            pending_sep = false;
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() || ch == '_' || ch == '-' {
            pending_sep = true;
        }
        // any other character is dropped without forcing a separator
    }
    out
}

/// Recursively copy `src` to `dst`, replacing `{{id}}`/`{{name}}` in text files.
/// `{{id}}` is already validated as safe; `{{name}}` is escaped for TypeScript
/// string literals when writing `.ts` files. Non-UTF-8 files are copied verbatim.
fn copy_template_interpolated<'a>(
    src: &'a Path,
    dst: &'a Path,
    id: &'a str,
    name: &'a str,
) -> BoxFuture<'a, Result<(), Error>> {
    async move {
        tokio::fs::create_dir_all(dst)
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, dst.display().to_string()))?;
        let mut entries = tokio::fs::read_dir(src)
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, src.display().to_string()))?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .with_kind(ErrorKind::Filesystem)?
        {
            let file_name = entry.file_name();
            // Defensive: never carry build/VCS cruft into the scaffold.
            if matches!(
                file_name.to_str(),
                Some("node_modules") | Some(".git") | Some("javascript")
            ) {
                continue;
            }
            let from = entry.path();
            let to = dst.join(&file_name);
            let file_type = entry.file_type().await.with_kind(ErrorKind::Filesystem)?;
            if file_type.is_dir() {
                copy_template_interpolated(&from, &to, id, name).await?;
            } else if file_type.is_file() {
                let bytes = tokio::fs::read(&from)
                    .await
                    .with_ctx(|_| (ErrorKind::Filesystem, from.display().to_string()))?;
                let escape_for_ts = from.extension().and_then(|e| e.to_str()) == Some("ts");
                let rendered = match String::from_utf8(bytes) {
                    Ok(text) => interpolate(&text, id, name, escape_for_ts).into_bytes(),
                    Err(e) => e.into_bytes(),
                };
                tokio::fs::write(&to, rendered)
                    .await
                    .with_ctx(|_| (ErrorKind::Filesystem, to.display().to_string()))?;
            }
            // symlinks (none expected in the template) are skipped
        }
        Ok(())
    }
    .boxed()
}

fn interpolate(content: &str, id: &str, name: &str, escape_for_ts: bool) -> String {
    let name = if escape_for_ts {
        name.replace('\\', "\\\\").replace('\'', "\\'")
    } else {
        name.to_owned()
    };
    content.replace("{{id}}", id).replace("{{name}}", &name)
}

/// Write `contents` to `path` only if nothing is there yet (a broken symlink
/// counts as present, so a re-run never clobbers).
async fn write_if_absent(path: &Path, contents: &str) -> Result<(), Error> {
    if tokio::fs::symlink_metadata(path).await.is_err() {
        tokio::fs::write(path, contents)
            .await
            .with_ctx(|_| (ErrorKind::Filesystem, path.display().to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// A fresh, empty temp dir unique to each call (no external crate needed).
    fn tmp() -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "s9pk-init-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn scaffolded_config_parses_as_workspace() {
        // the config init-workspace writes must load as the unified ClientConfig (its
        // schema marker + host/registry profile maps), which is what the runtime reads
        // — not merely as schema-tagged YAML.
        let config = IoFormat::Yaml
            .from_reader::<_, crate::context::config::ClientConfig>(
                WORKSPACE_CONFIG_CONTENTS.as_bytes(),
            )
            .unwrap();
        assert!(config.schema.is_some());
        assert!(config.host.is_some_and(|host| !host.0.is_empty()));
    }

    #[test]
    fn marker_requires_build_key_or_schema_config() {
        // empty .startos — not a marker
        let d = tmp();
        let startos = d.join(STARTOS_DIR);
        std::fs::create_dir_all(&startos).unwrap();
        assert!(!startos_dir_is_marker(&startos));

        // legacy flat config (no schema) — not a marker
        std::fs::write(startos.join(CONFIG_FILE), "host: https://x\n").unwrap();
        assert!(!startos_dir_is_marker(&startos));

        // schema config — marker
        std::fs::write(startos.join(CONFIG_FILE), WORKSPACE_CONFIG_CONTENTS).unwrap();
        assert!(startos_dir_is_marker(&startos));

        // build-key alone — marker (even with a flat config)
        let d2 = tmp();
        let s2 = d2.join(STARTOS_DIR);
        std::fs::create_dir_all(&s2).unwrap();
        std::fs::write(s2.join(CONFIG_FILE), "host: https://x\n").unwrap();
        std::fs::write(s2.join(BUILD_KEY_FILE), "key").unwrap();
        assert!(startos_dir_is_marker(&s2));
    }

    #[test]
    fn detects_package_repo() {
        let d = tmp();
        assert!(!is_package_repo(&d));

        std::fs::write(
            d.join("package.json"),
            r#"{ "dependencies": { "@start9labs/start-sdk": "2.0.1" } }"#,
        )
        .unwrap();
        assert!(is_package_repo(&d));

        let d2 = tmp();
        std::fs::create_dir_all(d2.join("startos").join("manifest")).unwrap();
        assert!(is_package_repo(&d2));

        let d3 = tmp();
        std::fs::create_dir_all(d3.join("startos")).unwrap();
        std::fs::write(d3.join("startos").join("index.ts"), "export {}").unwrap();
        assert!(is_package_repo(&d3));
    }

    #[test]
    fn enclosing_package_repo_walks_up_and_stops_at_workspace() {
        // a package repo with a nested subdir
        let repo = tmp();
        std::fs::write(
            repo.join("package.json"),
            r#"{ "dependencies": { "@start9labs/start-sdk": "2.0.1" } }"#,
        )
        .unwrap();
        let nested = repo.join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        assert_eq!(
            find_enclosing_package_repo(&nested).and_then(|p| p.canonicalize().ok()),
            repo.canonicalize().ok()
        );

        // a workspace marker between the dir and any package repo halts the walk
        let ws = tmp();
        std::fs::create_dir_all(ws.join(STARTOS_DIR)).unwrap();
        std::fs::write(
            ws.join(STARTOS_DIR).join(CONFIG_FILE),
            WORKSPACE_CONFIG_CONTENTS,
        )
        .unwrap();
        let inside = ws.join("pkgs");
        std::fs::create_dir_all(&inside).unwrap();
        assert_eq!(find_enclosing_package_repo(&inside), None);
    }

    #[test]
    fn no_workspace_error_points_at_package_repo_parent() {
        let repo = tmp();
        std::fs::write(
            repo.join("package.json"),
            r#"{ "dependencies": { "@start9labs/start-sdk": "2.0.1" } }"#,
        )
        .unwrap();
        let sub = repo.join("startos");
        std::fs::create_dir_all(&sub).unwrap();

        // inside a package repo → the error names the parent (the workspace location)
        let msg = format!("{}", no_workspace_error_at(Some(&sub)));
        assert!(
            msg.contains(&repo.parent().unwrap().display().to_string()),
            "{msg}"
        );
        assert!(msg.contains("init-workspace"), "{msg}");

        // not in a package repo → generic message, still directs to init-workspace
        let msg = format!("{}", no_workspace_error_at(Some(&tmp())));
        assert!(msg.contains("init-workspace"), "{msg}");
    }
}
