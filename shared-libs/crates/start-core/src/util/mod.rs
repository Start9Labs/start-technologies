use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::str::FromStr;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use ::serde::{Deserialize, Serialize};
use async_trait::async_trait;
use color_eyre::eyre::{self, eyre};
use fd_lock_rs::FdLock;
use futures::FutureExt;
use futures::future::BoxFuture;
use imbl_value::InternedString;
use lazy_static::lazy_static;
use pin_project::pin_project;
use sha2::Digest;
use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncReadExt, BufReader};
use tokio::sync::{Mutex, OwnedMutexGuard, RwLock, oneshot};
use tracing::instrument;
use ts_rs::TS;
use url::Url;

use crate::shutdown::Shutdown;
use crate::util::io::{canonicalize, create_file};
use crate::util::serde::{deserialize_from_str, serialize_display};
use crate::{Error, ErrorKind, ResultExt as _};

pub mod actor;
pub mod clap;
pub mod collections;
pub mod cpupower;
pub mod crypto;
pub mod data_url;
pub mod future;
pub mod http_reader;
pub mod io;
pub mod iter;
pub mod logger;
pub mod lshw;
pub mod mime;
pub mod mok;
pub mod net;
pub mod rpc;
pub mod rpc_client;
pub mod rsync;
pub mod serde;
// pub mod squashfs;
pub mod sync;
pub mod tui;
pub mod version;
pub mod writeback;

pub use clap::FromStrParser;
pub use data_url::DataUrl;
pub use mime::{mime, unmime};
pub use version::VersionString;

#[derive(Clone, Copy, Debug, ::serde::Deserialize, ::serde::Serialize)]
pub enum Never {}
impl Never {}
impl Never {
    pub fn absurd<T>(self) -> T {
        match self {}
    }
}
impl std::fmt::Display for Never {
    fn fmt(&self, _f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.absurd()
    }
}
impl std::error::Error for Never {}
impl<T: ?Sized> AsRef<T> for Never {
    fn as_ref(&self) -> &T {
        match *self {}
    }
}

pub trait Invoke<'a> {
    type Extended<'ext>
    where
        Self: 'ext,
        'ext: 'a;
    fn pipe<'ext: 'a>(
        &'ext mut self,
        next: &'ext mut tokio::process::Command,
    ) -> Self::Extended<'ext>;
    fn timeout<'ext: 'a>(&'ext mut self, timeout: Option<Duration>) -> Self::Extended<'ext>;
    fn input<'ext: 'a, Input: tokio::io::AsyncRead + Unpin + Send>(
        &'ext mut self,
        input: Option<&'ext mut Input>,
    ) -> Self::Extended<'ext>;
    fn capture<'ext: 'a>(&'ext mut self, capture: bool) -> Self::Extended<'ext>;
    fn invoke(
        &mut self,
        error_kind: crate::ErrorKind,
    ) -> impl Future<Output = Result<Vec<u8>, Error>> + Send;
}

pub struct ExtendedCommand<'a> {
    cmd: &'a mut tokio::process::Command,
    timeout: Option<Duration>,
    input: Option<&'a mut (dyn AsyncRead + Unpin + Send)>,
    pipe: VecDeque<&'a mut tokio::process::Command>,
    capture: bool,
}
impl<'a> From<&'a mut tokio::process::Command> for ExtendedCommand<'a> {
    fn from(value: &'a mut tokio::process::Command) -> Self {
        ExtendedCommand {
            cmd: value,
            timeout: None,
            input: None,
            pipe: VecDeque::new(),
            capture: true,
        }
    }
}
impl<'a> std::ops::Deref for ExtendedCommand<'a> {
    type Target = tokio::process::Command;
    fn deref(&self) -> &Self::Target {
        &*self.cmd
    }
}
impl<'a> std::ops::DerefMut for ExtendedCommand<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.cmd
    }
}

impl<'a> Invoke<'a> for tokio::process::Command {
    type Extended<'ext>
        = ExtendedCommand<'ext>
    where
        Self: 'ext,
        'ext: 'a;
    fn pipe<'ext: 'a>(
        &'ext mut self,
        next: &'ext mut tokio::process::Command,
    ) -> Self::Extended<'ext> {
        let mut cmd = ExtendedCommand::from(self);
        cmd.pipe.push_back(next);
        cmd
    }
    fn timeout<'ext: 'a>(&'ext mut self, timeout: Option<Duration>) -> Self::Extended<'ext> {
        let mut cmd = ExtendedCommand::from(self);
        cmd.timeout = timeout;
        cmd
    }
    fn input<'ext: 'a, Input: AsyncRead + Unpin + Send>(
        &'ext mut self,
        input: Option<&'ext mut Input>,
    ) -> Self::Extended<'ext> {
        let mut cmd = ExtendedCommand::from(self);
        cmd.input = if let Some(input) = input {
            Some(&mut *input)
        } else {
            None
        };
        cmd
    }
    fn capture<'ext: 'a>(&'ext mut self, capture: bool) -> Self::Extended<'ext> {
        let mut cmd = ExtendedCommand::from(self);
        cmd.capture = capture;
        cmd
    }
    async fn invoke(&mut self, error_kind: crate::ErrorKind) -> Result<Vec<u8>, Error> {
        ExtendedCommand::from(self).invoke(error_kind).await
    }
}

impl<'a> Invoke<'a> for ExtendedCommand<'a> {
    type Extended<'ext>
        = &'ext mut ExtendedCommand<'ext>
    where
        Self: 'ext,
        'ext: 'a;
    fn pipe<'ext: 'a>(
        &'ext mut self,
        next: &'ext mut tokio::process::Command,
    ) -> Self::Extended<'ext> {
        self.pipe.push_back(next.kill_on_drop(true));
        self
    }
    fn timeout<'ext: 'a>(&'ext mut self, timeout: Option<Duration>) -> Self::Extended<'ext> {
        self.timeout = timeout;
        self
    }
    fn input<'ext: 'a, Input: tokio::io::AsyncRead + Unpin + Send>(
        &'ext mut self,
        input: Option<&'ext mut Input>,
    ) -> Self::Extended<'ext> {
        self.input = if let Some(input) = input {
            Some(&mut *input)
        } else {
            None
        };
        self
    }
    fn capture<'ext: 'a>(&'ext mut self, capture: bool) -> Self::Extended<'ext> {
        self.capture = capture;
        self
    }
    #[instrument(skip_all)]
    async fn invoke(&mut self, error_kind: crate::ErrorKind) -> Result<Vec<u8>, Error> {
        self.cmd.kill_on_drop(true);
        if self.input.is_some() {
            self.cmd.stdin(Stdio::piped());
        }
        if self.pipe.is_empty() {
            let cmd_str = self
                .cmd
                .as_std()
                .get_program()
                .to_string_lossy()
                .into_owned();
            if self.capture {
                self.cmd.stdout(Stdio::piped());
                self.cmd.stderr(Stdio::piped());
            }
            let mut child = self.cmd.spawn().with_ctx(|_| (error_kind, &cmd_str))?;
            if let (Some(mut stdin), Some(input)) = (child.stdin.take(), self.input.take()) {
                use tokio::io::AsyncWriteExt;
                tokio::io::copy(input, &mut stdin).await?;
                stdin.flush().await?;
                stdin.shutdown().await?;
                drop(stdin);
            }
            let res = match self.timeout {
                None => child
                    .wait_with_output()
                    .await
                    .with_ctx(|_| (error_kind, &cmd_str))?,
                Some(t) => tokio::time::timeout(t, child.wait_with_output())
                    .await
                    .with_kind(ErrorKind::Timeout)?
                    .with_ctx(|_| (error_kind, &cmd_str))?,
            };
            crate::ensure_code!(
                res.status.success(),
                error_kind,
                "{}",
                Some(&res.stderr)
                    .filter(|a| !a.is_empty())
                    .or(Some(&res.stdout))
                    .filter(|a| !a.is_empty())
                    .and_then(|a| std::str::from_utf8(a).ok())
                    .unwrap_or(&format!("{} exited with {}", cmd_str, res.status))
            );
            Ok(res.stdout)
        } else {
            let mut futures = Vec::<BoxFuture<'_, Result<(), Error>>>::new(); // todo: predict capacity

            let mut cmds = std::mem::take(&mut self.pipe);
            cmds.push_front(&mut *self.cmd);
            let len = cmds.len();

            let timeout = self.timeout;

            let mut prev = self
                .input
                .take()
                .map(|i| Box::new(i) as Box<dyn AsyncRead + Unpin + Send>);
            for (idx, cmd) in IntoIterator::into_iter(cmds).enumerate() {
                let cmd_str = cmd.as_std().get_program().to_string_lossy().into_owned();
                let last = idx == len - 1;
                if self.capture || !last {
                    cmd.stdout(Stdio::piped());
                }
                if self.capture {
                    cmd.stderr(Stdio::piped());
                }
                if prev.is_some() {
                    cmd.stdin(Stdio::piped());
                }
                let mut child = cmd.spawn().with_ctx(|_| (error_kind, &cmd_str))?;
                let input = std::mem::replace(
                    &mut prev,
                    child
                        .stdout
                        .take()
                        .map(|i| Box::new(BufReader::new(i)) as Box<dyn AsyncRead + Unpin + Send>),
                );
                futures.push(
                    async move {
                        if let (Some(mut stdin), Some(mut input)) = (child.stdin.take(), input) {
                            use tokio::io::AsyncWriteExt;
                            tokio::io::copy(&mut input, &mut stdin).await?;
                            stdin.flush().await?;
                            stdin.shutdown().await?;
                            drop(stdin);
                        }
                        let res = match timeout {
                            None => child.wait_with_output().await?,
                            Some(t) => tokio::time::timeout(t, child.wait_with_output())
                                .await
                                .with_kind(ErrorKind::Timeout)??,
                        };
                        crate::ensure_code!(
                            res.status.success(),
                            error_kind,
                            "{}",
                            Some(&res.stderr)
                                .filter(|a| !a.is_empty())
                                .or(Some(&res.stdout))
                                .filter(|a| !a.is_empty())
                                .and_then(|a| std::str::from_utf8(a).ok())
                                .unwrap_or(&format!(
                                    "{} exited with {}",
                                    cmd.as_std().get_program().to_string_lossy(),
                                    res.status
                                ))
                        );

                        Ok(())
                    }
                    .boxed(),
                );
            }

            let (send, recv) = oneshot::channel();
            futures.push(
                async move {
                    if let Some(mut prev) = prev {
                        let mut res = Vec::new();
                        prev.read_to_end(&mut res).await?;
                        send.send(res).unwrap();
                    } else {
                        send.send(Vec::new()).unwrap();
                    }

                    Ok(())
                }
                .boxed(),
            );

            futures::future::try_join_all(futures).await?;

            Ok(recv.await.unwrap())
        }
    }
}

pub trait Apply: Sized {
    fn apply<O, F: FnOnce(Self) -> O>(self, func: F) -> O {
        func(self)
    }
}

pub trait ApplyRef {
    fn apply_ref<O, F: FnOnce(&Self) -> O>(&self, func: F) -> O {
        func(&self)
    }

    fn apply_mut<O, F: FnOnce(&mut Self) -> O>(&mut self, func: F) -> O {
        func(self)
    }
}

impl<T> Apply for T {}
impl<T> ApplyRef for T {}

pub async fn daemon<F: FnMut() -> Fut, Fut: Future<Output = ()> + Send + 'static>(
    mut f: F,
    cooldown: std::time::Duration,
    mut shutdown: tokio::sync::broadcast::Receiver<Option<Shutdown>>,
) -> Result<(), eyre::Error> {
    loop {
        tokio::select! {
            _ = shutdown.recv() => return Ok(()),
            _ = tokio::time::sleep(cooldown) => (),
        }
        match tokio::spawn(f()).await {
            Err(e) if e.is_panic() => return Err(eyre!("daemon panicked!")),
            _ => (),
        }
    }
}

pub trait SOption<T> {}
pub struct SSome<T>(T);
impl<T> SSome<T> {
    pub fn into(self) -> T {
        self.0
    }
}
impl<T> From<T> for SSome<T> {
    fn from(t: T) -> Self {
        SSome(t)
    }
}
impl<T> SOption<T> for SSome<T> {}
pub struct SNone<T>(PhantomData<T>);
impl<T> SNone<T> {
    pub fn new() -> Self {
        SNone(PhantomData)
    }
}
impl<T> SOption<T> for SNone<T> {}

#[async_trait]
pub trait AsyncFileExt: Sized {
    async fn maybe_open<P: AsRef<Path> + Send + Sync>(path: P) -> Result<Option<Self>, Error>;
    async fn delete<P: AsRef<Path> + Send + Sync>(path: P) -> std::io::Result<()>;
}
#[async_trait]
impl AsyncFileExt for File {
    async fn maybe_open<P: AsRef<Path> + Send + Sync>(path: P) -> Result<Option<Self>, Error> {
        match File::open(path.as_ref()).await {
            Ok(f) => Ok(Some(f)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_ctx(|_| (ErrorKind::Filesystem, path.as_ref().display())),
        }
    }
    async fn delete<P: AsRef<Path> + Send + Sync>(path: P) -> std::io::Result<()> {
        if let Ok(m) = tokio::fs::metadata(path.as_ref()).await {
            if m.is_dir() {
                tokio::fs::remove_dir_all(path).await
            } else {
                tokio::fs::remove_file(path).await
            }
        } else {
            Ok(())
        }
    }
}

pub struct FmtWriter<W: std::fmt::Write>(W);
impl<W: std::fmt::Write> std::io::Write for FmtWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .write_str(
                std::str::from_utf8(buf)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
            )
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub struct Container<T>(RwLock<Option<T>>);
impl<T> Container<T> {
    pub fn new(value: Option<T>) -> Self {
        Container(RwLock::new(value))
    }
    pub async fn set(&self, value: T) -> Option<T> {
        std::mem::replace(&mut *self.0.write().await, Some(value))
    }
    pub async fn take(&self) -> Option<T> {
        self.0.write().await.take()
    }
    pub async fn is_empty(&self) -> bool {
        self.0.read().await.is_none()
    }
    pub async fn drop(&self) {
        *self.0.write().await = None;
    }
}

#[pin_project]
pub struct HashWriter<H: Digest, W: tokio::io::AsyncWrite> {
    hasher: H,
    #[pin]
    writer: W,
}
impl<H: Digest, W: tokio::io::AsyncWrite> HashWriter<H, W> {
    pub fn new(hasher: H, writer: W) -> Self {
        HashWriter { hasher, writer }
    }
    pub fn finish(self) -> (H, W) {
        (self.hasher, self.writer)
    }
    pub fn inner(&self) -> &W {
        &self.writer
    }
    pub fn inner_mut(&mut self) -> &mut W {
        &mut self.writer
    }
}
impl<H: Digest, W: tokio::io::AsyncWrite> tokio::io::AsyncWrite for HashWriter<H, W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.project();
        let written = tokio::io::AsyncWrite::poll_write(this.writer, cx, buf);
        match written {
            // only update the hasher once
            Poll::Ready(res) => {
                if let Ok(n) = res {
                    this.hasher.update(&buf[..n]);
                }
                Poll::Ready(res)
            }
            Poll::Pending => Poll::Pending,
        }
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<std::io::Result<()>> {
        self.project().writer.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context) -> Poll<std::io::Result<()>> {
        self.project().writer.poll_shutdown(cx)
    }
}

pub trait IntoDoubleEndedIterator<T>: IntoIterator<Item = T> {
    type IntoIter: Iterator<Item = T> + DoubleEndedIterator;
    fn into_iter(self) -> <Self as IntoDoubleEndedIterator<T>>::IntoIter;
}
impl<T, U> IntoDoubleEndedIterator<U> for T
where
    T: IntoIterator<Item = U>,
    <T as IntoIterator>::IntoIter: DoubleEndedIterator,
{
    type IntoIter = <T as IntoIterator>::IntoIter;
    fn into_iter(self) -> <Self as IntoDoubleEndedIterator<U>>::IntoIter {
        IntoIterator::into_iter(self)
    }
}

pub struct GeneralBoxedGuard(Option<Box<dyn FnOnce() + Send + Sync>>);
impl GeneralBoxedGuard {
    pub fn new(f: impl FnOnce() + 'static + Send + Sync) -> Self {
        GeneralBoxedGuard(Some(Box::new(f)))
    }

    pub fn drop(mut self) {
        self.0.take().unwrap()()
    }

    pub fn drop_without_action(mut self) {
        self.0 = None;
    }
}

impl Drop for GeneralBoxedGuard {
    fn drop(&mut self) {
        if let Some(destroy) = self.0.take() {
            destroy();
        }
    }
}

pub struct GeneralGuard<F: FnOnce() -> T, T = ()>(Option<F>);
impl<F: FnOnce() -> T, T> GeneralGuard<F, T> {
    pub fn new(f: F) -> Self {
        GeneralGuard(Some(f))
    }

    pub fn drop(mut self) -> T {
        self.0.take().unwrap()()
    }

    pub fn drop_without_action(mut self) {
        self.0 = None;
    }
}

impl<F: FnOnce() -> T, T> Drop for GeneralGuard<F, T> {
    fn drop(&mut self) {
        if let Some(destroy) = self.0.take() {
            destroy();
        }
    }
}

pub struct FileLock(#[allow(unused)] OwnedMutexGuard<()>, Option<FdLock<File>>);
impl Drop for FileLock {
    fn drop(&mut self) {
        if let Some(fd_lock) = self.1.take() {
            tokio::task::spawn_blocking(|| fd_lock.unlock(true).map_err(|(_, e)| e).log_err());
        }
    }
}
impl FileLock {
    #[instrument(skip_all)]
    pub async fn new(path: impl AsRef<Path> + Send + Sync, blocking: bool) -> Result<Self, Error> {
        lazy_static! {
            static ref INTERNAL_LOCKS: Mutex<BTreeMap<PathBuf, Arc<Mutex<()>>>> =
                Mutex::new(BTreeMap::new());
        }
        let path = canonicalize(path.as_ref(), true)
            .await
            .with_kind(ErrorKind::Filesystem)?;
        let mut internal_locks = INTERNAL_LOCKS.lock().await;
        if !internal_locks.contains_key(&path) {
            internal_locks.insert(path.clone(), Arc::new(Mutex::new(())));
        }
        let tex = internal_locks.get(&path).unwrap().clone();
        drop(internal_locks);
        let tex_guard = if blocking {
            tex.lock_owned().await
        } else {
            tex.try_lock_owned()
                .with_kind(crate::ErrorKind::Filesystem)?
        };
        let parent = path.parent().unwrap_or(Path::new("/"));
        if tokio::fs::metadata(parent).await.is_err() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_ctx(|_| (crate::ErrorKind::Filesystem, parent.display().to_string()))?;
        }
        let f = create_file(&path).await?;
        let file_guard = tokio::task::spawn_blocking(move || {
            fd_lock_rs::FdLock::lock(f, fd_lock_rs::LockType::Exclusive, blocking)
        })
        .await
        .with_kind(crate::ErrorKind::Unknown)?
        .with_kind(crate::ErrorKind::Filesystem)?;
        Ok(FileLock(tex_guard, Some(file_guard)))
    }
    pub async fn unlock(mut self) -> Result<(), Error> {
        if let Some(fd_lock) = self.1.take() {
            tokio::task::spawn_blocking(|| fd_lock.unlock(true).map_err(|(_, e)| e))
                .await
                .with_kind(crate::ErrorKind::Unknown)?
                .with_kind(crate::ErrorKind::Filesystem)?;
        }
        Ok(())
    }
}

pub fn assure_send<T: Send>(x: T) -> T {
    x
}

pub enum MaybeOwned<'a, T> {
    Borrowed(&'a T),
    Owned(T),
}
impl<'a, T> std::ops::Deref for MaybeOwned<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Borrowed(a) => *a,
            Self::Owned(a) => a,
        }
    }
}
impl<'a, T> From<T> for MaybeOwned<'a, T> {
    fn from(value: T) -> Self {
        MaybeOwned::Owned(value)
    }
}
impl<'a, T> From<&'a T> for MaybeOwned<'a, T> {
    fn from(value: &'a T) -> Self {
        MaybeOwned::Borrowed(value)
    }
}

pub fn new_guid() -> InternedString {
    use rand::Rng;
    let mut buf = [0; 20];
    crate::util::crypto::os_rng().fill_bytes(&mut buf);
    InternedString::intern(base32::encode(
        base32::Alphabet::Rfc4648 { padding: false },
        &buf,
    ))
}

/// A utility for lazily computing a Display value. This is useful for i18n
/// where you want to defer the translation until the value is actually displayed,
/// avoiding allocations in the common case where the message is not rendered.
pub struct LazyDisplay<F>(F);
impl<F: Fn() -> D, D: fmt::Display> LazyDisplay<F> {
    pub fn new(f: F) -> Self {
        LazyDisplay(f)
    }
}
impl<F: Fn() -> D, D: fmt::Display> fmt::Display for LazyDisplay<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", (self.0)())
    }
}

/// Creates a lazily-evaluated Display implementation.
/// The closure is only called when the value is actually displayed.
pub fn lazy_display<F: Fn() -> D, D: fmt::Display>(f: F) -> LazyDisplay<F> {
    LazyDisplay::new(f)
}

#[derive(Debug, Clone, TS)]
#[ts(type = "string")]
pub enum PathOrUrl {
    Path(PathBuf),
    Url(Url),
}
impl FromStr for PathOrUrl {
    type Err = <PathBuf as FromStr>::Err;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(url) = s.parse::<Url>() {
            if let Some(path) = s.strip_prefix("file://") {
                Ok(Self::Path(path.parse()?))
            } else {
                Ok(Self::Url(url))
            }
        } else {
            Ok(Self::Path(s.parse()?))
        }
    }
}
impl fmt::Display for PathOrUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path(p) => write!(f, "file://{}", p.display()),
            Self::Url(u) => write!(f, "{u}"),
        }
    }
}
impl<'de> Deserialize<'de> for PathOrUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        deserialize_from_str(deserializer)
    }
}
impl Serialize for PathOrUrl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::Serializer,
    {
        serialize_display(self, serializer)
    }
}
