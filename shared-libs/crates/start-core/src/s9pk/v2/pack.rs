use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, LazyLock, OnceLock};

use clap::Parser;
use futures::future::{BoxFuture, ready};
use futures::{FutureExt, TryStreamExt};
use imbl_value::InternedString;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::sync::OnceCell;
use tokio_stream::wrappers::ReadDirStream;
use tracing::{debug, warn};
use ts_rs::TS;

use crate::context::CliContext;
use crate::dependencies::{DependencyMetadata, MetadataSrc};
use crate::prelude::*;
use crate::rpc_continuations::Guid;
use crate::s9pk::S9pk;
use crate::s9pk::git_hash::GitHash;
use crate::s9pk::manifest::{LocaleString, Manifest};
use crate::s9pk::merkle_archive::directory_contents::DirectoryContents;
use crate::s9pk::merkle_archive::source::http::HttpSource;
use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
use crate::s9pk::merkle_archive::source::{
    ArchiveSource, DynFileSource, DynRead, FileSource, TmpSource, into_dyn_read,
};
use crate::s9pk::merkle_archive::{Entry, MerkleArchive};
use crate::s9pk::v2::SIG_CONTEXT;
use crate::util::io::{TmpDir, create_file, open_file};
use crate::util::serde::IoFormat;
use crate::util::{DataUrl, Invoke, PathOrUrl, VersionString, new_guid};
use crate::{ImageId, PackageId};

pub static PREFER_DOCKER: OnceLock<bool> = OnceLock::new();

pub static CONTAINER_TOOL: LazyLock<&'static str> = LazyLock::new(|| {
    if *PREFER_DOCKER.get_or_init(|| false) {
        if std::process::Command::new("which")
            .arg("docker")
            .stdout(Stdio::null())
            .status()
            .map_or(false, |o| o.success())
        {
            "docker"
        } else {
            "podman"
        }
    } else {
        "podman"
    }
});
pub static CONTAINER_DATADIR: LazyLock<&'static str> = LazyLock::new(|| {
    if *CONTAINER_TOOL == "docker" {
        "/var/lib/docker"
    } else {
        "/var/lib/containers"
    }
});

pub struct SqfsDir {
    path: PathBuf,
    tmpdir: Arc<TmpDir>,
    permission_action: Option<&'static str>,
    sqfs: OnceCell<MultiCursorFile>,
}
impl SqfsDir {
    pub fn new(path: PathBuf, tmpdir: Arc<TmpDir>) -> Self {
        Self {
            path,
            tmpdir,
            permission_action: None,
            sqfs: OnceCell::new(),
        }
    }
    fn world_readable(path: PathBuf, tmpdir: Arc<TmpDir>) -> Self {
        Self {
            permission_action: Some("chmod(ugo+rX)@true"),
            ..Self::new(path, tmpdir)
        }
    }
    async fn file(&self) -> Result<&MultiCursorFile, Error> {
        self.sqfs
            .get_or_try_init(|| async move {
                let guid = Guid::new();
                let path = self.tmpdir.join(guid.as_ref()).with_extension("squashfs");
                if self.path.extension().and_then(|s| s.to_str()) == Some("tar") {
                    tar2sqfs(&self.path)?
                        .input(Some(&mut open_file(&self.path).await?))
                        .invoke(ErrorKind::Filesystem)
                        .await?;
                } else {
                    let mut command = Command::new("mksquashfs");
                    command.arg(&self.path).arg(&path).arg("-quiet");
                    if let Some(action) = self.permission_action {
                        let source_mode = tokio::fs::metadata(&self.path)
                            .await
                            .with_ctx(|_| (ErrorKind::Filesystem, self.path.display()))?
                            .permissions()
                            .mode()
                            & 0o7777;
                        let root_mode = source_mode | 0o555;
                        command
                            .arg("-root-mode")
                            .arg(format!("{root_mode:o}"))
                            .arg("-action")
                            .arg(action);
                    }
                    command.invoke(ErrorKind::Filesystem).await?;
                }

                Ok(MultiCursorFile::from(
                    open_file(&path)
                        .await
                        .with_ctx(|_| (ErrorKind::Filesystem, path.display()))?,
                ))
            })
            .await
    }
}

#[derive(Clone)]
pub enum PackSource {
    Buffered(Arc<[u8]>),
    File(PathBuf),
    Squashfs(Arc<SqfsDir>),
}
impl FileSource for PackSource {
    type Reader = DynRead;
    type SliceReader = DynRead;
    async fn size(&self) -> Result<u64, Error> {
        match self {
            Self::Buffered(a) => Ok(a.len() as u64),
            Self::File(f) => Ok(tokio::fs::metadata(f)
                .await
                .with_ctx(|_| (ErrorKind::Filesystem, f.display()))?
                .len()),
            Self::Squashfs(dir) => dir
                .file()
                .await
                .with_ctx(|_| (ErrorKind::Filesystem, dir.path.display()))?
                .size()
                .await
                .or_not_found("file metadata"),
        }
    }
    async fn reader(&self) -> Result<Self::Reader, Error> {
        match self {
            Self::Buffered(a) => Ok(into_dyn_read(FileSource::reader(a).await?)),
            Self::File(f) => Ok(into_dyn_read(FileSource::reader(f).await?)),
            Self::Squashfs(dir) => dir.file().await?.fetch_all().await.map(into_dyn_read),
        }
    }
    async fn slice(&self, position: u64, size: u64) -> Result<Self::SliceReader, Error> {
        match self {
            Self::Buffered(a) => Ok(into_dyn_read(FileSource::slice(a, position, size).await?)),
            Self::File(f) => Ok(into_dyn_read(FileSource::slice(f, position, size).await?)),
            Self::Squashfs(dir) => dir
                .file()
                .await?
                .fetch(position, size)
                .await
                .map(into_dyn_read),
        }
    }
}
impl From<PackSource> for DynFileSource {
    fn from(value: PackSource) -> Self {
        DynFileSource::new(value)
    }
}

#[derive(Deserialize, Serialize, Parser)]
#[group(skip)]
pub struct PackParams {
    #[arg(help = "help.arg.input-path")]
    pub path: Option<PathBuf>,
    #[arg(short, long, help = "help.arg.output-path")]
    pub output: Option<PathBuf>,
    #[arg(long, help = "help.arg.javascript-path")]
    pub javascript: Option<PathBuf>,
    #[arg(long, help = "help.arg.icon-path")]
    pub icon: Option<PathBuf>,
    #[arg(long, help = "help.arg.license-path")]
    pub license: Option<PathBuf>,
    #[arg(long, help = "help.arg.instructions-path")]
    pub instructions: Option<PathBuf>,
    #[arg(long, conflicts_with = "no_assets", help = "help.arg.assets-path")]
    pub assets: Option<PathBuf>,
    #[arg(long, conflicts_with = "assets", help = "help.arg.no-assets")]
    pub no_assets: bool,
    #[arg(long, help = "help.arg.architecture-mask")]
    pub arch: Vec<InternedString>,
}
impl PackParams {
    fn path(&self) -> &Path {
        self.path.as_deref().unwrap_or(Path::new("."))
    }
    fn output(&self, id: &PackageId) -> PathBuf {
        self.output
            .as_ref()
            .cloned()
            .unwrap_or_else(|| self.path().join(id).with_extension("s9pk"))
    }
    fn javascript(&self) -> PathBuf {
        self.javascript
            .as_ref()
            .cloned()
            .unwrap_or_else(|| self.path().join("javascript"))
    }
    async fn icon(&self) -> Result<PathBuf, Error> {
        if let Some(icon) = &self.icon {
            Ok(icon.clone())
        } else {
            ReadDirStream::new(tokio::fs::read_dir(self.path()).await?)
                .try_filter(|x| {
                    ready(
                        x.path()
                            .file_stem()
                            .map_or(false, |s| s.eq_ignore_ascii_case("icon")),
                    )
                })
                .map_err(Error::from)
                .try_fold(
                    Err(Error::new(eyre!("icon not found"), ErrorKind::NotFound)),
                    |acc, x| async move {
                        match acc {
                            Ok(_) => Err(Error::new(eyre!("multiple icons found in working directory, please specify which to use with `--icon`"), ErrorKind::InvalidRequest)),
                            Err(e) => Ok({
                                let path = x.path();
                                if path
                                    .file_stem()
                                    .map_or(false, |s| s.eq_ignore_ascii_case("icon"))
                                {
                                    Ok(path)
                                } else {
                                    Err(e)
                                }
                            }),
                        }
                    },
                )
                .await?
        }
    }
    async fn license(&self) -> Result<PathBuf, Error> {
        if let Some(license) = &self.license {
            Ok(license.clone())
        } else {
            ReadDirStream::new(tokio::fs::read_dir(self.path()).await?)
                .try_filter(|x| {
                    ready(
                        x.path()
                            .file_stem()
                            .map_or(false, |s| s.eq_ignore_ascii_case("license")),
                    )
                })
                .map_err(Error::from)
                .try_fold(
                    Err(Error::new(eyre!("license not found"), ErrorKind::NotFound)),
                    |acc, x| async move {
                        match acc {
                            Ok(_) => Err(Error::new(eyre!("multiple licenses found in working directory, please specify which to use with `--license`"), ErrorKind::InvalidRequest)),
                            Err(e) => Ok({
                                let path = x.path();
                                if path
                                    .file_stem()
                                    .map_or(false, |s| s.eq_ignore_ascii_case("license"))
                                {
                                    Ok(path)
                                } else {
                                    Err(e)
                                }
                            }),
                        }
                    },
                )
                .await?
        }
    }
    async fn instructions(&self) -> Result<PathBuf, Error> {
        if let Some(instructions) = &self.instructions {
            Ok(instructions.clone())
        } else {
            let candidate = self.path().join("instructions.md");
            if tokio::fs::metadata(&candidate).await.is_ok() {
                Ok(candidate)
            } else {
                Err(Error::new(
                    eyre!(
                        "instructions.md not found at the package root - it is a required file alongside README.md"
                    ),
                    ErrorKind::NotFound,
                ))
            }
        }
    }
    fn assets(&self) -> PathBuf {
        self.assets
            .as_ref()
            .cloned()
            .unwrap_or_else(|| self.path().join("assets"))
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImageConfig {
    pub source: ImageSource,
    #[ts(type = "string[]")]
    pub arch: BTreeSet<InternedString>,
    #[ts(type = "string | null")]
    pub emulate_missing_as: Option<InternedString>,
    #[serde(default)]
    pub nvidia_container: bool,
}

#[derive(Parser)]
#[group(skip)]
struct CliImageConfig {
    #[arg(long, conflicts_with("docker_tag"), help = "help.arg.docker-build")]
    docker_build: bool,
    #[arg(long, requires("docker_build"), help = "help.arg.dockerfile-path")]
    dockerfile: Option<PathBuf>,
    #[arg(long, requires("docker_build"), help = "help.arg.workdir-path")]
    workdir: Option<PathBuf>,
    #[arg(long, conflicts_with_all(["dockerfile", "workdir"]), help = "help.arg.docker-tag")]
    docker_tag: Option<String>,
    #[arg(long, help = "help.arg.architecture-mask")]
    arch: Vec<InternedString>,
    #[arg(long, help = "help.arg.emulate-missing-arch")]
    emulate_missing_as: Option<InternedString>,
    #[arg(long, help = "help.arg.nvidia-container")]
    nvidia_container: bool,
}
impl TryFrom<CliImageConfig> for ImageConfig {
    type Error = clap::Error;
    fn try_from(value: CliImageConfig) -> Result<Self, Self::Error> {
        let res = Self {
            source: if value.docker_build {
                ImageSource::DockerBuild {
                    dockerfile: value.dockerfile,
                    workdir: value.workdir,
                    build_args: None,
                }
            } else if let Some(tag) = value.docker_tag {
                ImageSource::DockerTag(tag)
            } else {
                ImageSource::Packed
            },
            arch: value.arch.into_iter().collect(),
            emulate_missing_as: value.emulate_missing_as,
            nvidia_container: value.nvidia_container,
        };
        res.emulate_missing_as
            .as_ref()
            .map(|a| {
                if !res.arch.contains(a) {
                    Err(clap::Error::raw(
                        clap::error::ErrorKind::InvalidValue,
                        "`emulate-missing-as` must match one of the provided `arch`es",
                    ))
                } else {
                    Ok(())
                }
            })
            .transpose()?;
        Ok(res)
    }
}
impl clap::Args for ImageConfig {
    fn augment_args(cmd: clap::Command) -> clap::Command {
        CliImageConfig::augment_args(cmd)
    }
    fn augment_args_for_update(cmd: clap::Command) -> clap::Command {
        CliImageConfig::augment_args_for_update(cmd)
    }
}
impl clap::FromArgMatches for ImageConfig {
    fn from_arg_matches(matches: &clap::ArgMatches) -> Result<Self, clap::Error> {
        Self::try_from(CliImageConfig::from_arg_matches(matches)?)
    }
    fn update_from_arg_matches(&mut self, matches: &clap::ArgMatches) -> Result<(), clap::Error> {
        *self = Self::try_from(CliImageConfig::from_arg_matches(matches)?)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)]
#[ts(export)]
pub enum BuildArg {
    String(String),
    EnvVar { env: String },
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ImageSource {
    Packed,
    #[serde(rename_all = "camelCase")]
    DockerBuild {
        #[ts(optional)]
        workdir: Option<PathBuf>,
        #[ts(optional)]
        dockerfile: Option<PathBuf>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        build_args: Option<BTreeMap<String, BuildArg>>,
    },
    DockerTag(String),
    // Recipe(DirRecipe),
}
impl Default for ImageSource {
    fn default() -> Self {
        ImageSource::Packed
    }
}
impl ImageSource {
    pub fn ingredients(&self) -> Vec<PathBuf> {
        match self {
            Self::Packed => Vec::new(),
            Self::DockerBuild {
                dockerfile,
                workdir,
                ..
            } => {
                vec![dockerfile.clone().unwrap_or_else(|| {
                    workdir
                        .as_deref()
                        .unwrap_or(Path::new("."))
                        .join("Dockerfile")
                })]
            }
            Self::DockerTag(_) => Vec::new(),
        }
    }
    #[instrument(skip_all)]
    pub fn load<'a, S: From<TmpSource<PackSource>> + FileSource + Clone>(
        &'a self,
        tmp_dir: Arc<TmpDir>,
        id: &'a PackageId,
        version: &'a VersionString,
        image_id: &'a ImageId,
        arch: &'a str,
        into: &'a mut DirectoryContents<S>,
    ) -> BoxFuture<'a, Result<(), Error>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct DockerImageConfig {
            env: Vec<String>,
            #[serde(default)]
            working_dir: PathBuf,
            #[serde(default)]
            user: String,
            entrypoint: Option<Vec<String>>,
            cmd: Option<Vec<String>>,
        }
        async move {
            match self {
                ImageSource::Packed => Ok(()),
                ImageSource::DockerBuild {
                    workdir,
                    dockerfile,
                    build_args,
                } => {
                    let workdir = workdir.as_deref().unwrap_or(Path::new("."));
                    let dockerfile = dockerfile
                        .clone()
                        .unwrap_or_else(|| workdir.join("Dockerfile"));
                    let docker_platform = if arch == "x86_64" {
                        "--platform=linux/amd64".to_owned()
                    } else if arch == "aarch64" {
                        "--platform=linux/arm64".to_owned()
                    } else {
                        format!("--platform=linux/{arch}")
                    };
                    // docker buildx build ${path} -o type=image,name=start9/${id}
                    let tag = format!("start9/{id}/{image_id}:{}", new_guid());
                    let mut command = Command::new(*CONTAINER_TOOL);
                    if *CONTAINER_TOOL == "docker" {
                        command.arg("buildx");
                    }
                    command
                        .arg("build")
                        .arg(workdir)
                        .arg("-f")
                        .arg(dockerfile)
                        .arg("-t")
                        .arg(&tag)
                        .arg(&docker_platform)
                        .arg("--build-arg")
                        .arg(format!("ARCH={}", arch));

                    // add build arguments
                    if let Some(build_args) = build_args {
                        for (key, value) in build_args {
                            let build_arg_value = match value {
                                BuildArg::String(val) => val.to_string(),
                                BuildArg::EnvVar { env } => {
                                    match std::env::var(&env) {
                                        Ok(val) => val,
                                        Err(_) => continue, // skip if env var not set or invalid
                                    }
                                }
                            };

                            command
                                .arg("--build-arg")
                                .arg(format!("{}={}", key, build_arg_value));
                        }
                    }

                    command
                        .arg("-o")
                        .arg("type=docker,dest=-")
                        .capture(false)
                        .pipe(Command::new(*CONTAINER_TOOL).arg("load"))
                        .invoke(ErrorKind::Docker)
                        .await?;
                    ImageSource::DockerTag(tag.clone())
                        .load(tmp_dir, id, version, image_id, arch, into)
                        .await?;
                    Command::new(*CONTAINER_TOOL)
                        .arg("rmi")
                        .arg("-f")
                        .arg(&tag)
                        .invoke(ErrorKind::Docker)
                        .await?;
                    Ok(())
                }
                ImageSource::DockerTag(tag) => {
                    let docker_platform = if arch == "x86_64" {
                        "--platform=linux/amd64".to_owned()
                    } else if arch == "aarch64" {
                        "--platform=linux/arm64".to_owned()
                    } else {
                        format!("--platform=linux/{arch}")
                    };
                    let container = String::from_utf8(
                        Command::new(*CONTAINER_TOOL)
                            .arg("create")
                            .arg(&docker_platform)
                            .arg(&tag)
                            .invoke(ErrorKind::Docker)
                            .await?,
                    )?;
                    let container = container.trim();
                    let config = serde_json::from_slice::<DockerImageConfig>(
                        &Command::new(*CONTAINER_TOOL)
                            .arg("container")
                            .arg("inspect")
                            .arg("--format")
                            .arg("{{json .Config}}")
                            .arg(container)
                            .invoke(ErrorKind::Docker)
                            .await?,
                    )
                    .with_kind(ErrorKind::Deserialization)?;
                    let base_path = Path::new("images").join(arch).join(image_id);
                    into.insert_path(
                        base_path.with_extension("json"),
                        Entry::file(
                            TmpSource::new(
                                tmp_dir.clone(),
                                PackSource::Buffered(
                                    serde_json::to_vec(&ImageMetadata {
                                        workdir: if config.working_dir == Path::new("") {
                                            "/".into()
                                        } else {
                                            config.working_dir
                                        },
                                        user: if config.user.is_empty() {
                                            "root".into()
                                        } else {
                                            config.user.into()
                                        },
                                        entrypoint: config.entrypoint,
                                        cmd: config.cmd,
                                    })
                                    .with_kind(ErrorKind::Serialization)?
                                    .into(),
                                ),
                            )
                            .into(),
                        ),
                    )?;
                    into.insert_path(
                        base_path.with_extension("env"),
                        Entry::file(
                            TmpSource::new(
                                tmp_dir.clone(),
                                PackSource::Buffered(config.env.join("\n").into_bytes().into()),
                            )
                            .into(),
                        ),
                    )?;
                    let dest = tmp_dir
                        .join(Guid::new().as_ref())
                        .with_extension("squashfs");

                    Command::new(*CONTAINER_TOOL)
                        .arg("export")
                        .arg(container)
                        .pipe(&mut tar2sqfs(&dest)?)
                        .capture(false)
                        .invoke(ErrorKind::Docker)
                        .await?;
                    Command::new(*CONTAINER_TOOL)
                        .arg("rm")
                        .arg(container)
                        .invoke(ErrorKind::Docker)
                        .await?;
                    into.insert_path(
                        base_path.with_extension("squashfs"),
                        Entry::file(TmpSource::new(tmp_dir.clone(), PackSource::File(dest)).into()),
                    )?;

                    Ok(())
                }
            }
        }
        .boxed()
    }
}

fn tar2sqfs(dest: impl AsRef<Path>) -> Result<Command, Error> {
    let dest = dest.as_ref();

    Ok({
        #[cfg(target_os = "linux")]
        {
            let mut command = Command::new("tar2sqfs");
            command.arg("-q").arg(&dest);
            command
        }
        #[cfg(target_os = "macos")]
        {
            let directory = dest
                .parent()
                .unwrap_or_else(|| Path::new("/"))
                .to_path_buf();
            let mut command = Command::new(*CONTAINER_TOOL);
            command
                .arg("run")
                .arg("-i")
                .arg("--rm")
                .arg("--mount")
                .arg(format!("type=bind,src={},dst=/data", directory.display()))
                .arg("ghcr.io/start9labs/sdk/utils:latest")
                .arg("tar2sqfs")
                .arg("-q")
                .arg(Path::new("/data").join(&dest.file_name().unwrap_or_default()));
            command
        }
    })
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImageMetadata {
    pub workdir: PathBuf,
    #[ts(type = "string")]
    pub user: InternedString,
    pub entrypoint: Option<Vec<String>>,
    pub cmd: Option<Vec<String>>,
}

#[instrument(skip_all)]
pub async fn pack(ctx: CliContext, params: PackParams) -> Result<(), Error> {
    let tmp_dir = Arc::new(TmpDir::new().await?);
    let mut files = DirectoryContents::<TmpSource<PackSource>>::new();
    let js_dir = params.javascript();
    let manifest: Arc<[u8]> = Command::new("node")
        .arg("-e")
        .arg(format!(
            "console.log(JSON.stringify(require('{}/index.js').manifest))",
            js_dir.display()
        ))
        .invoke(ErrorKind::Javascript)
        .await?
        .into();
    files.insert(
        "manifest.json".into(),
        Entry::file(TmpSource::new(
            tmp_dir.clone(),
            PackSource::Buffered(manifest.clone()),
        )),
    );
    let icon = params.icon().await?;
    let icon_ext = icon
        .extension()
        .or_not_found("icon file extension")?
        .to_string_lossy();
    files.insert(
        InternedString::from_display(&lazy_format!("icon.{}", icon_ext)),
        Entry::file(TmpSource::new(tmp_dir.clone(), PackSource::File(icon))),
    );
    files.insert(
        "LICENSE.md".into(),
        Entry::file(TmpSource::new(
            tmp_dir.clone(),
            PackSource::File(params.license().await?),
        )),
    );
    files.insert(
        "instructions.md".into(),
        Entry::file(TmpSource::new(
            tmp_dir.clone(),
            PackSource::File(params.instructions().await?),
        )),
    );
    files.insert(
        "javascript.squashfs".into(),
        Entry::file(TmpSource::new(
            tmp_dir.clone(),
            PackSource::Squashfs(Arc::new(SqfsDir::world_readable(js_dir, tmp_dir.clone()))),
        )),
    );

    let mut s9pk = S9pk::new(
        MerkleArchive::new(files, ctx.build_key()?, SIG_CONTEXT),
        None,
    )
    .await?;

    let manifest = s9pk.as_manifest_mut();
    manifest.metadata.git_hash = GitHash::from_path(params.path()).await?;
    // Surface the omission so a genuinely broken/misconfigured git isn't silent; for a
    // fresh scaffold this doubles as a nudge to make the first commit.
    if manifest.metadata.git_hash.is_none() {
        eprintln!(
            "{}",
            t!(
                "s9pk.pack.git-hash-omitted",
                path = params.path().display().to_string()
            )
        );
    }
    if !params.arch.is_empty() {
        let arches: BTreeSet<InternedString> = match manifest.hardware_requirements.arch.take() {
            Some(a) => params
                .arch
                .iter()
                .filter(|x| a.contains(*x))
                .cloned()
                .collect(),
            None => params.arch.iter().cloned().collect(),
        };
        if arches.is_empty() {
            return Err(Error::new(
                eyre!(
                    "none of the requested architectures ({:?}) are supported by this package",
                    params.arch
                ),
                ErrorKind::InvalidRequest,
            ));
        }
        manifest.images.iter_mut().for_each(|(id, c)| {
            let filtered = c
                .arch
                .intersection(&arches)
                .cloned()
                .collect::<BTreeSet<_>>();
            if filtered.is_empty() {
                if let Some(arch) = &c.emulate_missing_as {
                    tracing::warn!(
                        "ImageId {} is not available for {}, emulating as {}",
                        id,
                        arches.iter().join("/"),
                        arch
                    );
                    c.arch = [arch.clone()].into_iter().collect();
                } else {
                    tracing::error!(
                        "ImageId {} is not available for {}",
                        id,
                        arches.iter().join("/"),
                    );
                }
            } else {
                c.arch = filtered;
            }
        });
        manifest.hardware_requirements.arch = Some(arches);
    }

    if !params.no_assets {
        let assets_dir = params.assets();
        s9pk.as_archive_mut().contents_mut().insert_path(
            "assets.squashfs",
            Entry::file(TmpSource::new(
                tmp_dir.clone(),
                PackSource::Squashfs(Arc::new(SqfsDir::new(assets_dir, tmp_dir.clone()))),
            )),
        )?;
    }

    s9pk.load_images(tmp_dir.clone()).await?;

    let mut to_insert = Vec::new();
    for (id, dependency) in &mut s9pk.as_manifest_mut().dependencies.0 {
        if let Some((title, icon)) = match dependency.metadata.take() {
            Some(MetadataSrc::Metadata(metadata)) => {
                let icon = match metadata.icon {
                    PathOrUrl::Path(path) => DataUrl::from_path(path).await?,
                    PathOrUrl::Url(url) => {
                        if url.scheme() == "http" || url.scheme() == "https" {
                            DataUrl::from_response(ctx.client.get(url).send().await?).await?
                        } else if url.scheme() == "data" {
                            url.as_str().parse()?
                        } else {
                            return Err(Error::new(
                                eyre!("unknown scheme: {}", url.scheme()),
                                ErrorKind::InvalidRequest,
                            ));
                        }
                    }
                };
                Some((metadata.title, icon))
            }
            Some(MetadataSrc::S9pk(Some(s9pk))) => {
                let s9pk = match s9pk {
                    PathOrUrl::Path(path) => {
                        S9pk::deserialize(&MultiCursorFile::from(open_file(path).await?), None)
                            .await?
                            .into_dyn()
                    }
                    PathOrUrl::Url(url) => {
                        if url.scheme() == "http" || url.scheme() == "https" {
                            S9pk::deserialize(
                                &Arc::new(HttpSource::new(ctx.client.clone(), url).await?),
                                None,
                            )
                            .await?
                            .into_dyn()
                        } else {
                            return Err(Error::new(
                                eyre!("unknown scheme: {}", url.scheme()),
                                ErrorKind::InvalidRequest,
                            ));
                        }
                    }
                };
                Some((
                    LocaleString::Translated(s9pk.as_manifest().metadata.title.to_string()),
                    s9pk.icon_data_url().await?,
                ))
            }
            Some(MetadataSrc::S9pk(None)) | None => {
                warn!("no metadata specified for {id}, leaving metadata empty");
                None
            }
        } {
            let dep_path = Path::new("dependencies").join(id);
            to_insert.push((
                dep_path.join("metadata.json"),
                Entry::file(TmpSource::new(
                    tmp_dir.clone(),
                    PackSource::Buffered(
                        IoFormat::Json.to_vec(&DependencyMetadata { title })?.into(),
                    ),
                )),
            ));
            to_insert.push((
                dep_path
                    .join("icon")
                    .with_extension(icon.canonical_ext().unwrap_or("ico")),
                Entry::file(TmpSource::new(
                    tmp_dir.clone(),
                    PackSource::Buffered(icon.data.into_owned().into()),
                )),
            ));
        }
    }
    for (path, source) in to_insert {
        s9pk.as_archive_mut()
            .contents_mut()
            .insert_path(path, source)?;
    }

    s9pk.validate_and_filter(None)?;

    s9pk.serialize(
        &mut create_file(params.output(&s9pk.as_manifest().id)).await?,
        false,
    )
    .await?;

    drop(s9pk);

    tmp_dir.gc().await?;

    Ok(())
}

#[instrument(skip_all)]
pub async fn list_ingredients(_: CliContext, params: PackParams) -> Result<Vec<PathBuf>, Error> {
    let js_path = params.javascript().join("index.js");
    let manifest: Manifest = match async {
        serde_json::from_slice(
            &Command::new("node")
                .arg("-e")
                .arg(format!(
                    "console.log(JSON.stringify(require('{}').manifest))",
                    js_path.display()
                ))
                .invoke(ErrorKind::Javascript)
                .await?,
        )
        .with_kind(ErrorKind::Deserialization)
    }
    .await
    {
        Ok(m) => m,
        Err(e) => {
            warn!("failed to load manifest: {e}");
            debug!("{e:?}");
            return Ok(vec![
                js_path,
                params.icon().await?,
                params.license().await?,
                params.instructions().await?,
            ]);
        }
    };
    let mut ingredients = vec![
        js_path,
        params.icon().await?,
        params.license().await?,
        params.instructions().await?,
    ];

    for (_, dependency) in manifest.dependencies.0 {
        match dependency.metadata {
            Some(MetadataSrc::Metadata(crate::dependencies::Metadata {
                icon: PathOrUrl::Path(icon),
                ..
            })) => {
                ingredients.push(icon);
            }
            Some(MetadataSrc::S9pk(Some(PathOrUrl::Path(s9pk)))) => {
                ingredients.push(s9pk);
            }
            _ => (),
        }
    }

    if !params.no_assets {
        let assets_dir = params.assets();
        ingredients.push(assets_dir);
    }

    for image in manifest.images.values() {
        ingredients.extend(image.source.ingredients());
    }

    Ok(ingredients)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::context::config::ClientConfig;
    use crate::developer::write_signing_key;

    const CHILD_ENV: &str = "STARTOS_PACK_PERMISSIONS_TEST_CHILD";
    const TEST_NAME: &str =
        "s9pk::v2::pack::tests::javascript_squashfs_is_readable_after_restrictive_umask";

    fn mode(path: impl AsRef<Path>) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[tokio::test]
    async fn javascript_squashfs_is_readable_after_restrictive_umask() {
        let workspace = Arc::new(TmpDir::new().await.unwrap());
        // Umask is process-global, so isolate it instead of racing other tests.
        if std::env::var_os(CHILD_ENV).is_none() {
            let startos = workspace.join(".startos");
            tokio::fs::create_dir(&startos).await.unwrap();
            write_signing_key(
                &ed25519_dalek::SigningKey::from_bytes(&[7; 32]),
                startos.join("build.key.pem"),
            )
            .await
            .unwrap();

            Command::new("sh")
                .arg("-c")
                .arg(r#"umask 077; exec "$@""#)
                .arg("sh")
                .arg(std::env::current_exe().unwrap())
                .arg(TEST_NAME)
                .arg("--exact")
                .arg("--nocapture")
                .env(CHILD_ENV, &**workspace)
                .current_dir(&*workspace)
                .capture(false)
                .invoke(ErrorKind::Filesystem)
                .await
                .unwrap();
            return;
        }

        let package = workspace.join("package");
        let javascript = package.join("javascript");
        tokio::fs::create_dir_all(javascript.join("nested"))
            .await
            .unwrap();
        tokio::fs::write(
            javascript.join("index.js"),
            r#"module.exports.manifest = {
  id: "permissions",
  version: "1.0.0:0",
  canMigrateTo: "1.0.0:0",
  canMigrateFrom: "1.0.0:0",
  title: "Permissions",
  description: { short: "Test package", long: "Test package" },
  releaseNotes: "Initial test version",
  license: "MIT",
  packageRepo: "https://example.com/package",
  upstreamRepo: "https://example.com/upstream",
  images: {},
  volumes: []
}
"#,
        )
        .await
        .unwrap();
        tokio::fs::write(javascript.join("nested/helper"), "#!/bin/sh\n")
            .await
            .unwrap();
        tokio::fs::write(package.join("icon.png"), [])
            .await
            .unwrap();
        tokio::fs::write(package.join("LICENSE.md"), "MIT\n")
            .await
            .unwrap();
        tokio::fs::write(package.join("instructions.md"), "Test instructions\n")
            .await
            .unwrap();
        tokio::fs::set_permissions(&javascript, std::fs::Permissions::from_mode(0o770))
            .await
            .unwrap();
        tokio::fs::set_permissions(
            javascript.join("nested/helper"),
            std::fs::Permissions::from_mode(0o700),
        )
        .await
        .unwrap();

        assert_eq!(mode(&javascript), 0o770);
        assert_eq!(mode(javascript.join("index.js")), 0o600);
        assert_eq!(mode(javascript.join("nested/helper")), 0o700);

        let output = workspace.join("permissions.s9pk");
        pack(
            CliContext::init(ClientConfig::default()).unwrap(),
            PackParams {
                path: Some(package),
                output: Some(output.clone()),
                javascript: None,
                icon: None,
                license: None,
                instructions: None,
                assets: None,
                no_assets: true,
                arch: Vec::new(),
            },
        )
        .await
        .unwrap();

        let s9pk = S9pk::open(output, None).await.unwrap();
        let javascript_entry = s9pk
            .as_archive()
            .contents()
            .get_path("javascript.squashfs")
            .unwrap();
        let squashfs_path = workspace.join("javascript.squashfs");
        tokio::fs::write(
            &squashfs_path,
            javascript_entry
                .expect_file()
                .unwrap()
                .to_vec(javascript_entry.hash())
                .await
                .unwrap(),
        )
        .await
        .unwrap();
        let extracted = workspace.join("extracted");
        Command::new("sh")
            .arg("-c")
            .arg(r#"umask 000; exec unsquashfs -quiet -dest "$1" "$2""#)
            .arg("sh")
            .arg(&extracted)
            .arg(squashfs_path)
            .invoke(ErrorKind::Filesystem)
            .await
            .unwrap();

        assert_eq!(mode(&extracted), 0o775);
        assert_eq!(mode(extracted.join("nested")), 0o755);
        assert_eq!(mode(extracted.join("index.js")), 0o644);
        assert_eq!(mode(extracted.join("nested/helper")), 0o755);

        assert_eq!(mode(&javascript), 0o770);
        assert_eq!(mode(javascript.join("index.js")), 0o600);
        assert_eq!(mode(javascript.join("nested/helper")), 0o700);
    }
}
