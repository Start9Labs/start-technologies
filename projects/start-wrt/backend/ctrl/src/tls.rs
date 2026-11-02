//! Certificates for the HTTPS listener: the local CA issues one per name the
//! router is reached by.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;

use imbl_value::InternedString;
use patch_db::value::index::Index;
use patch_db::ModelExt;
use rustls::crypto::CryptoProvider;
use rustls::server::ClientHello;
use serde::{Deserialize, Serialize};
use startos::db::{DbAccess, DbAccessMut};
use startos::net::ssl::{cert_is_unexpired, root_ca_config, CertStore};
use startos::net::tls::{TlsHandler, TlsHandlerAction};
use startos::net::web_server::{extract, Accept, ExtractVisitor, TcpMetadata};
use startos::prelude::{HasModel, Model, PatchDb, TypedPatchDb};
use startos::util::serde::{Der64, Pem};
use visit_rs::Visit;

use crate::prelude::*;
use crate::ssl::{read_ca_chain, startwrt_branding, ROUTER_HOSTNAME};
use crate::{captive, ServerContext};

/// Holds private keys; the directory alone is readable by root.
const CERT_DIR: &str = "/etc/startwrt/certs";
const CERT_DB: &str = "certs.db";

/// Deriving `HasModel` emits an inherent `impl` on `Model`, allowed only in
/// start-core.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertDatabase {
    local: CertStore,
}
impl HasModel for CertDatabase {
    type Model = Model<Self>;
}

const LOCAL: &str = "local";

impl DbAccess<CertStore> for CertDatabase {
    fn access(db: &Model<Self>) -> &Model<CertStore> {
        db.transmute_ref(|v| LOCAL.index_into(v).unwrap_or(&patch_db::value::NULL))
    }
}
impl DbAccessMut<CertStore> for CertDatabase {
    fn access_mut(db: &mut Model<Self>) -> &mut Model<CertStore> {
        db.transmute_mut(|v| LOCAL.index_or_insert(v))
    }
}

/// The local CA from `/etc/ssl`, with no leaves.
fn local_store() -> Result<CertStore, Error> {
    let ((root_key, root_cert), (int_key, int_cert)) = read_ca_chain()?;
    Ok(CertStore {
        root_key: Der64::new(root_key),
        root_cert: Pem::new(root_cert),
        int_key: Der64::new(int_key),
        int_cert: Pem::new(int_cert),
        leaves: BTreeMap::new(),
    })
}

fn same_ca(a: &CertStore, b: &CertStore) -> Result<bool, Error> {
    Ok(a.root_cert.0.to_der()? == b.root_cert.0.to_der()?
        && a.int_cert.0.to_der()? == b.int_cert.0.to_der()?)
}

/// Keeps the stored leaves while the CA is unchanged, dropping expired ones.
fn reconcile_local(stored: CertStore, fresh: CertStore) -> Result<CertStore, Error> {
    if !same_ca(&stored, &fresh)? {
        return Ok(fresh);
    }
    let mut store = stored;
    store
        .leaves
        .retain(|_, leaf| cert_is_unexpired(&leaf.certs.nistp256).unwrap_or(false));
    Ok(store)
}

/// An unreadable store is set aside and replaced by an empty one.
async fn open_or_replace(path: &Path, ca: CertStore) -> Result<TypedPatchDb<CertDatabase>, Error> {
    match open_store(path, ca.clone()).await {
        Ok(db) => Ok(db),
        Err(e) => {
            tracing::error!("{e}; starting a new certificate store");
            tokio::fs::rename(path, path.with_extension("db.unreadable")).await?;
            open_store(path, ca).await
        }
    }
}

/// Keeps the stored leaves only while the CA is unchanged.
async fn open_store(path: &Path, ca: CertStore) -> Result<TypedPatchDb<CertDatabase>, Error> {
    let db = PatchDb::open(path).await.map_err(|e| {
        Error::new(
            eyre!("Failed to open {}: {e}", path.display()),
            ErrorKind::Filesystem,
        )
    })?;
    let fresh = ca.clone();
    let db =
        TypedPatchDb::<CertDatabase>::load_or_init(db, || async { Ok(CertDatabase { local: ca }) })
            .await?;
    db.mutate(|db| {
        let local = <CertDatabase as DbAccessMut<CertStore>>::access_mut(db);
        let store = reconcile_local(local.de()?, fresh)?;
        local.ser(&store)
    })
    .await
    .result?;
    Ok(db)
}

/// Opens the certificate store.
pub async fn init(ctx: ServerContext) -> Result<RouterTlsHandler, Error> {
    let dir = Path::new(CERT_DIR);
    tokio::fs::create_dir_all(dir).await?;
    tokio::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).await?;
    Ok(RouterTlsHandler {
        db: open_or_replace(&dir.join(CERT_DB), local_store()?).await?,
        ctx,
        crypto_provider: Arc::new(rustls::crypto::ring::default_provider()),
    })
}

fn http_alpn() -> Vec<Vec<u8>> {
    vec![b"h2".to_vec(), b"http/1.1".to_vec()]
}

/// Serves `router.lan` and the DDNS hostname by name, and any other
/// connection by the router address it reached.
#[derive(Clone)]
pub struct RouterTlsHandler {
    db: TypedPatchDb<CertDatabase>,
    ctx: ServerContext,
    crypto_provider: Arc<CryptoProvider>,
}

impl<'a, A> TlsHandler<'a, A> for RouterTlsHandler
where
    A: Accept + 'a,
    <A as Accept>::Metadata: Visit<ExtractVisitor<TcpMetadata>> + Clone + Send + Sync + 'static,
{
    async fn get_config(
        &'a mut self,
        hello: &'a ClientHello<'a>,
        metadata: &'a <A as Accept>::Metadata,
    ) -> Option<TlsHandlerAction> {
        if captive::is_active() && !hello.server_name().is_none_or(captive::serves) {
            return None;
        }
        let ddns = match hello.server_name() {
            Some(name) if !name.eq_ignore_ascii_case(ROUTER_HOSTNAME) => {
                self.ctx.ddns_identity().await
            }
            _ => None,
        };
        let dialed = extract::<TcpMetadata, _>(metadata).map(|tcp| tcp.local_addr.ip());
        let name = served(hello.server_name(), ddns.as_ref(), dialed);
        let mut cfg = root_ca_config(
            &self.db,
            &self.crypto_provider,
            &startwrt_branding(""),
            hello,
            [name].into(),
        )
        .await?;
        cfg.alpn_protocols = http_alpn();
        Some(TlsHandlerAction::Tls(cfg))
    }
}

/// `router.lan` and the DDNS hostname are served by name. Any other name, or
/// none, is served by the router address the connection reached.
fn served(
    requested: Option<&str>,
    ddns: Option<&InternedString>,
    dialed: Option<IpAddr>,
) -> InternedString {
    let requested = requested
        .map(|name| InternedString::intern(name.trim_end_matches('.').to_ascii_lowercase()));
    match requested {
        Some(name) if &*name == ROUTER_HOSTNAME || ddns == Some(&name) => name,
        _ => match dialed {
            Some(ip) => InternedString::from_display(&ip.to_canonical()),
            None => InternedString::intern(ROUTER_HOSTNAME),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use openssl::pkey::PKey;
    use startos::net::ssl::{gen_nistp256, make_int_cert, make_root_cert};

    use super::*;

    fn store() -> CertStore {
        let branding = startwrt_branding("test");
        let root_key = gen_nistp256().unwrap();
        let root_cert = make_root_cert(&root_key, &branding, std::time::SystemTime::now()).unwrap();
        let int_key = gen_nistp256().unwrap();
        let int_cert = make_int_cert((&root_key, &root_cert), &int_key, &branding).unwrap();
        CertStore {
            root_key: Der64::new(root_key),
            root_cert: Pem::new(root_cert),
            int_key: Der64::new(int_key),
            int_cert: Pem::new(int_cert),
            leaves: BTreeMap::new(),
        }
    }

    /// Same CA, separately loaded.
    fn copy(store: &CertStore) -> CertStore {
        let key = |k: &PKey<openssl::pkey::Private>| {
            PKey::private_key_from_der(&k.private_key_to_der().unwrap()).unwrap()
        };
        CertStore {
            root_key: Der64::new(key(&store.root_key.0)),
            root_cert: Pem::new(store.root_cert.0.clone()),
            int_key: Der64::new(key(&store.int_key.0)),
            int_cert: Pem::new(store.int_cert.0.clone()),
            leaves: BTreeMap::new(),
        }
    }

    async fn db_with(local: CertStore) -> TypedPatchDb<CertDatabase> {
        TypedPatchDb::load_or_init(PatchDb::in_memory(), || async {
            Ok::<_, startos::Error>(CertDatabase { local })
        })
        .await
        .unwrap()
    }

    async fn issue(db: &TypedPatchDb<CertDatabase>, name: &str) -> (Vec<String>, Vec<Vec<u8>>) {
        let hostnames: BTreeSet<InternedString> = [InternedString::intern(name)].into();
        let cert = db
            .mutate(|db| {
                <CertDatabase as DbAccessMut<CertStore>>::access_mut(db)
                    .cert_for(&hostnames, &startwrt_branding(""))
            })
            .await
            .result
            .unwrap();
        let sans = cert.leaf.certs.nistp256.subject_alt_names().unwrap();
        (
            sans.iter()
                .filter_map(|n| n.dnsname().map(str::to_string))
                .collect(),
            sans.iter()
                .filter_map(|n| n.ipaddress().map(<[u8]>::to_vec))
                .collect(),
        )
    }

    async fn stored(db: &TypedPatchDb<CertDatabase>) -> CertStore {
        <CertDatabase as DbAccess<CertStore>>::access(&db.peek().await)
            .de()
            .unwrap()
    }

    #[test]
    fn served_names() {
        let ddns = InternedString::intern("home.example.com");
        let v4: IpAddr = "192.168.20.1".parse().unwrap();
        let mapped: IpAddr = "::ffff:192.168.20.1".parse().unwrap();
        let local = |name: &str| InternedString::intern(name);

        assert_eq!(
            served(Some("Router.LAN."), Some(&ddns), Some(v4)),
            local(ROUTER_HOSTNAME)
        );
        assert_eq!(
            served(Some("HOME.example.com."), Some(&ddns), Some(v4)),
            ddns.clone()
        );
        assert_eq!(
            served(Some("home.example.com"), None, Some(v4)),
            local("192.168.20.1")
        );
        assert_eq!(
            served(Some("attacker.example"), Some(&ddns), Some(v4)),
            local("192.168.20.1")
        );
        assert_eq!(
            served(None, Some(&ddns), Some(mapped)),
            local("192.168.20.1")
        );
        assert_eq!(
            served(None, None, Some("fd13:6d5c:b354:12::1".parse().unwrap())),
            local("fd13:6d5c:b354:12::1")
        );
        assert_eq!(served(None, None, None), local(ROUTER_HOSTNAME));
    }

    #[tokio::test]
    async fn leaf_names_only_the_name_asked_for() {
        let db = db_with(store()).await;
        assert_eq!(
            issue(&db, "192.168.20.1").await,
            (vec![], vec![vec![192, 168, 20, 1]])
        );
        assert_eq!(
            issue(&db, ROUTER_HOSTNAME).await,
            (vec![ROUTER_HOSTNAME.to_string()], vec![])
        );
    }

    #[tokio::test]
    async fn reconcile_keeps_leaves_under_the_same_ca() {
        let db = db_with(store()).await;
        issue(&db, "192.168.0.1").await;
        let stored = stored(&db).await;
        let fresh = copy(&stored);
        assert_eq!(reconcile_local(stored, fresh).unwrap().leaves.len(), 1);
    }

    #[tokio::test]
    async fn reconcile_drops_leaves_from_another_ca() {
        let db = db_with(store()).await;
        issue(&db, "192.168.0.1").await;
        let reconciled = reconcile_local(stored(&db).await, store()).unwrap();
        assert!(reconciled.leaves.is_empty());
    }

    #[tokio::test]
    async fn store_survives_reopen_and_replaces_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CERT_DB);
        let base = store();

        let db = open_or_replace(&path, copy(&base)).await.unwrap();
        issue(&db, "192.168.0.1").await;
        drop(db);
        let db = open_or_replace(&path, copy(&base)).await.unwrap();
        assert_eq!(stored(&db).await.leaves.len(), 1);
        drop(db);

        std::fs::write(&path, b"not a database").unwrap();
        let db = open_or_replace(&path, copy(&base)).await.unwrap();
        assert!(stored(&db).await.leaves.is_empty());
        assert!(path.with_extension("db.unreadable").exists());
    }
}
