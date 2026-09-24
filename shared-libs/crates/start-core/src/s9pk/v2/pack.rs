use std::collections::{BTreeMap, BTreeSet};
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
    sqfs: OnceCell<MultiCursorFile>,
}
impl SqfsDir {
    pub fn new(path: PathBuf, tmpdir: Arc<TmpDir>) -> Self {
        Self {
            path,
            tmpdir,
            sqfs: OnceCell::new(),
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
                    Command::new("mksquashfs")
                        .arg(&self.path)
                        .arg(&path)
                        .arg("-quiet")
                        .invoke(ErrorKind::Filesystem)
                        .await?;
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
    /// Unlike `instructions`, a README is optional: it is a repository document, and an
    /// existing package that lacks one must keep building. When present it is packed so an
    /// agent administering the service can read it from the installed s9pk — version-matched,
    /// and without reaching the network.
    async fn readme(&self) -> Result<Option<PathBuf>, Error> {
        let candidate = self.path().join("README.md");
        Ok(tokio::fs::metadata(&candidate)
            .await
            .is_ok()
            .then_some(candidate))
    }
    fn assets(&self) -> PathBuf {
        self.assets
            .as_ref()
            .cloned()
            .unwrap_or_else(|| self.path().join("assets"))
    }
}

#[derive(Debug, Default, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImageConfig {
    pub source: ImageSource,
    #[ts(type = "string[]")]
    pub arch: BTreeSet<InternedString>,
    pub emulate_missing: bool,
    #[serde(rename = "emulateMissingAs", skip_serializing_if = "Option::is_none")]
    #[ts(skip)]
    legacy_emulate_missing_as: Option<InternedString>,
    #[serde(default)]
    pub nvidia_container: bool,
}
impl ImageConfig {
    fn legacy_emulation_arch(&self) -> Option<&InternedString> {
        self.legacy_emulate_missing_as
            .as_ref()
            .filter(|arch| self.arch.contains(*arch))
    }

    fn emulation_arch(&self, is_available: impl Fn(&str) -> bool) -> Option<&InternedString> {
        self.legacy_emulation_arch()
            .filter(|arch| is_available(arch))
            .or_else(|| self.arch.iter().find(|arch| is_available(arch)))
    }

    pub(crate) fn resolve_arch<'a, T>(
        &'a self,
        requested: &'a str,
        image_id: &ImageId,
        archive: &DirectoryContents<T>,
    ) -> Option<&'a str> {
        let is_available = |arch: &str| {
            archive
                .get_path(
                    Path::new("images")
                        .join(arch)
                        .join(image_id)
                        .with_extension("squashfs"),
                )
                .and_then(|entry| entry.as_file())
                .is_some()
        };
        if is_available(requested) {
            Some(requested)
        } else if self.emulate_missing {
            self.emulation_arch(is_available).map(|arch| &**arch)
        } else {
            None
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImageConfigInput {
    source: ImageSource,
    arch: BTreeSet<InternedString>,
    #[serde(default)]
    emulate_missing: Option<bool>,
    #[serde(default)]
    emulate_missing_as: Option<InternedString>,
    #[serde(default)]
    nvidia_container: bool,
}

impl<'de> Deserialize<'de> for ImageConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let input = ImageConfigInput::deserialize(deserializer)?;
        let legacy_emulate_missing_as = input
            .emulate_missing_as
            .filter(|arch| !arch.is_empty() && input.emulate_missing != Some(false));
        Ok(Self {
            source: input.source,
            arch: input.arch,
            emulate_missing: input
                .emulate_missing
                .unwrap_or(legacy_emulate_missing_as.is_some()),
            legacy_emulate_missing_as,
            nvidia_container: input.nvidia_container,
        })
    }
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
    #[arg(long, help = "help.arg.no-emulation")]
    no_emulation: bool,
    #[arg(long, help = "help.arg.nvidia-container")]
    nvidia_container: bool,
}
impl TryFrom<CliImageConfig> for ImageConfig {
    type Error = clap::Error;
    fn try_from(value: CliImageConfig) -> Result<Self, Self::Error> {
        Ok(Self {
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
            emulate_missing: !value.no_emulation,
            legacy_emulate_missing_as: None,
            nvidia_container: value.nvidia_container,
        })
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
                            .arg("/startos-pack-placeholder")
                            .invoke(ErrorKind::Docker)
                            .await?,
                    )?;
                    let container = container.trim();
                    let packed_image: Result<(), Error> = async {
                        let image = String::from_utf8(
                            Command::new(*CONTAINER_TOOL)
                                .arg("container")
                                .arg("inspect")
                                .arg("--format")
                                .arg("{{.Image}}")
                                .arg(container)
                                .invoke(ErrorKind::Docker)
                                .await?,
                        )?;
                        let config = serde_json::from_slice::<DockerImageConfig>(
                            &Command::new(*CONTAINER_TOOL)
                                .arg("image")
                                .arg("inspect")
                                .arg("--format")
                                .arg("{{json .Config}}")
                                .arg(image.trim())
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
                        into.insert_path(
                            base_path.with_extension("squashfs"),
                            Entry::file(
                                TmpSource::new(tmp_dir.clone(), PackSource::File(dest)).into(),
                            ),
                        )?;

                        Ok(())
                    }
                    .await;
                    let removal = Command::new(*CONTAINER_TOOL)
                        .arg("rm")
                        .arg(container)
                        .invoke(ErrorKind::Docker)
                        .await;
                    packed_image?;
                    removal?;
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
        .invoke(ErrorKind::ServiceRuntime)
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
    if let Some(readme) = params.readme().await? {
        files.insert(
            "README.md".into(),
            Entry::file(TmpSource::new(tmp_dir.clone(), PackSource::File(readme))),
        );
    }
    files.insert(
        "javascript.squashfs".into(),
        Entry::file(TmpSource::new(
            tmp_dir.clone(),
            PackSource::Squashfs(Arc::new(SqfsDir::new(js_dir, tmp_dir.clone()))),
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
            let has_missing_native_arch = arches.iter().any(|arch| !c.arch.contains(arch));
            let mut filtered = c
                .arch
                .intersection(&arches)
                .cloned()
                .collect::<BTreeSet<_>>();
            if filtered.is_empty() {
                if c.emulate_missing {
                    if let Some(arch) = c.emulation_arch(|_| true).cloned() {
                        tracing::warn!(
                            "{}",
                            t!(
                                "s9pk.pack.image-unavailable-emulating",
                                image = id.to_string(),
                                requested = arches.iter().join("/"),
                                emulated = arch.to_string(),
                            )
                        );
                        c.arch = [arch].into_iter().collect();
                    } else {
                        tracing::error!(
                            "{}",
                            t!(
                                "s9pk.pack.image-no-architecture-to-emulate",
                                image = id.to_string(),
                            )
                        );
                    }
                } else {
                    tracing::error!(
                        "{}",
                        t!(
                            "s9pk.pack.image-unavailable",
                            image = id.to_string(),
                            requested = arches.iter().join("/"),
                        )
                    );
                }
            } else {
                if has_missing_native_arch {
                    filtered.extend(c.legacy_emulation_arch().cloned());
                }
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
                .invoke(ErrorKind::ServiceRuntime)
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
            ]
            .into_iter()
            .chain(params.readme().await?)
            .collect());
        }
    };
    let mut ingredients = vec![
        js_path,
        params.icon().await?,
        params.license().await?,
        params.instructions().await?,
    ];
    ingredients.extend(params.readme().await?);

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

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    #[ignore = "requires Docker or Podman and tar2sqfs"]
    async fn packs_images_with_and_without_default_commands() {
        use tokio::io::AsyncReadExt;

        if std::env::var_os("STARTOS_PACK_TEST_DOCKER").is_some() {
            let _ = PREFER_DOCKER.set(true);
        }
        let tmp_dir = Arc::new(TmpDir::new().await.unwrap());
        let tag = format!("start9/pack-command-test:{}", new_guid());
        let id = "pack-command-test".parse().unwrap();
        let version = "1.0.0:0".parse().unwrap();
        let image_id = "image".parse().unwrap();
        let result: Result<(), Error> = async {
            tokio::fs::write(tmp_dir.join("marker"), "hello").await?;
            for (dockerfile, entrypoint, cmd) in [
                ("FROM scratch\nCOPY marker /marker\n", None, None),
                (
                    "FROM scratch\nCOPY marker /marker\nENTRYPOINT [\"/app\"]\nCMD [\"--serve\"]\n",
                    Some(vec!["/app".to_owned()]),
                    Some(vec!["--serve".to_owned()]),
                ),
            ] {
                tokio::fs::write(tmp_dir.join("Dockerfile"), dockerfile).await?;
                Command::new(*CONTAINER_TOOL)
                    .arg("build")
                    .arg("-t")
                    .arg(&tag)
                    .arg(&**tmp_dir)
                    .invoke(ErrorKind::Docker)
                    .await?;
                let mut contents = DirectoryContents::<TmpSource<PackSource>>::new();
                ImageSource::DockerTag(tag.clone())
                    .load(
                        tmp_dir.clone(),
                        &id,
                        &version,
                        &image_id,
                        "x86_64",
                        &mut contents,
                    )
                    .await?;
                let mut json = Vec::new();
                contents
                    .get_path("images/x86_64/image.json")
                    .unwrap()
                    .as_file()
                    .unwrap()
                    .reader()
                    .await?
                    .read_to_end(&mut json)
                    .await?;
                let metadata: ImageMetadata = serde_json::from_slice(&json).unwrap();
                assert_eq!(metadata.entrypoint, entrypoint);
                assert_eq!(metadata.cmd, cmd);
                assert!(contents.get_path("images/x86_64/image.squashfs").is_some());
            }
            Ok(())
        }
        .await;
        let cleanup = Command::new(*CONTAINER_TOOL)
            .arg("rmi")
            .arg("-f")
            .arg(&tag)
            .invoke(ErrorKind::Docker)
            .await;
        result.unwrap();
        cleanup.unwrap();
    }

    #[test]
    fn cli_enables_emulation_by_default() {
        let config = CliImageConfig::try_parse_from(["image", "--arch", "x86_64"]).unwrap();
        let config = ImageConfig::try_from(config).unwrap();

        assert!(config.emulate_missing);
        let serialized = serde_json::to_value(config).unwrap();
        assert!(serialized.get("emulateMissingAs").is_none());
    }

    #[test]
    fn no_emulation_cli_flag_disables_emulation() {
        let config =
            CliImageConfig::try_parse_from(["image", "--arch", "x86_64", "--no-emulation"])
                .unwrap();
        let config = ImageConfig::try_from(config).unwrap();

        assert!(!config.emulate_missing);
    }

    #[test]
    fn old_cli_flags_are_rejected() {
        for args in [
            vec!["image", "--arch", "x86_64", "--emulate-missing"],
            vec![
                "image",
                "--arch",
                "x86_64",
                "--emulate-missing-as",
                "x86_64",
            ],
        ] {
            assert!(CliImageConfig::try_parse_from(args).is_err());
        }
    }

    #[test]
    fn legacy_manifest_fallback_remains_the_preferred_emulation_architecture() {
        let config: ImageConfig = serde_json::from_value(serde_json::json!({
            "source": "packed",
            "arch": ["aarch64", "x86_64"],
            "emulateMissingAs": "x86_64",
            "nvidiaContainer": false
        }))
        .unwrap();

        assert_eq!(
            config.emulation_arch(|_| true).map(|arch| &**arch),
            Some("x86_64"),
        );
    }
}
