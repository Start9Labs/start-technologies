use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::net::{TcpStream, UnixListener, UnixStream};
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::db::model::Database;
use crate::disk::mount::filesystem::bind::Bind;
use crate::disk::mount::filesystem::{ReadOnly, ReadWrite};
use crate::disk::mount::guard::MountGuard;
use crate::net::vhost::{ConnRegistry, MAX_PROXY_CONNS_PER_TARGET, ProxyContext, ProxyTarget};
use crate::net::web_server::AcceptStream;
use crate::util::future::NonDetachingJoinHandle;
use crate::util::io::{BackTrackingIO, TmpDir};
use crate::util::sync::Watch;

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderState {
    running: bool,
    profiles: BTreeMap<InternedString, BTreeSet<HttpProxyProtocol>>,
}

pub(crate) struct HttpProxySockets {
    root: Arc<TmpDir>,
    pub providers: Watch<BTreeMap<PackageId, ProviderState>>,
    _watcher: NonDetachingJoinHandle<()>,
}

impl HttpProxySockets {
    pub async fn new(db: TypedPatchDb<Database>) -> Result<Self, Error> {
        let root = Arc::new(TmpDir::new().await?);
        let providers = Watch::new(BTreeMap::new());
        let published = providers.clone();
        let watcher = tokio::spawn(async move {
            let mut watch = db.watch("/public/packageData".parse().unwrap()).await;
            loop {
                let snapshot: Result<AllPackageData, Error> =
                    (|| Ok(imbl_value::from_value(watch.peek_and_mark_seen()?)?))();
                let state = match snapshot {
                    Ok(packages) => provider_states(&packages),
                    Err(error) => {
                        tracing::error!(?error, "Failed to read HTTP proxy providers");
                        BTreeMap::new()
                    }
                };
                published.send_if_modified(|previous| {
                    if *previous == state {
                        return false;
                    }
                    *previous = state;
                    true
                });
                if watch.changed().await.is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            root,
            providers,
            _watcher: watcher.into(),
        })
    }

    pub fn provider_root(&self, provider: &PackageId) -> PathBuf {
        self.root.join(base32::encode(
            base32::Alphabet::Rfc4648 { padding: false },
            &blake3::hash(provider.as_bytes()).as_bytes()[..16],
        ))
    }

    async fn prepare_provider(&self, provider: &PackageId) -> Result<PathBuf, Error> {
        let root = self.provider_root(provider);
        tokio::fs::create_dir_all(root.join("in")).await?;
        tokio::fs::create_dir_all(root.join("next")).await?;
        Ok(root)
    }

    pub async fn mount_provider(
        &self,
        provider: &PackageId,
        rootfs: &Path,
    ) -> Result<Vec<MountGuard>, Error> {
        let root = self.prepare_provider(provider).await?;
        let ingress = root.join("in");
        std::os::unix::fs::chown(&ingress, Some(100000), Some(100000))?;
        tokio::fs::set_permissions(&ingress, std::fs::Permissions::from_mode(0o770)).await?;
        let target = rootfs.join(CONTAINER_SOCKET_DIR.trim_start_matches('/'));
        let outer = MountGuard::mount(&Bind::new(&root), &target, ReadOnly).await?;
        let writable = MountGuard::mount(&Bind::new(ingress), target.join("in"), ReadWrite).await?;
        Ok(vec![outer, writable])
    }
}

fn provider_states(packages: &AllPackageData) -> BTreeMap<PackageId, ProviderState> {
    packages
        .0
        .iter()
        .filter_map(|(id, package)| {
            let registration = registration_for(package)?;
            Some((
                id.clone(),
                ProviderState {
                    running: package.status_info.desired.running()
                        && package.status_info.started.is_some(),
                    profiles: registration
                        .profiles
                        .iter()
                        .map(|profile| (profile.id.clone(), profile.protocols.clone()))
                        .collect(),
                },
            ))
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SelectedProfile {
    running: bool,
    protocols: BTreeSet<HttpProxyProtocol>,
}

fn selected_profiles(
    stages: &[HttpProxyStage],
    providers: &BTreeMap<PackageId, ProviderState>,
) -> Vec<Option<SelectedProfile>> {
    stages
        .iter()
        .map(|stage| {
            let provider = providers.get(&stage.package_id)?;
            Some(SelectedProfile {
                running: provider.running,
                protocols: provider.profiles.get(&stage.profile_id)?.clone(),
            })
        })
        .collect()
}

/// Refuses a symlink, which the provider can plant in its writable `in` directory.
async fn connect_ingress(path: &Path) -> std::io::Result<UnixStream> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};

    let socket = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    if !socket.metadata()?.file_type().is_socket() {
        return Err(std::io::Error::other("HTTP proxy ingress is not a socket"));
    }
    UnixStream::connect(format!("/proc/self/fd/{}", socket.as_raw_fd())).await
}

#[derive(Debug)]
pub(crate) struct ChainTarget {
    head: Option<PathBuf>,
    pub protocols: BTreeSet<HttpProxyProtocol>,
    pub cancel: CancellationToken,
}

impl ChainTarget {
    pub async fn connect(&self) -> AcceptStream {
        if let Some(head) = &self.head {
            if let Ok(Ok(stream)) = tokio::time::timeout(
                crate::net::vhost::BACKEND_DIAL_TIMEOUT,
                connect_ingress(head),
            )
            .await
            {
                return Box::pin(stream);
            }
        }
        crate::net::http::proxy_status_stream(http::StatusCode::SERVICE_UNAVAILABLE)
    }
}

pub(crate) struct ChainRoute {
    stages: Vec<HttpProxyStage>,
    providers: Vec<Option<SelectedProfile>>,
    upstream: ProxyTarget,
    pub target: Arc<ChainTarget>,
    listeners: Vec<NonDetachingJoinHandle<()>>,
    paths: Vec<PathBuf>,
}

impl std::fmt::Debug for ChainRoute {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ChainRoute")
            .field("stages", &self.stages)
            .field("target", &self.target)
            .finish_non_exhaustive()
    }
}

impl ChainRoute {
    pub fn matches(
        &self,
        stages: &[HttpProxyStage],
        providers: &BTreeMap<PackageId, ProviderState>,
        upstream: &ProxyTarget,
    ) -> bool {
        self.stages == stages
            && self.upstream == *upstream
            && self.providers == selected_profiles(stages, providers)
    }

    pub async fn new(
        sockets: &HttpProxySockets,
        binding: &HttpProxyBinding,
        stages: &[HttpProxyStage],
        upstream: ProxyTarget,
    ) -> Self {
        let providers = sockets
            .providers
            .peek(|providers| selected_profiles(stages, providers));
        let protocols = common_protocols(
            providers
                .iter()
                .map(|profile| profile.as_ref().map(|profile| &profile.protocols)),
        );
        let available = !protocols.is_empty()
            && providers
                .iter()
                .all(|provider| provider.as_ref().is_some_and(|provider| provider.running));
        let mut route = Self {
            stages: stages.to_vec(),
            providers,
            upstream,
            target: Arc::new(ChainTarget {
                head: None,
                protocols: if protocols.is_empty() {
                    HttpProxyProtocol::all()
                } else {
                    protocols
                },
                cancel: CancellationToken::new(),
            }),
            listeners: Vec::new(),
            paths: Vec::new(),
        };
        if available {
            match route.start(sockets, binding).await {
                Ok(head) => Arc::get_mut(&mut route.target).unwrap().head = Some(head),
                Err(error) => {
                    tracing::error!(?error, ?binding, "Failed to start HTTP proxy chain");
                    route.listeners.clear();
                    for path in route.paths.drain(..) {
                        let _ = std::fs::remove_file(path);
                    }
                }
            }
        }
        route
    }

    async fn start(
        &mut self,
        sockets: &HttpProxySockets,
        binding: &HttpProxyBinding,
    ) -> Result<PathBuf, Error> {
        let mut next = NextHop::Application {
            address: self.upstream.addr,
            tls: self.upstream.connect_ssl.clone(),
        };
        for stage in self.stages.iter().rev() {
            let root = sockets.prepare_provider(&stage.package_id).await?;
            let path = binding.socket_path(&root, "next", &stage.profile_id);
            match tokio::fs::remove_file(&path).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            let listener = UnixListener::bind(&path)?;
            self.paths.push(path.clone());
            tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).await?;
            let context = ProxyContext {
                cancel: self.target.cancel.child_token(),
                registry: ConnRegistry::new(MAX_PROXY_CONNS_PER_TARGET),
            };
            let hop = next;
            let worker = tokio::spawn(async move {
                loop {
                    let accepted = tokio::select! {
                        _ = context.cancel.cancelled() => break,
                        accepted = listener.accept() => accepted,
                    };
                    let Ok((stream, _)) = accepted else {
                        continue;
                    };
                    let (stream, registration, cancel) = context.track_with(Box::pin(stream));
                    let hop = hop.clone();
                    tokio::spawn(async move {
                        let _registration = registration;
                        tokio::select! {
                            _ = cancel.cancelled() => {},
                            result = hop.relay(stream) => {
                                if let Err(error) = result { tracing::trace!(?error, "HTTP proxy continuation failed"); }
                            }
                        }
                    });
                }
            });
            self.listeners.push(worker.into());
            next = NextHop::Unix(binding.socket_path(&root, "in", &stage.profile_id));
        }
        let NextHop::Unix(head) = next else {
            unreachable!("a chain has a stage")
        };
        Ok(head)
    }
}

impl Drop for ChainRoute {
    fn drop(&mut self) {
        self.target.cancel.cancel();
        self.listeners.clear();
        for path in &self.paths {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[derive(Clone)]
enum NextHop {
    Unix(PathBuf),
    Application {
        address: SocketAddr,
        tls: Option<Arc<ClientConfig>>,
    },
}

impl NextHop {
    async fn relay(&self, from: AcceptStream) -> Result<(), Error> {
        let mut from = BackTrackingIO::new(from);
        let backend = async {
            match self {
                Self::Unix(path) => {
                    Ok::<AcceptStream, Error>(Box::pin(connect_ingress(path).await?))
                }
                Self::Application { address, tls } => {
                    let protocol = if tls.is_some() {
                        Some(
                            crate::net::http::read_http_protocol(&mut from)
                                .await?
                                .ok_or_else(|| {
                                    Error::new(eyre!("empty continuation"), ErrorKind::Network)
                                })?,
                        )
                    } else {
                        None
                    };
                    let stream = TcpStream::connect(address).await?;
                    if let Some(tls) = tls {
                        let protocol = protocol.unwrap();
                        let stream = TlsConnector::from(tls.clone())
                            .with_alpn(vec![protocol.alpn().to_vec()])
                            .connect(ServerName::IpAddress(address.ip().into()), stream)
                            .await?;
                        if protocol == HttpProxyProtocol::Http2
                            && stream.get_ref().1.alpn_protocol() != Some(b"h2".as_slice())
                        {
                            return Err(Error::new(
                                eyre!("backend did not negotiate HTTP/2"),
                                ErrorKind::Network,
                            ));
                        }
                        Ok::<AcceptStream, Error>(Box::pin(stream))
                    } else {
                        Ok::<AcceptStream, Error>(Box::pin(stream))
                    }
                }
            }
        };
        let mut backend = match tokio::time::timeout(Duration::from_secs(60), backend).await {
            Ok(Ok(backend)) => backend,
            _ => crate::net::http::proxy_status_stream(http::StatusCode::SERVICE_UNAVAILABLE),
        };
        let _ = from.rewind();
        tokio::io::copy_bidirectional(&mut from, &mut backend).await?;
        Ok(())
    }
}
