use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::fs::File;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::action::{ActionResult, ActionResultValue};
use crate::prelude::*;
use crate::rpc_continuations::{Guid, RpcContinuation, RpcContinuations};

pub const PREPARED_TRANSFER_TTL: Duration = Duration::from_secs(30 * 60);
pub const ACTIVE_TRANSFER_TIMEOUT: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct UploadedFile {
    /// Untrusted original filename supplied by the uploader.
    pub name: String,
    /// The actual volume-relative destination of the installed file.
    pub path: String,
    /// Untrusted Content-Type supplied by the uploader.
    pub mime: Option<String>,
    pub commitment: FileCommitment,
}
rpc_toolkit::reflect_ts!(UploadedFile);
rpc_toolkit::ts_export!(UploadedFile, namespaces = [""]);

#[derive(Debug, Clone, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct FileCommitment {
    /// BLAKE3 digest encoded with padded standard Base64.
    pub hash: String,
    pub size: u64,
}
rpc_toolkit::reflect_ts!(FileCommitment);

#[derive(Debug, Clone, Default, Serialize, Deserialize, VisitFields)]
#[serde(rename_all = "camelCase")]
pub struct UploadLimits {
    /// Maximum uploaded bytes, inclusive; null leaves the size unbounded.
    pub max_size: Option<u64>,
}
rpc_toolkit::reflect_ts!(UploadLimits);
rpc_toolkit::ts_export!(UploadLimits, namespaces = [""]);

impl UploadLimits {
    fn check_size(&self, size: u64) -> Result<(), Error> {
        if let Some(max) = self.max_size {
            if size > max {
                return Err(Error::new(
                    eyre!("{}", t!("service.action-files.size-limit", max = max)),
                    ErrorKind::InvalidRequest,
                ));
            }
        }
        Ok(())
    }
}

pub(super) fn invalid() -> Error {
    Error::new(
        eyre!("{}", t!("service.action-files.invalid-transfer")),
        ErrorKind::InvalidRequest,
    )
}

fn syscall(result: libc::c_int) -> std::io::Result<libc::c_int> {
    if result < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(result)
    }
}

fn open_at(parent: &File, name: &CStr, flags: libc::c_int, mode: u32) -> std::io::Result<File> {
    let fd = syscall(unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            mode as libc::c_uint,
        )
    })?;
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn open_root(root: &Path) -> Result<File, Error> {
    let mut parent = File::open("/")?;
    for component in root.components() {
        match component {
            Component::RootDir => (),
            Component::Normal(name) => {
                use std::os::unix::ffi::OsStrExt;
                let name = CString::new(name.as_bytes()).map_err(|_| invalid())?;
                parent = open_at(&parent, &name, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
            }
            _ => return Err(invalid()),
        }
    }
    Ok(parent)
}

struct Destination {
    parent: File,
    name: CString,
}
impl Destination {
    fn new(root: &Path, path: &str) -> Result<Self, Error> {
        let parts = path.split('/').collect::<Vec<_>>();
        if parts
            .iter()
            .any(|p| p.is_empty() || *p == "." || *p == "..")
        {
            return Err(invalid());
        }
        let mut parent = open_root(root)?;
        for component in &parts[..parts.len() - 1] {
            let name = CString::new(*component).map_err(|_| invalid())?;
            parent = open_at(&parent, &name, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        }
        let name = CString::new(*parts.last().ok_or_else(invalid)?).map_err(|_| invalid())?;
        Ok(Self { parent, name })
    }

    fn check_file(&self) -> Result<(), Error> {
        match self.stat() {
            Ok(stat) if stat.st_mode & libc::S_IFMT == libc::S_IFREG => Ok(()),
            Ok(_) => Err(invalid()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn stat(&self) -> std::io::Result<libc::stat> {
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        syscall(unsafe {
            libc::fstatat(
                self.parent.as_raw_fd(),
                self.name.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        })?;
        Ok(unsafe { stat.assume_init() })
    }

    fn open(&self) -> Result<File, Error> {
        if self.stat()?.st_mode & libc::S_IFMT != libc::S_IFREG {
            return Err(invalid());
        }
        let file = open_at(
            &self.parent,
            &self.name,
            libc::O_RDONLY | libc::O_NONBLOCK,
            0,
        )?;
        if !file.metadata()?.is_file() {
            return Err(invalid());
        }
        Ok(file)
    }

    fn unlink(&self, name: &CStr) -> std::io::Result<()> {
        syscall(unsafe { libc::unlinkat(self.parent.as_raw_fd(), name.as_ptr(), 0) }).map(|_| ())
    }
}

struct Stage {
    target: Arc<Destination>,
    name: CString,
    installed: bool,
}
impl Stage {
    fn new(target: Arc<Destination>) -> Result<(Self, tokio::fs::File), Error> {
        let name = CString::new(format!(".startos-upload-{}", Guid::new())).unwrap();
        let file = open_at(
            &target.parent,
            &name,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )?;
        Ok((
            Self {
                target,
                name,
                installed: false,
            },
            tokio::fs::File::from_std(file),
        ))
    }
    fn install(&mut self) -> Result<(), Error> {
        syscall(unsafe {
            libc::renameat(
                self.target.parent.as_raw_fd(),
                self.name.as_ptr(),
                self.target.parent.as_raw_fd(),
                self.target.name.as_ptr(),
            )
        })?;
        self.installed = true;
        Ok(())
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        if !self.installed {
            if let Err(e) = self.target.unlink(&self.name) {
                tracing::warn!("Removing action upload stage: {e}");
            }
        }
    }
}

struct DeleteFile {
    target: Arc<Destination>,
    file: File,
    armed: AtomicBool,
}
impl DeleteFile {
    fn cleanup(&self) {
        if !self.armed.swap(false, Ordering::AcqRel) {
            return;
        }
        let result = (|| -> Result<(), Error> {
            let original = self.file.metadata()?;
            let metadata = self.target.stat()?;
            if metadata.st_dev as u64 == original.dev() && metadata.st_ino as u64 == original.ino()
            {
                // The package must keep this path stable until cleanup completes.
                self.target.unlink(&self.target.name)?;
            }
            Ok(())
        })();
        if let Err(e) = result {
            tracing::warn!("Removing action download: {e}");
        }
    }
}
impl Drop for DeleteFile {
    fn drop(&mut self) {
        self.cleanup();
    }
}

struct Download {
    file: File,
    cleanup: Arc<DeleteFile>,
    mime: String,
    disposition: String,
}

enum UploadTarget {
    File(Arc<Destination>),
    Directory(File),
}

impl UploadTarget {
    fn new(root: &Path, path: &str) -> Result<(Self, String), Error> {
        if path.is_empty() || path == "." {
            return Ok((Self::Directory(open_root(root)?), String::new()));
        }
        let normalized = path.strip_suffix('/').unwrap_or(path);
        let target = Destination::new(root, normalized)?;
        match target.stat() {
            Ok(stat) if stat.st_mode & libc::S_IFMT == libc::S_IFDIR => {
                let directory = open_at(
                    &target.parent,
                    &target.name,
                    libc::O_RDONLY | libc::O_DIRECTORY,
                    0,
                )?;
                Ok((Self::Directory(directory), normalized.to_owned()))
            }
            _ if normalized != path => Err(invalid()),
            _ => {
                target.check_file()?;
                Ok((Self::File(Arc::new(target)), path.to_owned()))
            }
        }
    }
}

struct Upload {
    target: UploadTarget,
    limits: UploadLimits,
    path: String,
    mode: u32,
    uid: u32,
    gid: u32,
}

enum Transfer {
    Upload(Upload),
    Uploaded(UploadedFile),
    Download { file: Download, published: bool },
}
struct Entry {
    event: Guid,
    transfer: Transfer,
    deadline: Instant,
    _expiry: tokio_util::sync::DropGuard,
}
impl Entry {
    fn is_expired(&self) -> bool {
        self.deadline <= Instant::now()
    }

    fn check_event(&self, event: &Guid) -> Result<(), Error> {
        if &self.event != event || self.is_expired() {
            return Err(invalid());
        }
        Ok(())
    }
}
pub(super) struct ActionFiles {
    entries: Mutex<BTreeMap<Guid, Entry>>,
    shutdown: CancellationToken,
    ttl: Duration,
    active_timeout: Duration,
}
impl Default for ActionFiles {
    fn default() -> Self {
        Self {
            entries: Mutex::new(BTreeMap::new()),
            shutdown: CancellationToken::new(),
            ttl: PREPARED_TRANSFER_TTL,
            active_timeout: ACTIVE_TRANSFER_TIMEOUT,
        }
    }
}
impl Drop for ActionFiles {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}
impl ActionFiles {
    fn insert(self: &Arc<Self>, id: Guid, event: Guid, transfer: Transfer) {
        let ttl = match &transfer {
            // Receipts must outlive pending and active sibling uploads.
            Transfer::Uploaded(_) => self.ttl + self.active_timeout,
            _ => self.ttl,
        };
        let deadline = Instant::now() + ttl;
        let expiry = CancellationToken::new();
        self.entries.lock().unwrap().insert(
            id.clone(),
            Entry {
                event,
                transfer,
                deadline,
                _expiry: expiry.clone().drop_guard(),
            },
        );
        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            tokio::select! {
                _ = expiry.cancelled() => (),
                _ = tokio::time::sleep_until(deadline) => {
                    if let Some(store) = weak.upgrade() { store.expire_entry(&id); }
                }
            }
        });
    }

    fn expire_entry(&self, id: &Guid) {
        let mut entries = self.entries.lock().unwrap();
        if entries.get(id).is_some_and(Entry::is_expired) {
            entries.remove(id);
        }
    }

    pub async fn prepare_upload(
        self: &Arc<Self>,
        continuations: &RpcContinuations,
        event: Guid,
        root: &Path,
        path: String,
        limits: UploadLimits,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
    ) -> Result<Guid, Error> {
        let (target, path) = UploadTarget::new(root, &path)?;
        let mode = mode.unwrap_or(0o600);
        if mode & !0o7777 != 0 || uid == Some(u32::MAX) || gid == Some(u32::MAX) {
            return Err(invalid());
        }
        let id = Guid::new();
        self.insert(
            id.clone(),
            event,
            Transfer::Upload(Upload {
                target,
                limits,
                path,
                mode,
                uid: uid.unwrap_or(0),
                gid: gid.unwrap_or(0),
            }),
        );
        let weak = Arc::downgrade(self);
        let request_id = id.clone();
        continuations
            .add(
                id.clone(),
                RpcContinuation::rest(
                    move |request| upload_request(weak, request_id, request),
                    self.ttl,
                ),
            )
            .await;
        Ok(id)
    }

    pub fn prepare_download(
        self: &Arc<Self>,
        event: Guid,
        root: &Path,
        path: &str,
        mime: Option<String>,
        filename: Option<String>,
    ) -> Result<Guid, Error> {
        let target = Arc::new(Destination::new(root, path)?);
        let file = target.open()?;
        let mime = mime.unwrap_or_else(|| "application/octet-stream".to_owned());
        header::HeaderValue::from_str(&mime).map_err(|_| invalid())?;
        let filename = filename.unwrap_or_else(|| target.name.to_string_lossy().into_owned());
        let disposition = format!(
            "attachment; filename=\"download\"; filename*=UTF-8''{}",
            encode_filename(&filename)
        );
        let cleanup = Arc::new(DeleteFile {
            target,
            file: file.try_clone()?,
            armed: AtomicBool::new(false),
        });
        let id = Guid::new();
        self.insert(
            id.clone(),
            event,
            Transfer::Download {
                file: Download {
                    file,
                    cleanup,
                    mime,
                    disposition,
                },
                published: false,
            },
        );
        Ok(id)
    }

    pub fn get_upload_limits(&self, event: &Guid, id: &Guid) -> Result<UploadLimits, Error> {
        let entries = self.entries.lock().unwrap();
        let entry = entries.get(id).ok_or_else(invalid)?;
        entry.check_event(event)?;
        match &entry.transfer {
            Transfer::Upload(upload) => Ok(upload.limits.clone()),
            _ => Err(invalid()),
        }
    }

    pub fn resolve_upload(&self, event: &Guid, id: &Guid) -> Result<UploadedFile, Error> {
        let entries = self.entries.lock().unwrap();
        let entry = entries.get(id).ok_or_else(invalid)?;
        entry.check_event(event)?;
        match &entry.transfer {
            Transfer::Uploaded(file) => Ok(file.clone()),
            _ => Err(invalid()),
        }
    }

    pub async fn publish(
        self: &Arc<Self>,
        continuations: &RpcContinuations,
        event: &Guid,
        result: &ActionResult,
    ) -> Result<(), Error> {
        let mut files = BTreeMap::new();
        if let ActionResult::V1(result) = result {
            if let Some(value) = &result.result {
                collect_files(value, &mut files)?;
            }
        }
        let mut pending = Vec::new();
        {
            let mut entries = self.entries.lock().unwrap();
            for id in files.keys() {
                let entry = entries.get(id).ok_or_else(invalid)?;
                entry.check_event(event)?;
                if !matches!(
                    entry.transfer,
                    Transfer::Download {
                        published: false,
                        ..
                    }
                ) {
                    return Err(invalid());
                }
            }
            for (id, delete) in files {
                let entry = entries.get_mut(&id).unwrap();
                if let Transfer::Download { file, published } = &mut entry.transfer {
                    *published = true;
                    file.cleanup.armed.store(delete, Ordering::Release);
                    pending.push((id, entry.deadline.saturating_duration_since(Instant::now())));
                }
            }
        }
        for (id, ttl) in pending {
            let weak = Arc::downgrade(self);
            let request_id = id.clone();
            continuations
                .add(
                    id,
                    RpcContinuation::rest(
                        move |request| download_request(weak, request_id, request),
                        ttl,
                    ),
                )
                .await;
        }
        Ok(())
    }
}

fn collect_files(value: &ActionResultValue, files: &mut BTreeMap<Guid, bool>) -> Result<(), Error> {
    match value {
        ActionResultValue::File { value, delete } => {
            if files.insert(value.clone(), *delete).is_some() {
                return Err(invalid());
            }
        }
        ActionResultValue::Group { value } => {
            for member in value {
                collect_files(&member.value, files)?;
            }
        }
        _ => (),
    }
    Ok(())
}

fn encode_filename(name: &str) -> String {
    name.bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn upload_name(request: &Request) -> Result<String, Error> {
    let encoded = request
        .headers()
        .get("x-file-name")
        .ok_or_else(invalid)?
        .to_str()
        .map_err(|_| invalid())?;
    let decoded = crate::util::percent_encoding::decode(encoded.as_bytes()).ok_or_else(invalid)?;
    let name = String::from_utf8(decoded).map_err(|_| invalid())?;
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', '\0', '\r', '\n'])
    {
        return Err(invalid());
    }
    Ok(name)
}

async fn receive_upload(upload: Upload, request: Request) -> Result<UploadedFile, Error> {
    let name = upload_name(&request)?;
    let mime = request
        .headers()
        .get(header::CONTENT_TYPE)
        .map(|h| h.to_str().map(String::from))
        .transpose()
        .map_err(|_| invalid())?;
    let expected_size = request
        .headers()
        .get(header::CONTENT_LENGTH)
        .map(|h| {
            h.to_str()
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .ok_or_else(invalid)
        })
        .transpose()?;
    if let Some(expected) = expected_size {
        upload.limits.check_size(expected)?;
    }
    let (target, path) = match upload.target {
        UploadTarget::File(target) => (target, upload.path),
        UploadTarget::Directory(parent) => {
            let target = Arc::new(Destination {
                parent,
                name: CString::new(name.as_str()).map_err(|_| invalid())?,
            });
            target.check_file()?;
            let path = if upload.path.is_empty() {
                name.clone()
            } else {
                format!("{}/{name}", upload.path)
            };
            (target, path)
        }
    };
    let (mut stage, mut file) = Stage::new(target)?;
    let mut body = request.into_body().into_data_stream();
    let mut hash = blake3::Hasher::new();
    let mut size = 0u64;
    while let Some(chunk) = body.next().await {
        let chunk = chunk.with_kind(ErrorKind::Network)?;
        size = size.checked_add(chunk.len() as u64).ok_or_else(invalid)?;
        upload.limits.check_size(size)?;
        if expected_size.is_some_and(|expected| size > expected) {
            return Err(invalid());
        }
        file.write_all(&chunk).await?;
        hash.update(&chunk);
    }
    if expected_size.is_some_and(|expected| size != expected) {
        return Err(invalid());
    }
    file.flush().await?;
    syscall(unsafe { libc::fchown(file.as_raw_fd(), upload.uid, upload.gid) })?;
    syscall(unsafe { libc::fchmod(file.as_raw_fd(), upload.mode as libc::mode_t) })?;
    file.sync_all().await?;
    stage.install()?;
    Ok(UploadedFile {
        name,
        path,
        mime,
        commitment: FileCommitment {
            hash: base64::engine::general_purpose::STANDARD.encode(hash.finalize().as_bytes()),
            size,
        },
    })
}

async fn upload_request(
    weak: Weak<ActionFiles>,
    id: Guid,
    request: Request,
) -> Result<Response, Error> {
    if request.method() != Method::POST {
        return Ok(StatusCode::METHOD_NOT_ALLOWED.into_response());
    }
    let (upload, event, shutdown, timeout) = {
        let store = weak.upgrade().ok_or_else(invalid)?;
        let entry = store
            .entries
            .lock()
            .unwrap()
            .remove(&id)
            .ok_or_else(invalid)?;
        if entry.is_expired() {
            return Err(invalid());
        }
        let Transfer::Upload(upload) = entry.transfer else {
            return Err(invalid());
        };
        (
            upload,
            entry.event.clone(),
            store.shutdown.clone(),
            store.active_timeout,
        )
    };
    let uploaded = tokio::select! {
        biased;
        _ = shutdown.cancelled() => return Err(invalid()),
        result = tokio::time::timeout(timeout, receive_upload(upload, request)) => result.map_err(|_| invalid())??,
    };
    if let Some(store) = weak.upgrade() {
        store.insert(id, event, Transfer::Uploaded(uploaded));
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

struct DownloadGuard(Arc<DeleteFile>);
impl Drop for DownloadGuard {
    fn drop(&mut self) {
        self.0.cleanup();
    }
}

async fn download_request(
    weak: Weak<ActionFiles>,
    id: Guid,
    request: Request,
) -> Result<Response, Error> {
    if request.method() != Method::GET {
        return Ok(StatusCode::METHOD_NOT_ALLOWED.into_response());
    }
    let (download, shutdown, timeout) = {
        let store = weak.upgrade().ok_or_else(invalid)?;
        let entry = store
            .entries
            .lock()
            .unwrap()
            .remove(&id)
            .ok_or_else(invalid)?;
        if entry.is_expired() {
            return Err(invalid());
        }
        let Transfer::Download {
            file,
            published: true,
        } = entry.transfer
        else {
            return Err(invalid());
        };
        (file, store.shutdown.clone(), store.active_timeout)
    };
    let Download {
        file,
        cleanup,
        mime,
        disposition,
    } = download;
    let deadline = Instant::now() + timeout;
    let weak_cleanup = Arc::downgrade(&cleanup);
    let finished = CancellationToken::new();
    let timer_finished = finished.clone();
    let stop = shutdown.child_token();
    let timer_stop = stop.clone();
    tokio::spawn(async move {
        tokio::select! {
            _ = timer_finished.cancelled() => return,
            _ = tokio::time::sleep_until(deadline) => (),
            _ = shutdown.cancelled() => (),
        }
        timer_stop.cancel();
        if let Some(cleanup) = weak_cleanup.upgrade() {
            cleanup.cleanup();
        }
    });
    let guard = DownloadGuard(cleanup);
    let finished = finished.drop_guard();
    let stream = async_stream::stream! {
        let guard = guard;
        let _finished = finished;
        let mut file = tokio::fs::File::from_std(file);
        let mut buffer = vec![0; 64 * 1024];
        loop {
            let read = tokio::select! {
                biased;
                _ = stop.cancelled() => Err(std::io::Error::new(std::io::ErrorKind::TimedOut, invalid().to_string())),
                result = file.read(&mut buffer) => result,
            };
            match read {
                Ok(0) => break,
                Ok(n) => yield Ok(bytes::Bytes::copy_from_slice(&buffer[..n])),
                Err(e) => {
                    guard.0.cleanup();
                    yield Err(e);
                    break;
                }
            }
        }
    };
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::CONTENT_DISPOSITION, disposition)
        .body(Body::from_stream(
            stream.map(|chunk: Result<bytes::Bytes, std::io::Error>| chunk),
        ))
        .map_err(|_| invalid())?)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::{PermissionsExt, symlink};

    use axum::body::to_bytes;

    use super::*;
    use crate::action::{ActionResultMember, ActionResultV1};

    fn store() -> Arc<ActionFiles> {
        let mut store = ActionFiles::default();
        store.ttl = Duration::from_secs(10);
        store.active_timeout = Duration::from_secs(20);
        Arc::new(store)
    }

    fn upload(body: Body) -> Request {
        Request::builder()
            .method(Method::POST)
            .header("x-file-name", "r%C3%A9sum%C3%A9%20%2B.txt")
            .header(header::CONTENT_TYPE, "text/plain")
            .body(body)
            .unwrap()
    }

    async fn prepare(
        store: &Arc<ActionFiles>,
        rpc: &RpcContinuations,
        event: &Guid,
        root: &Path,
        path: &str,
    ) -> Guid {
        store
            .prepare_upload(
                rpc,
                event.clone(),
                root,
                path.to_owned(),
                UploadLimits::default(),
                None,
                Some(unsafe { libc::geteuid() }),
                Some(unsafe { libc::getegid() }),
            )
            .await
            .unwrap()
    }

    async fn request(rpc: &RpcContinuations, id: &Guid, req: Request) -> Result<Response, Error> {
        rpc.get_rest_handler(id).await.unwrap()(req).await
    }

    fn result(id: &Guid, delete: bool) -> ActionResult {
        ActionResult::V1(ActionResultV1 {
            title: "Done".into(),
            message: None,
            result: Some(ActionResultValue::Group {
                value: vec![ActionResultMember {
                    name: "Files".into(),
                    description: None,
                    value: ActionResultValue::Group {
                        value: vec![ActionResultMember {
                            name: "Download".into(),
                            description: None,
                            value: ActionResultValue::File {
                                value: id.clone(),
                                delete,
                            },
                        }],
                    },
                }],
            }),
        })
    }

    fn get() -> Request {
        Request::builder().body(Body::empty()).unwrap()
    }

    async fn expire(duration: Duration) {
        tokio::task::yield_now().await;
        tokio::time::advance(duration).await;
        tokio::task::yield_now().await;
    }

    async fn prepare_limited(
        store: &Arc<ActionFiles>,
        rpc: &RpcContinuations,
        event: &Guid,
        root: &Path,
        path: &str,
        max_size: Option<u64>,
    ) -> Guid {
        store
            .prepare_upload(
                rpc,
                event.clone(),
                root,
                path.to_owned(),
                UploadLimits { max_size },
                Some(0o640),
                Some(unsafe { libc::geteuid() }),
                Some(unsafe { libc::getegid() }),
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn size_limits_are_inclusive_allow_empty_and_default_to_unbounded() {
        let temp = tempfile::tempdir().unwrap();
        let store = store();
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        for (max, bytes) in [(Some(3), "abc"), (Some(0), ""), (None, "unbounded")] {
            let id = prepare_limited(&store, &rpc, &event, temp.path(), "file", max).await;
            assert_eq!(store.get_upload_limits(&event, &id).unwrap().max_size, max);
            let mut req = upload(Body::from(bytes));
            req.headers_mut()
                .insert(header::CONTENT_LENGTH, bytes.len().into());
            request(&rpc, &id, req).await.unwrap();
            assert_eq!(
                std::fs::read(temp.path().join("file")).unwrap(),
                bytes.as_bytes()
            );
            assert_eq!(
                store.resolve_upload(&event, &id).unwrap().commitment.size,
                bytes.len() as u64
            );
            assert!(store.get_upload_limits(&event, &id).is_err());
            assert!(store.resolve_upload(&event, &id).is_ok());
        }
    }

    #[tokio::test]
    async fn oversized_declared_length_is_rejected_before_creating_a_stage() {
        let temp = tempfile::tempdir().unwrap();
        let store = store();
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        let id = prepare_limited(&store, &rpc, &event, temp.path(), "file", Some(3)).await;
        // An unlinked parent cannot accept a stage.
        std::fs::remove_dir(temp.path()).unwrap();
        let req = Request::builder()
            .method(Method::POST)
            .header("x-file-name", "file")
            .header(header::CONTENT_LENGTH, "4")
            .body(Body::from_stream(futures::stream::poll_fn(
                |_| -> std::task::Poll<Option<Result<bytes::Bytes, std::io::Error>>> {
                    panic!("oversized body must not be polled")
                },
            )))
            .unwrap();
        let error = request(&rpc, &id, req).await.unwrap_err();
        assert!(
            error
                .to_string()
                .contains(&t!("service.action-files.size-limit", max = 3).to_string())
        );
        assert!(rpc.get_rest_handler(&id).await.is_none());
        assert!(store.resolve_upload(&event, &id).is_err());
    }

    #[tokio::test]
    async fn streamed_overflow_bounds_the_stage_and_rolls_back() {
        let temp = tempfile::tempdir().unwrap();
        let store = store();
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        for length in [None, Some("3"), Some("999")] {
            std::fs::write(temp.path().join("file"), b"previous").unwrap();
            let id = prepare_limited(&store, &rpc, &event, temp.path(), "file", Some(3)).await;
            let staged = Arc::new(Mutex::new(None::<File>));
            let inspect = staged.clone();
            let root = temp.path().to_owned();
            let body = async_stream::stream! {
                yield Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"ab"));
                let stage = std::fs::read_dir(&root).unwrap().map(|e| e.unwrap().path())
                    .find(|p| p.file_name().unwrap().to_string_lossy().starts_with(".startos-upload-")).unwrap();
                *inspect.lock().unwrap() = Some(File::open(stage).unwrap());
                yield Ok(bytes::Bytes::from_static(b"cd"));
                panic!("overflow must stop reading");
            };
            let mut req = upload(Body::from_stream(body));
            if let Some(length) = length {
                req.headers_mut()
                    .insert(header::CONTENT_LENGTH, length.parse().unwrap());
            }
            let error = request(&rpc, &id, req).await.unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains(&t!("service.action-files.size-limit", max = 3).to_string())
            );
            if length != Some("999") {
                // Tokio may leave the accepted prefix buffered.
                assert!(
                    staged
                        .lock()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .metadata()
                        .unwrap()
                        .len()
                        <= 2
                );
            } else {
                assert!(staged.lock().unwrap().is_none());
            }
            assert_eq!(
                std::fs::read(temp.path().join("file")).unwrap(),
                b"previous"
            );
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
            assert!(store.resolve_upload(&event, &id).is_err());
            assert!(rpc.get_rest_handler(&id).await.is_none());
        }
        let id = prepare_limited(&store, &rpc, &event, temp.path(), "file", Some(0)).await;
        assert!(request(&rpc, &id, upload(Body::from("x"))).await.is_err());
        assert_eq!(
            std::fs::read(temp.path().join("file")).unwrap(),
            b"previous"
        );
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn limits_query_is_scoped_to_pending_upload_service_event_and_expiry() {
        let temp = tempfile::tempdir().unwrap();
        let store = store();
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        let id = prepare_limited(&store, &rpc, &event, temp.path(), "file", Some(0)).await;
        assert_eq!(
            store.get_upload_limits(&event, &id).unwrap().max_size,
            Some(0)
        );
        assert!(store.get_upload_limits(&Guid::new(), &id).is_err());
        assert!(
            ActionFiles::default()
                .get_upload_limits(&event, &id)
                .is_err()
        );
        assert!(store.get_upload_limits(&event, &Guid::new()).is_err());
        std::fs::write(temp.path().join("download"), b"bytes").unwrap();
        let download = store
            .prepare_download(event.clone(), temp.path(), "download", None, None)
            .unwrap();
        assert!(store.get_upload_limits(&event, &download).is_err());
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(11)).await;
        assert!(store.get_upload_limits(&event, &id).is_err());
    }

    #[tokio::test]
    async fn directory_targets_preserve_uploaded_utf8_basename_and_file_attributes() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("directory")).unwrap();
        std::fs::set_permissions(
            temp.path().join("directory"),
            std::fs::Permissions::from_mode(0o750),
        )
        .unwrap();
        let directory_metadata = std::fs::metadata(temp.path().join("directory")).unwrap();
        let store = store();
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        for (destination, expected) in [
            ("directory", "directory/résumé +.txt"),
            ("directory/", "directory/résumé +.txt"),
            (".", "résumé +.txt"),
            ("", "résumé +.txt"),
        ] {
            let id = prepare_limited(&store, &rpc, &event, temp.path(), destination, Some(3)).await;
            request(&rpc, &id, upload(Body::from("abc"))).await.unwrap();
            let file = store.resolve_upload(&event, &id).unwrap();
            assert_eq!(file.path, expected);
            assert_eq!(file.name, "résumé +.txt");
            assert_eq!(std::fs::read(temp.path().join(expected)).unwrap(), b"abc");
            let metadata = std::fs::metadata(temp.path().join(expected)).unwrap();
            assert_eq!(metadata.mode() & 0o7777, 0o640);
            assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
            assert_eq!(metadata.gid(), unsafe { libc::getegid() });
        }
        let metadata = std::fs::metadata(temp.path().join("directory")).unwrap();
        assert_eq!(metadata.mode(), directory_metadata.mode());
        assert_eq!(metadata.uid(), directory_metadata.uid());
        assert_eq!(metadata.gid(), directory_metadata.gid());
    }

    #[tokio::test]
    async fn directory_child_collisions_and_untrusted_names_cannot_escape() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("directory")).unwrap();
        let child = temp.path().join("directory/résumé +.txt");
        let store = store();
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        for directory in [false, true] {
            let id = prepare(&store, &rpc, &event, temp.path(), "directory/").await;
            if directory {
                std::fs::create_dir(&child).unwrap();
            } else {
                symlink(outside.path().join("secret"), &child).unwrap();
            }
            assert!(
                request(&rpc, &id, upload(Body::from("bytes")))
                    .await
                    .is_err()
            );
            assert_eq!(
                std::fs::read_dir(temp.path().join("directory"))
                    .unwrap()
                    .count(),
                1
            );
            assert!(!outside.path().join("secret").exists());
            if directory {
                std::fs::remove_dir(&child).unwrap();
            } else {
                std::fs::remove_file(&child).unwrap();
            }
        }
        std::fs::write(&child, b"old").unwrap();
        let id = prepare(&store, &rpc, &event, temp.path(), "directory").await;
        request(&rpc, &id, upload(Body::from("replacement")))
            .await
            .unwrap();
        assert_eq!(std::fs::read(&child).unwrap(), b"replacement");
        for name in [
            ".",
            "..",
            "../escape",
            "%2Fabsolute",
            "x%5Cy",
            "%00",
            "%",
            "%FF",
            "",
        ] {
            let id = prepare(&store, &rpc, &event, temp.path(), "directory").await;
            let req = Request::builder()
                .method(Method::POST)
                .header("x-file-name", name)
                .body(Body::from("bad"))
                .unwrap();
            assert!(request(&rpc, &id, req).await.is_err());
            assert_eq!(std::fs::read(&child).unwrap(), b"replacement");
            assert_eq!(
                std::fs::read_dir(temp.path().join("directory"))
                    .unwrap()
                    .count(),
                1
            );
        }
    }

    #[tokio::test]
    async fn directory_descriptor_is_pinned_at_preparation() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("directory")).unwrap();
        let store = store();
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        let id = prepare(&store, &rpc, &event, temp.path(), "directory/").await;
        std::fs::rename(temp.path().join("directory"), temp.path().join("pinned")).unwrap();
        symlink(outside.path(), temp.path().join("directory")).unwrap();
        request(&rpc, &id, upload(Body::from("safe")))
            .await
            .unwrap();
        assert_eq!(
            std::fs::read(temp.path().join("pinned/résumé +.txt")).unwrap(),
            b"safe"
        );
        assert!(!outside.path().join("résumé +.txt").exists());
        assert_eq!(
            store.resolve_upload(&event, &id).unwrap().path,
            "directory/résumé +.txt"
        );
    }

    #[test]
    fn size_limit_errors_are_localized_in_all_supported_locales() {
        for locale in ["en_US", "de_DE", "es_ES", "fr_FR", "pl_PL"] {
            let message = t!(
                "service.action-files.size-limit",
                locale = locale,
                max = 123
            );
            assert!(message.contains("123"));
            assert_ne!(message, "service.action-files.size-limit");
            if locale != "en_US" {
                assert_ne!(
                    message,
                    t!(
                        "service.action-files.size-limit",
                        locale = "en_US",
                        max = 123
                    )
                );
            }
        }
    }

    #[tokio::test]
    async fn upload_installs_bytes_and_host_metadata_and_receipt_is_retryable() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("nested")).unwrap();
        let target = temp.path().join("nested/destination");
        std::fs::write(&target, b"previous").unwrap();
        let store = store();
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        let id = prepare(&store, &rpc, &event, temp.path(), "nested/destination").await;
        assert_eq!(std::fs::read(&target).unwrap(), b"previous");
        assert!(store.resolve_upload(&event, &id).is_err());
        let response = request(&rpc, &id, upload(Body::from("abc"))).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(std::fs::read(&target).unwrap(), b"abc");
        let file = store.resolve_upload(&event, &id).unwrap();
        assert_eq!(file.name, "résumé +.txt");
        assert_eq!(file.path, "nested/destination");
        assert_eq!(file.mime.as_deref(), Some("text/plain"));
        assert_eq!(file.commitment.size, 3);
        assert_eq!(
            file.commitment.hash,
            "ZDezrDhGUTP/tjt1JzqNtUjFWEZdedsD/TWcbNW9nYU="
        );
        assert_eq!(
            store.resolve_upload(&event, &id).unwrap().commitment.hash,
            file.commitment.hash
        );
        assert!(store.resolve_upload(&Guid::new(), &id).is_err());
        assert!(ActionFiles::default().resolve_upload(&event, &id).is_err());
        assert!(rpc.get_rest_handler(&id).await.is_none());
        let metadata = std::fs::metadata(&target).unwrap();
        assert_eq!(metadata.mode() & 0o7777, 0o600);
        assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
        assert_eq!(metadata.gid(), unsafe { libc::getegid() });
    }

    #[tokio::test]
    async fn defaults_are_logical_root_and_failed_ownership_rolls_back() {
        let temp = tempfile::tempdir().unwrap();
        let store = store();
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        std::fs::write(temp.path().join("file"), b"previous").unwrap();
        let id = store
            .prepare_upload(
                &rpc,
                event.clone(),
                temp.path(),
                "file".into(),
                UploadLimits::default(),
                None,
                None,
                None,
            )
            .await
            .unwrap();
        {
            let entries = store.entries.lock().unwrap();
            let Transfer::Upload(upload) = &entries.get(&id).unwrap().transfer else {
                panic!("expected upload");
            };
            assert_eq!((upload.mode, upload.uid, upload.gid), (0o600, 0, 0));
        }
        if unsafe { libc::geteuid() } != 0 {
            assert!(
                request(&rpc, &id, upload(Body::from("bytes")))
                    .await
                    .is_err()
            );
            assert_eq!(
                std::fs::read(temp.path().join("file")).unwrap(),
                b"previous"
            );
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
        }
        for (mode, uid, gid) in [
            (Some(0o10000), None, None),
            (None, Some(u32::MAX), None),
            (None, None, Some(u32::MAX)),
        ] {
            assert!(
                store
                    .prepare_upload(
                        &rpc,
                        event.clone(),
                        temp.path(),
                        "file".into(),
                        UploadLimits::default(),
                        mode,
                        uid,
                        gid
                    )
                    .await
                    .is_err()
            );
        }
    }

    #[tokio::test]
    async fn upload_honors_mode_and_does_not_trust_body_as_a_path() {
        let temp = tempfile::tempdir().unwrap();
        let store = store();
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        let id = store
            .prepare_upload(
                &rpc,
                event.clone(),
                temp.path(),
                "file".into(),
                UploadLimits::default(),
                Some(0o640),
                Some(unsafe { libc::geteuid() }),
                Some(unsafe { libc::getegid() }),
            )
            .await
            .unwrap();
        let mut req = upload(Body::from(
            r#"{"path":"/etc/passwd","commitment":{"hash":"forged","size":0}}"#,
        ));
        req.headers_mut().remove(header::CONTENT_TYPE);
        request(&rpc, &id, req).await.unwrap();
        let file = store.resolve_upload(&event, &id).unwrap();
        assert_eq!(file.path, "file");
        assert!(file.mime.is_none());
        assert_ne!(file.commitment.hash, "forged");
        assert_eq!(
            std::fs::metadata(temp.path().join("file"))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
            0o640
        );
    }

    #[tokio::test]
    async fn failed_and_truncated_uploads_preserve_target_and_remove_stage() {
        let temp = tempfile::tempdir().unwrap();
        let store = store();
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        std::fs::write(temp.path().join("file"), b"old").unwrap();
        let bodies = vec![
            upload(Body::from_stream(futures::stream::iter(vec![
                Ok(bytes::Bytes::from_static(b"partial")),
                Err(std::io::Error::other("broken body")),
            ]))),
            Request::builder()
                .method(Method::POST)
                .header("x-file-name", "name")
                .header(header::CONTENT_LENGTH, "99")
                .body(Body::from("short"))
                .unwrap(),
            Request::builder()
                .method(Method::POST)
                .header("x-file-name", "name")
                .header(header::CONTENT_LENGTH, "1")
                .body(Body::from("long"))
                .unwrap(),
        ];
        for req in bodies {
            let id = prepare(&store, &rpc, &event, temp.path(), "file").await;
            assert!(request(&rpc, &id, req).await.is_err());
            assert_eq!(std::fs::read(temp.path().join("file")).unwrap(), b"old");
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
            assert!(store.resolve_upload(&event, &id).is_err());
            assert!(rpc.get_rest_handler(&id).await.is_none());
        }
    }

    #[tokio::test]
    async fn paths_symlinks_and_nonregular_files_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), b"secret").unwrap();
        symlink(outside.path(), temp.path().join("link")).unwrap();
        symlink(outside.path().join("secret"), temp.path().join("symlink")).unwrap();
        std::fs::create_dir(temp.path().join("directory")).unwrap();
        nix::unistd::mkfifo(&temp.path().join("fifo"), nix::sys::stat::Mode::S_IRUSR).unwrap();
        let store = store();
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        for path in [
            "/absolute",
            "..",
            "../secret",
            "link/secret",
            "symlink",
            "fifo",
            "missing/file",
            "directory/../file",
            "directory//file",
            "directory/./file",
            "a\0b",
            "file/",
        ] {
            assert!(
                store
                    .prepare_upload(
                        &rpc,
                        event.clone(),
                        temp.path(),
                        path.to_owned(),
                        UploadLimits::default(),
                        None,
                        None,
                        None
                    )
                    .await
                    .is_err(),
                "upload accepted {path:?}"
            );
            assert!(
                store
                    .prepare_download(event.clone(), temp.path(), path, None, None)
                    .is_err(),
                "download accepted {path:?}"
            );
        }
        for path in ["", ".", "directory", "directory/"] {
            assert!(
                store
                    .prepare_download(event.clone(), temp.path(), path, None, None)
                    .is_err()
            );
        }
        let root_link = temp.path().join("root");
        symlink(outside.path(), &root_link).unwrap();
        assert!(Destination::new(&root_link, "secret").is_err());
        assert_eq!(
            std::fs::read(outside.path().join("secret")).unwrap(),
            b"secret"
        );
    }

    #[tokio::test]
    async fn parent_descriptor_survives_path_replacement_without_following_symlink() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("parent")).unwrap();
        let store = store();
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        let id = prepare(&store, &rpc, &event, temp.path(), "parent/file").await;
        std::fs::rename(temp.path().join("parent"), temp.path().join("pinned")).unwrap();
        symlink(outside.path(), temp.path().join("parent")).unwrap();
        request(&rpc, &id, upload(Body::from("safe")))
            .await
            .unwrap();
        assert_eq!(
            std::fs::read(temp.path().join("pinned/file")).unwrap(),
            b"safe"
        );
        assert!(!outside.path().join("file").exists());
    }

    #[tokio::test]
    async fn invalid_filename_and_wrong_method_consume_http_capability() {
        let temp = tempfile::tempdir().unwrap();
        let store = store();
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        for name in [
            "../bad",
            "%2Fetc%2Fpasswd",
            "%ZZ",
            "%FF",
            "bad%00name",
            "bad%0D%0Aname",
        ] {
            let id = prepare(&store, &rpc, &event, temp.path(), "file").await;
            let req = Request::builder()
                .method(Method::POST)
                .header("x-file-name", name)
                .body(Body::empty())
                .unwrap();
            assert!(request(&rpc, &id, req).await.is_err());
            assert!(rpc.get_rest_handler(&id).await.is_none());
            assert!(!temp.path().join("file").exists());
        }
        let id = prepare(&store, &rpc, &event, temp.path(), "file").await;
        assert_eq!(
            request(&rpc, &id, get()).await.unwrap().status(),
            StatusCode::METHOD_NOT_ALLOWED
        );
        assert!(rpc.get_rest_handler(&id).await.is_none());
    }

    #[tokio::test]
    async fn downloads_activate_only_on_matching_nested_result_and_stream_real_bytes() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("file"), b"download bytes").unwrap();
        let store = store();
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        let id = store
            .prepare_download(
                event.clone(),
                temp.path(),
                "file",
                Some("text/plain".into()),
                Some("résumé\r\n\".txt".into()),
            )
            .unwrap();
        assert!(rpc.get_rest_handler(&id).await.is_none());
        assert!(store.resolve_upload(&event, &id).is_err());
        assert!(
            store
                .publish(&rpc, &Guid::new(), &result(&id, true))
                .await
                .is_err()
        );
        assert!(
            Arc::new(ActionFiles::default())
                .publish(&rpc, &event, &result(&id, true))
                .await
                .is_err()
        );
        store
            .publish(&rpc, &event, &result(&id, true))
            .await
            .unwrap();
        assert!(
            store
                .publish(&rpc, &event, &result(&id, true))
                .await
                .is_err()
        );
        let response = request(&rpc, &id, get()).await.unwrap();
        assert!(temp.path().join("file").exists());
        assert_eq!(response.headers()[header::CONTENT_TYPE], "text/plain");
        assert_eq!(
            response.headers()[header::CONTENT_DISPOSITION],
            "attachment; filename=\"download\"; filename*=UTF-8''r%C3%A9sum%C3%A9%0D%0A%22.txt"
        );
        assert_eq!(
            to_bytes(response.into_body(), usize::MAX).await.unwrap(),
            "download bytes"
        );
        assert!(!temp.path().join("file").exists());
        assert!(rpc.get_rest_handler(&id).await.is_none());
    }

    #[tokio::test]
    async fn download_drop_cleans_up_but_never_unlinks_a_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let store = store();
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        std::fs::write(temp.path().join("file"), b"original").unwrap();
        let id = store
            .prepare_download(event.clone(), temp.path(), "file", None, None)
            .unwrap();
        store
            .publish(&rpc, &event, &result(&id, true))
            .await
            .unwrap();
        drop(request(&rpc, &id, get()).await.unwrap());
        assert!(!temp.path().join("file").exists());
        for replacement_is_symlink in [false, true] {
            std::fs::write(temp.path().join("file"), b"original").unwrap();
            let id = store
                .prepare_download(event.clone(), temp.path(), "file", None, None)
                .unwrap();
            store
                .publish(&rpc, &event, &result(&id, true))
                .await
                .unwrap();
            std::fs::rename(temp.path().join("file"), temp.path().join("original")).unwrap();
            if replacement_is_symlink {
                symlink("original", temp.path().join("file")).unwrap();
            } else {
                std::fs::write(temp.path().join("file"), b"replacement").unwrap();
            }
            let response = request(&rpc, &id, get()).await.unwrap();
            assert_eq!(
                to_bytes(response.into_body(), usize::MAX).await.unwrap(),
                "original"
            );
            assert!(std::fs::symlink_metadata(temp.path().join("file")).is_ok());
            assert_eq!(
                std::fs::read(temp.path().join("original")).unwrap(),
                b"original"
            );
            std::fs::remove_file(temp.path().join("file")).unwrap();
        }
    }

    #[tokio::test(start_paused = true)]
    async fn expiry_keeps_unpublished_and_delete_false_files_and_removes_armed_files() {
        let temp = tempfile::tempdir().unwrap();
        let store = store();
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        for (path, published, delete) in [
            ("unpublished", false, true),
            ("keep", true, false),
            ("remove", true, true),
        ] {
            std::fs::write(temp.path().join(path), b"bytes").unwrap();
            let id = store
                .prepare_download(event.clone(), temp.path(), path, None, None)
                .unwrap();
            if published {
                store
                    .publish(&rpc, &event, &result(&id, delete))
                    .await
                    .unwrap();
            }
        }
        expire(Duration::from_secs(11)).await;
        assert!(temp.path().join("unpublished").exists());
        assert!(temp.path().join("keep").exists());
        assert!(!temp.path().join("remove").exists());
        assert!(store.entries.lock().unwrap().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn active_timeout_deletes_an_unpolled_download_response() {
        let temp = tempfile::tempdir().unwrap();
        let store = store();
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        std::fs::write(temp.path().join("file"), b"bytes").unwrap();
        let id = store
            .prepare_download(event.clone(), temp.path(), "file", None, None)
            .unwrap();
        store
            .publish(&rpc, &event, &result(&id, true))
            .await
            .unwrap();
        let response = request(&rpc, &id, get()).await.unwrap();
        expire(Duration::from_secs(21)).await;
        assert!(!temp.path().join("file").exists());
        assert!(to_bytes(response.into_body(), usize::MAX).await.is_err());
        assert!(store.entries.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn teardown_cancels_active_download_without_retaining_store() {
        let temp = tempfile::tempdir().unwrap();
        let store = store();
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        for path in ["active", "pending", "unpublished"] {
            std::fs::write(temp.path().join(path), b"bytes").unwrap();
        }
        let id = store
            .prepare_download(event.clone(), temp.path(), "active", None, None)
            .unwrap();
        store
            .publish(&rpc, &event, &result(&id, true))
            .await
            .unwrap();
        let response = request(&rpc, &id, get()).await.unwrap();
        let pending = store
            .prepare_download(event.clone(), temp.path(), "pending", None, None)
            .unwrap();
        store
            .publish(&rpc, &event, &result(&pending, true))
            .await
            .unwrap();
        store
            .prepare_download(event, temp.path(), "unpublished", None, None)
            .unwrap();
        let weak = Arc::downgrade(&store);
        drop(store);
        assert!(weak.upgrade().is_none());
        tokio::task::yield_now().await;
        assert!(!temp.path().join("active").exists());
        assert!(!temp.path().join("pending").exists());
        assert!(temp.path().join("unpublished").exists());
        assert!(to_bytes(response.into_body(), usize::MAX).await.is_err());
        assert!(request(&rpc, &pending, get()).await.is_err());
    }

    #[tokio::test]
    async fn receipt_survives_preparation_expiry_and_eventually_expires_without_truncation() {
        let temp = tempfile::tempdir().unwrap();
        let store = store();
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        let id = prepare(&store, &rpc, &event, temp.path(), "uploaded").await;
        request(&rpc, &id, upload(Body::from("bytes")))
            .await
            .unwrap();
        tokio::time::pause();
        let pending = prepare(&store, &rpc, &event, temp.path(), "pending").await;
        expire(Duration::from_secs(11)).await;
        store.expire_entry(&id);
        assert!(store.resolve_upload(&event, &id).is_ok());
        assert!(rpc.get_rest_handler(&pending).await.is_none());
        expire(Duration::from_secs(20)).await;
        assert!(store.resolve_upload(&event, &id).is_err());
        assert_eq!(
            std::fs::read(temp.path().join("uploaded")).unwrap(),
            b"bytes"
        );
        assert!(!temp.path().join("pending").exists());
    }

    #[tokio::test]
    async fn completed_receipt_outlives_a_late_slow_sibling_upload() {
        let temp = tempfile::tempdir().unwrap();
        let mut state = ActionFiles::default();
        state.ttl = Duration::from_secs(100);
        state.active_timeout = Duration::from_secs(200);
        let store = Arc::new(state);
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        let small = prepare(&store, &rpc, &event, temp.path(), "small").await;
        let large = prepare(&store, &rpc, &event, temp.path(), "large").await;
        request(&rpc, &small, upload(Body::from("small")))
            .await
            .unwrap();

        tokio::time::pause();
        expire(Duration::from_secs(90)).await;
        tokio::time::resume();
        let (started, receive_started) = tokio::sync::oneshot::channel();
        let (finish, receive_finish) = tokio::sync::oneshot::channel();
        let body = async_stream::stream! {
            yield Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"first"));
            started.send(()).unwrap();
            receive_finish.await.unwrap();
            yield Ok(bytes::Bytes::from_static(b"last"));
        };
        let handler = rpc.get_rest_handler(&large).await.unwrap();
        let sibling = tokio::spawn(handler(upload(Body::from_stream(body))));
        receive_started.await.unwrap();
        tokio::time::pause();
        expire(Duration::from_secs(190)).await;
        assert!(!sibling.is_finished());
        assert_eq!(
            store
                .resolve_upload(&event, &small)
                .unwrap()
                .commitment
                .size,
            5
        );
        tokio::time::resume();
        finish.send(()).unwrap();
        assert_eq!(
            sibling.await.unwrap().unwrap().status(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            store
                .resolve_upload(&event, &large)
                .unwrap()
                .commitment
                .size,
            9
        );
        assert!(store.resolve_upload(&event, &small).is_ok());
    }

    #[tokio::test]
    async fn cancellation_and_timeout_roll_back_an_incomplete_upload() {
        for cancel_service in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let mut state = ActionFiles::default();
            state.active_timeout = Duration::from_millis(50);
            let store = Arc::new(state);
            let rpc = RpcContinuations::new();
            let event = Guid::new();
            std::fs::write(temp.path().join("file"), b"previous").unwrap();
            let id = prepare(&store, &rpc, &event, temp.path(), "file").await;
            let (started, receive) = tokio::sync::oneshot::channel();
            let body = async_stream::stream! {
                yield Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"partial"));
                started.send(()).ok();
                futures::future::pending::<()>().await;
            };
            let handler = rpc.get_rest_handler(&id).await.unwrap();
            let task = tokio::spawn(handler(upload(Body::from_stream(body))));
            receive.await.unwrap();
            assert!(store.resolve_upload(&event, &id).is_err());
            assert!(store.get_upload_limits(&event, &id).is_err());
            assert_eq!(
                std::fs::read(temp.path().join("file")).unwrap(),
                b"previous"
            );
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 2);
            if cancel_service {
                drop(store);
            }
            assert!(task.await.unwrap().is_err());
            assert_eq!(
                std::fs::read(temp.path().join("file")).unwrap(),
                b"previous"
            );
            assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
        }
    }

    #[tokio::test]
    async fn upload_tokens_cannot_be_published_as_downloads() {
        let temp = tempfile::tempdir().unwrap();
        let store = store();
        let rpc = RpcContinuations::new();
        let event = Guid::new();
        let id = prepare(&store, &rpc, &event, temp.path(), "file").await;
        assert!(
            store
                .publish(&rpc, &event, &result(&id, true))
                .await
                .is_err()
        );
        request(&rpc, &id, upload(Body::from("bytes")))
            .await
            .unwrap();
        assert!(
            store
                .publish(&rpc, &event, &result(&id, true))
                .await
                .is_err()
        );
        assert!(store.resolve_upload(&event, &id).is_ok());
    }
}
